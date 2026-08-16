use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::approval::ApprovedPlan;
use crate::canonical::to_canonical_json;
use crate::error::{Result, VaultcError};
use crate::identity::{ContentHash, OperationId};
use crate::ir::SourceFile;
use crate::plan::OutputOperation;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProvenanceSource {
    pub source_id: String,
    pub snapshot_id: String,
    pub source_file_id: String,
    pub source_path: String,
    pub source_content_hash: ContentHash,
    pub source_document_id: Option<String>,
    pub evidence_content_hash: Option<ContentHash>,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProvenanceRecord {
    Output {
        output_path: String,
        output_hash: ContentHash,
        operation_id: String,
        source_snapshot_ids: Vec<String>,
        source_document_ids: Vec<String>,
        sources: Vec<ProvenanceSource>,
        proposal_id: Option<String>,
        evidence: Vec<vaultc_protocol::EvidenceRefWire>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceExplanation {
    pub output_path: String,
    pub records: Vec<ProvenanceRecord>,
}

// This is the single closure point for copied, deduplicated, and generated
// derivations; keeping the branches adjacent prevents provenance drift.
#[allow(clippy::too_many_lines)]
pub(crate) fn records_for_output(
    approved: &ApprovedPlan,
    output_hashes: &[(String, ContentHash)],
) -> Vec<ProvenanceRecord> {
    let mut records = Vec::new();
    for (path, hash) in output_hashes {
        if let Some(operation) = approved
            .plan
            .operations
            .iter()
            .find(|operation| operation.destination() == path)
        {
            let (operation_id, mut sources, source_document_ids) =
                match operation {
                    OutputOperation::Copy {
                        operation_id,
                        source_id,
                        source_path,
                        ..
                    }
                    | OutputOperation::RewriteMarkdown {
                        operation_id,
                        source_id,
                        source_path,
                        ..
                    }
                    | OutputOperation::RewriteCanvas {
                        operation_id,
                        source_id,
                        source_path,
                        ..
                    } => {
                        let source_document =
                            approved.plan.workspace.documents.values().find(|document| {
                                &document.source_file.source_id == source_id
                                    && &document.source_file.logical_path == source_path
                            });
                        let mut document_ids = Vec::new();
                        if let Some(document) = source_document {
                            document_ids.push(document.document_id);
                            for group in &approved.plan.exact_groups {
                                if group.members.contains(&document.document_id) {
                                    document_ids.extend(group.members.iter().copied());
                                }
                            }
                        }
                        document_ids.sort();
                        document_ids.dedup();
                        let mut sources: Vec<_> =
                            document_ids
                                .iter()
                                .filter_map(|document_id| {
                                    approved.plan.workspace.documents.get(document_id).map(
                                        |document| {
                                            provenance_source(
                                                &document.source_file,
                                                Some(document.document_id.to_string()),
                                                None,
                                                None,
                                                None,
                                            )
                                        },
                                    )
                                })
                                .collect();
                        if sources.is_empty()
                            && let Some(file) = find_source_file(approved, source_id, source_path)
                        {
                            sources.push(provenance_source(file, None, None, None, None));
                        }
                        if let Some(asset) = approved.plan.workspace.assets.values().find(|asset| {
                            asset.canonical_source().is_some_and(|source| {
                                &source.source_id == source_id
                                    && source.logical_path == *source_path
                            })
                        }) {
                            sources.extend(
                                asset.sources.iter().map(|source| {
                                    provenance_source(source, None, None, None, None)
                                }),
                            );
                        }
                        sources.sort();
                        sources.dedup();
                        let mut documents: Vec<_> = document_ids
                            .into_iter()
                            .map(|document_id| document_id.to_string())
                            .collect();
                        documents.sort();
                        documents.dedup();
                        (*operation_id, sources, documents)
                    }
                };
            sources.sort();
            sources.dedup();
            let mut snapshot_ids: Vec<_> = sources
                .iter()
                .map(|source| source.snapshot_id.clone())
                .collect();
            snapshot_ids.sort();
            snapshot_ids.dedup();
            records.push(ProvenanceRecord::Output {
                output_path: path.clone(),
                output_hash: *hash,
                operation_id: operation_id.to_string(),
                source_snapshot_ids: snapshot_ids,
                source_document_ids,
                sources,
                proposal_id: None,
                evidence: Vec::new(),
            });
        } else if let Some(proposal) = approved
            .approved_proposals
            .iter()
            .find(|proposal| generated_output_path(&proposal.proposal).as_deref() == Some(path))
        {
            let operation_id = generated_operation_id(&proposal.proposal.proposal_id);
            let mut snapshots: Vec<_> = proposal
                .proposal
                .evidence
                .iter()
                .map(|evidence| evidence.snapshot_id.clone())
                .collect();
            let mut documents: Vec<_> = proposal
                .proposal
                .evidence
                .iter()
                .map(|evidence| evidence.document_id.clone())
                .collect();
            snapshots.sort();
            snapshots.dedup();
            documents.sort();
            documents.dedup();
            let mut sources = Vec::new();
            for evidence in &proposal.proposal.evidence {
                let Ok(document_id) = evidence.document_id.parse::<crate::identity::DocumentId>()
                else {
                    continue;
                };
                let Some(document) = approved.plan.workspace.documents.get(&document_id) else {
                    continue;
                };
                let evidence_hash = ContentHash::parse_hex(&evidence.content_hash).ok();
                sources.push(provenance_source(
                    &document.source_file,
                    Some(document.document_id.to_string()),
                    evidence_hash,
                    evidence.byte_start,
                    evidence.byte_end,
                ));
            }
            sources.sort();
            sources.dedup();
            records.push(ProvenanceRecord::Output {
                output_path: path.clone(),
                output_hash: *hash,
                operation_id: operation_id.to_string(),
                source_snapshot_ids: snapshots,
                source_document_ids: documents,
                sources,
                proposal_id: Some(proposal.proposal.proposal_id.clone()),
                evidence: proposal.proposal.evidence.clone(),
            });
        }
    }
    records.sort_by(|left, right| output_path(left).cmp(output_path(right)));
    records
}

fn find_source_file<'a>(
    approved: &'a ApprovedPlan,
    source_id: &crate::source::SourceId,
    source_path: &str,
) -> Option<&'a SourceFile> {
    approved
        .plan
        .snapshots
        .iter()
        .find(|snapshot| &snapshot.source_id == source_id)
        .and_then(|snapshot| {
            snapshot
                .files
                .iter()
                .find(|file| file.logical_path == source_path)
        })
}

fn provenance_source(
    file: &SourceFile,
    source_document_id: Option<String>,
    evidence_content_hash: Option<ContentHash>,
    byte_start: Option<u64>,
    byte_end: Option<u64>,
) -> ProvenanceSource {
    ProvenanceSource {
        source_id: file.source_id.to_string(),
        snapshot_id: file.snapshot_id.to_string(),
        source_file_id: file.file_id.to_string(),
        source_path: file.logical_path.clone(),
        source_content_hash: file.content_hash,
        source_document_id,
        evidence_content_hash,
        byte_start,
        byte_end,
    }
}

pub(crate) fn generated_output_path(
    proposal: &vaultc_protocol::KnowledgeProposal,
) -> Option<String> {
    match &proposal.kind {
        vaultc_protocol::ProposalKind::CreateGeneratedNote {
            title,
            suggested_path,
            ..
        } => {
            let relative = suggested_path.clone().unwrap_or_else(|| {
                format!(
                    "{}.md",
                    sanitize_generated_name(title, &proposal.proposal_id)
                )
            });
            Some(format!("knowledge/_generated/{relative}"))
        }
        vaultc_protocol::ProposalKind::ExplainConflict { .. } => None,
    }
}

fn sanitize_generated_name(title: &str, proposal_id: &str) -> String {
    let mut name = title
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    while name.contains("--") {
        name = name.replace("--", "-");
    }
    name = name.trim_matches('-').to_owned();
    if name.is_empty() {
        name = "generated".into();
    }
    let suffix =
        ContentHash::from_domain_bytes("vaultc:proposal-path:v1\0", proposal_id.as_bytes());
    format!("{}~{}", name, &suffix.hex()[..8])
}

pub(crate) fn generated_operation_id(proposal_id: &str) -> OperationId {
    OperationId::from_parts("vaultc:generated-operation:v1\0", &[proposal_id.as_bytes()])
}

fn output_path(record: &ProvenanceRecord) -> &str {
    match record {
        ProvenanceRecord::Output { output_path, .. } => output_path,
    }
}

pub fn explain(artifact: &Path, requested_path: &str) -> Result<ProvenanceExplanation> {
    if artifact.is_file()
        && artifact
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("vaultpack"))
    {
        #[cfg(feature = "archives")]
        {
            let temporary =
                tempfile::tempdir().map_err(|error| VaultcError::io(artifact, error))?;
            crate::pack::extract_pack_safely(artifact, temporary.path())?;
            return explain_directory(temporary.path(), requested_path);
        }
        #[cfg(not(feature = "archives"))]
        {
            return Err(VaultcError::UnsupportedSource(artifact.to_path_buf()));
        }
    }
    explain_directory(artifact, requested_path)
}

fn explain_directory(artifact: &Path, requested_path: &str) -> Result<ProvenanceExplanation> {
    let path = artifact.join(".vaultc/provenance.jsonl");
    let text = fs::read_to_string(&path).map_err(|error| VaultcError::io(&path, error))?;
    let mut records = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: ProvenanceRecord =
            serde_json::from_str(line).map_err(|error| VaultcError::MalformedInput {
                path: path.display().to_string(),
                reason: format!("line {}: {error}", line_number + 1),
            })?;
        if output_path(&record) == requested_path {
            records.push(record);
        }
    }
    if records.is_empty() {
        return Err(VaultcError::VerificationFailed(format!(
            "no provenance found for `{requested_path}`"
        )));
    }
    Ok(ProvenanceExplanation {
        output_path: requested_path.into(),
        records,
    })
}

pub(crate) fn encode_jsonl(records: &[ProvenanceRecord]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    for record in records {
        output.extend(to_canonical_json(record)?);
        output.push(b'\n');
    }
    Ok(output)
}
