use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::config::DedupPolicy;
use crate::error::{Result, VaultcError};
use crate::identity::{ContentHash, DocumentId};
use crate::ir::Document;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactDuplicateGroup {
    pub canonical: DocumentId,
    pub members: Vec<DocumentId>,
    pub body_hash: ContentHash,
    pub frontmatter_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearDuplicateCandidate {
    pub left: DocumentId,
    pub right: DocumentId,
    pub estimated_similarity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DedupReport {
    pub exact_groups: Vec<ExactDuplicateGroup>,
    pub near_candidates: Vec<NearDuplicateCandidate>,
    #[serde(default)]
    pub truncated_documents: Vec<DocumentId>,
}

pub fn analyze<'a>(
    documents: impl IntoIterator<Item = &'a Document>,
    policy: &DedupPolicy,
) -> Result<DedupReport> {
    if policy.minhash_components == 0
        || policy.lsh_bands == 0
        || policy.rows_per_band == 0
        || policy.lsh_bands.checked_mul(policy.rows_per_band) != Some(policy.minhash_components)
        || u32::try_from(policy.minhash_components).is_err()
        || !policy.near_threshold.is_finite()
        || !(0.0..=1.0).contains(&policy.near_threshold)
    {
        return Err(VaultcError::InvalidConfig(
            "invalid MinHash/LSH deduplication policy".into(),
        ));
    }
    let documents: Vec<_> = documents.into_iter().collect();
    let mut exact_map: BTreeMap<(ContentHash, ContentHash), Vec<&Document>> = BTreeMap::new();
    for document in &documents {
        exact_map
            .entry((document.body_hash, document.frontmatter_hash))
            .or_default()
            .push(document);
    }

    let mut exact_groups = Vec::new();
    let mut representative_of = BTreeMap::new();
    for ((body_hash, frontmatter_hash), mut members) in exact_map {
        members.sort_by(|left, right| canonical_tuple(left).cmp(&canonical_tuple(right)));
        let canonical = members[0].document_id;
        for member in &members {
            representative_of.insert(member.document_id, canonical);
        }
        if members.len() > 1 {
            exact_groups.push(ExactDuplicateGroup {
                canonical,
                members: members
                    .iter()
                    .map(|document| document.document_id)
                    .collect(),
                body_hash,
                frontmatter_hash,
            });
        }
    }
    exact_groups.sort_by_key(|group| group.canonical);

    let representatives: Vec<_> = documents
        .iter()
        .copied()
        .filter(|document| representative_of[&document.document_id] == document.document_id)
        .collect();
    let (near_candidates, truncated_documents) = near_candidates(&representatives, policy);
    Ok(DedupReport {
        exact_groups,
        near_candidates,
        truncated_documents,
    })
}

fn canonical_tuple(document: &Document) -> (&[u8], &str, crate::identity::SnapshotId, DocumentId) {
    (
        document.source_file.logical_path.as_bytes(),
        document.source_file.source_id.as_str(),
        document.source_file.snapshot_id,
        document.document_id,
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "candidate generation keeps bounded LSH collection and deterministic reduction together"
)]
fn near_candidates(
    documents: &[&Document],
    policy: &DedupPolicy,
) -> (Vec<NearDuplicateCandidate>, Vec<DocumentId>) {
    let mut signatures = BTreeMap::new();
    let mut short_texts: BTreeMap<String, Vec<DocumentId>> = BTreeMap::new();
    for document in documents {
        let graphemes: Vec<&str> =
            UnicodeSegmentation::graphemes(document.comparison_text.as_str(), true).collect();
        if graphemes.len() < 5 {
            short_texts
                .entry(document.comparison_text.clone())
                .or_default()
                .push(document.document_id);
        } else {
            signatures.insert(
                document.document_id,
                minhash_signature(&graphemes, policy.minhash_components),
            );
        }
    }

    let mut pairs = BTreeSet::new();
    let mut pair_counts: BTreeMap<DocumentId, usize> = BTreeMap::new();
    let mut truncated = BTreeSet::new();
    let pair_limit = policy.max_candidates_per_document.saturating_mul(4).max(1);
    let mut buckets: BTreeMap<(usize, Vec<u64>), Vec<DocumentId>> = BTreeMap::new();
    for (document_id, signature) in &signatures {
        for band in 0..policy.lsh_bands {
            let start = band * policy.rows_per_band;
            let end = start + policy.rows_per_band;
            buckets
                .entry((band, signature[start..end].to_vec()))
                .or_default()
                .push(*document_id);
        }
    }
    let unique_buckets: BTreeSet<_> = buckets.into_values().collect();
    for members in &unique_buckets {
        add_bounded_pairs(
            members,
            pair_limit,
            &mut pairs,
            &mut pair_counts,
            &mut truncated,
        );
    }
    for members in short_texts.values() {
        add_bounded_pairs(
            members,
            pair_limit,
            &mut pairs,
            &mut pair_counts,
            &mut truncated,
        );
    }

    let mut scored = Vec::new();
    for (left, right) in pairs {
        let similarity = match (signatures.get(&left), signatures.get(&right)) {
            (Some(left_signature), Some(right_signature)) => {
                let matches = left_signature
                    .iter()
                    .zip(right_signature)
                    .filter(|(left, right)| left == right)
                    .count();
                #[allow(clippy::cast_precision_loss)]
                let score = matches as f64 / policy.minhash_components as f64;
                score
            }
            _ => 1.0,
        };
        if similarity >= policy.near_threshold {
            scored.push(NearDuplicateCandidate {
                left,
                right,
                estimated_similarity: similarity,
            });
        }
    }
    scored.sort_by(|left, right| {
        right
            .estimated_similarity
            .total_cmp(&left.estimated_similarity)
            .then_with(|| left.left.cmp(&right.left))
            .then_with(|| left.right.cmp(&right.right))
    });
    let mut accepted_per_document: BTreeMap<DocumentId, usize> = BTreeMap::new();
    let mut candidates = Vec::new();
    for candidate in scored {
        if accepted_per_document
            .get(&candidate.left)
            .copied()
            .unwrap_or_default()
            >= policy.max_candidates_per_document
            || accepted_per_document
                .get(&candidate.right)
                .copied()
                .unwrap_or_default()
                >= policy.max_candidates_per_document
        {
            truncated.insert(candidate.left);
            truncated.insert(candidate.right);
            continue;
        }
        *accepted_per_document.entry(candidate.left).or_default() += 1;
        *accepted_per_document.entry(candidate.right).or_default() += 1;
        candidates.push(candidate);
    }
    (candidates, truncated.into_iter().collect())
}

fn add_bounded_pairs(
    members: &[DocumentId],
    pair_limit: usize,
    pairs: &mut BTreeSet<(DocumentId, DocumentId)>,
    counts: &mut BTreeMap<DocumentId, usize>,
    truncated: &mut BTreeSet<DocumentId>,
) {
    for (left_index, &left) in members.iter().enumerate() {
        for &right in members.iter().skip(left_index + 1) {
            if counts.get(&left).copied().unwrap_or_default() >= pair_limit {
                truncated.insert(left);
                break;
            }
            if counts.get(&right).copied().unwrap_or_default() >= pair_limit {
                truncated.insert(right);
                continue;
            }
            let pair = ordered_pair(left, right);
            if pairs.insert(pair) {
                *counts.entry(left).or_default() += 1;
                *counts.entry(right).or_default() += 1;
            }
        }
    }
}

fn minhash_signature(graphemes: &[&str], components: usize) -> Vec<u64> {
    let mut shingles = BTreeSet::new();
    for window in graphemes.windows(5) {
        shingles.insert(window.concat());
    }
    let mut signature = vec![u64::MAX; components];
    for (component, minimum) in signature.iter_mut().enumerate() {
        let component = u32::try_from(component).unwrap_or(u32::MAX);
        for shingle in &shingles {
            let hash = ContentHash::from_parts(
                "vaultc:minhash:v1\0",
                &[&component.to_be_bytes(), shingle.as_bytes()],
            );
            let mut first = [0_u8; 8];
            first.copy_from_slice(&hash.as_bytes()[..8]);
            *minimum = (*minimum).min(u64::from_be_bytes(first));
        }
    }
    signature
}

fn ordered_pair(left: DocumentId, right: DocumentId) -> (DocumentId, DocumentId) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn threshold_boundary_matches_v1_vectors() {
        let threshold = 0.85;
        assert!(108.0 / 128.0 < threshold);
        assert!(109.0 / 128.0 >= threshold);
    }
}
