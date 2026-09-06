//! Schema 3 evidence-complete semantic integration and offline materialization.
//!
//! Provider clients do not belong here. This module accepts already-recorded
//! proposals as hostile data, validates the complete approval closure, and
//! emits deterministic files without network access.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::canonical::{canonical_hash, to_canonical_json, to_canonical_json_pretty};
use crate::error::{OkcError, Result};
use crate::identity::{BlockId, ContentHash, DocumentId};
use crate::source::SourceId;

pub const INTEGRATION_SCHEMA_VERSION: u32 = 3;
pub const PRODUCT_VERSION: &str = "0.3.0";
pub const PACK_PROFILE: &str = "okc-tar-zstd-deterministic-v3";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBlock {
    pub block_id: BlockId,
    pub content_hash: ContentHash,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataValue {
    pub metadata_id: String,
    pub key: String,
    pub value_index: u32,
    pub content_hash: ContentHash,
    pub value: Value,
}

impl MetadataValue {
    pub fn new(
        document_id: DocumentId,
        key: impl Into<String>,
        value_index: u32,
        value: &Value,
    ) -> Result<Self> {
        let key = key.into();
        validate_bounded_data("metadata key", &key, 1, 1_024)?;
        let identity = MetadataIdentityInput {
            document_id,
            key: &key,
            value_index,
            value,
        };
        let content_hash = canonical_hash("okc:metadata-value:v3\0", &identity)?;
        Ok(Self {
            metadata_id: format!(
                "metadata_{}",
                canonical_hash("okc:metadata-id:v3\0", &identity)?.hex()
            ),
            key,
            value_index,
            content_hash,
            value: value.clone(),
        })
    }
}

#[derive(Serialize)]
struct MetadataIdentityInput<'a> {
    document_id: DocumentId,
    key: &'a str,
    value_index: u32,
    value: &'a Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationDocument {
    pub source_id: SourceId,
    pub document_id: DocumentId,
    pub original_path: String,
    pub document_hash: ContentHash,
    pub blocks: Vec<SourceBlock>,
    pub metadata: Vec<MetadataValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationCorpus {
    pub schema_version: u32,
    pub corpus_hash: ContentHash,
    pub policy_hash: ContentHash,
    pub documents: Vec<IntegrationDocument>,
}

impl IntegrationCorpus {
    pub fn seal(policy_hash: ContentHash, mut documents: Vec<IntegrationDocument>) -> Result<Self> {
        documents.sort_by_key(|document| document.document_id);
        for document in &mut documents {
            document.blocks.sort_by_key(|block| block.block_id);
            document.metadata.sort_by(|left, right| {
                left.metadata_id
                    .as_bytes()
                    .cmp(right.metadata_id.as_bytes())
            });
        }
        let corpus_hash = canonical_hash("okc:integration-corpus:v3\0", &documents)?;
        let corpus = Self {
            schema_version: INTEGRATION_SCHEMA_VERSION,
            corpus_hash,
            policy_hash,
            documents,
        };
        validate_corpus(&corpus)?;
        Ok(corpus)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyCluster {
    pub cluster_id: String,
    pub title: String,
    /// Relative path below `knowledge/`.
    pub canonical_path: String,
    pub document_ids: Vec<DocumentId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyProposal {
    pub schema_version: u32,
    pub corpus_hash: ContentHash,
    pub taxonomy_hash: ContentHash,
    pub clusters: Vec<TaxonomyCluster>,
    pub organizer_recording_hash: ContentHash,
}

impl TaxonomyProposal {
    pub fn seal(
        corpus: &IntegrationCorpus,
        mut clusters: Vec<TaxonomyCluster>,
        organizer_recording_hash: ContentHash,
    ) -> Result<Self> {
        clusters.sort_by(|left, right| left.cluster_id.as_bytes().cmp(right.cluster_id.as_bytes()));
        for cluster in &mut clusters {
            cluster.document_ids.sort();
        }
        let identity = TaxonomyIdentity {
            corpus_hash: corpus.corpus_hash,
            clusters: &clusters,
            organizer_recording_hash,
        };
        let proposal = Self {
            schema_version: INTEGRATION_SCHEMA_VERSION,
            corpus_hash: corpus.corpus_hash,
            taxonomy_hash: canonical_hash("okc:taxonomy:v3\0", &identity)?,
            clusters,
            organizer_recording_hash,
        };
        validate_taxonomy(corpus, &proposal)?;
        Ok(proposal)
    }
}

#[derive(Serialize)]
struct TaxonomyIdentity<'a> {
    corpus_hash: ContentHash,
    clusters: &'a [TaxonomyCluster],
    organizer_recording_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalBinding {
    pub target_hash: ContentHash,
    pub approved: bool,
    pub curator_id: String,
    pub policy_version: String,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispositionTargetKind {
    Block,
    Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispositionTarget {
    pub kind: DispositionTargetKind,
    pub document_id: DocumentId,
    pub target_id: String,
    pub content_hash: ContentHash,
}

impl DispositionTarget {
    pub fn block(document_id: DocumentId, block: &SourceBlock) -> Self {
        Self {
            kind: DispositionTargetKind::Block,
            document_id,
            target_id: block.block_id.to_string(),
            content_hash: block.content_hash,
        }
    }

    pub fn metadata(document_id: DocumentId, metadata: &MetadataValue) -> Self {
        Self {
            kind: DispositionTargetKind::Metadata,
            document_id,
            target_id: metadata.metadata_id.clone(),
            content_hash: metadata.content_hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispositionKind {
    Integrated,
    PreservedVerbatim,
    OmissionProposed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDisposition {
    pub target: DispositionTarget,
    pub disposition: DispositionKind,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionEvidence {
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisSection {
    pub section_id: String,
    pub heading: String,
    pub markdown_body: String,
    pub evidence: Vec<SectionEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelatedLinkKind {
    Prerequisite,
    Related,
    Contrasts,
    Supersedes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelatedLink {
    pub kind: RelatedLinkKind,
    pub target_cluster_id: String,
    pub evidence: Vec<SectionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContradictionClaim {
    pub claim_id: String,
    pub rendered_claim: String,
    pub observed_at: Option<String>,
    pub context: String,
    pub evidence: Vec<SectionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContradictionSet {
    pub contradiction_id: String,
    pub summary: String,
    pub claims: Vec<ContradictionClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisProposal {
    pub schema_version: u32,
    pub cluster_id: String,
    pub taxonomy_hash: ContentHash,
    pub proposal_hash: ContentHash,
    pub revision: u32,
    pub sections: Vec<SynthesisSection>,
    pub related_links: Vec<RelatedLink>,
    pub dispositions: Vec<SourceDisposition>,
    pub contradictions: Vec<ContradictionSet>,
    pub synthesis_recording_hash: ContentHash,
}

impl SynthesisProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn seal(
        cluster_id: impl Into<String>,
        taxonomy_hash: ContentHash,
        revision: u32,
        sections: Vec<SynthesisSection>,
        related_links: Vec<RelatedLink>,
        dispositions: Vec<SourceDisposition>,
        contradictions: Vec<ContradictionSet>,
        synthesis_recording_hash: ContentHash,
    ) -> Result<Self> {
        let cluster_id = cluster_id.into();
        let identity = SynthesisIdentity {
            cluster_id: &cluster_id,
            taxonomy_hash,
            revision,
            sections: &sections,
            related_links: &related_links,
            dispositions: &dispositions,
            contradictions: &contradictions,
            synthesis_recording_hash,
        };
        let proposal_hash = canonical_hash("okc:synthesis-proposal:v3\0", &identity)?;
        Ok(Self {
            schema_version: INTEGRATION_SCHEMA_VERSION,
            cluster_id,
            taxonomy_hash,
            proposal_hash,
            revision,
            sections,
            related_links,
            dispositions,
            contradictions,
            synthesis_recording_hash,
        })
    }
}

#[derive(Serialize)]
struct SynthesisIdentity<'a> {
    cluster_id: &'a str,
    taxonomy_hash: ContentHash,
    revision: u32,
    sections: &'a [SynthesisSection],
    related_links: &'a [RelatedLink],
    dispositions: &'a [SourceDisposition],
    contradictions: &'a [ContradictionSet],
    synthesis_recording_hash: ContentHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriticSeverity {
    Minor,
    Major,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriticFindingKind {
    UnsupportedClaim,
    Omission,
    HiddenContradiction,
    Misattribution,
    LinkLoss,
    MetadataLoss,
    PromptInjectionInfluence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriticFinding {
    pub finding_id: String,
    pub severity: CriticSeverity,
    pub kind: CriticFindingKind,
    pub message: String,
    pub evidence: Vec<SectionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriticReport {
    pub schema_version: u32,
    pub cluster_id: String,
    pub proposal_hash: ContentHash,
    pub critic_hash: ContentHash,
    pub findings: Vec<CriticFinding>,
    pub critic_recording_hash: ContentHash,
}

impl CriticReport {
    pub fn seal(
        cluster_id: impl Into<String>,
        proposal_hash: ContentHash,
        findings: Vec<CriticFinding>,
        critic_recording_hash: ContentHash,
    ) -> Result<Self> {
        let cluster_id = cluster_id.into();
        let identity = CriticIdentity {
            cluster_id: &cluster_id,
            proposal_hash,
            findings: &findings,
            critic_recording_hash,
        };
        let critic_hash = canonical_hash("okc:critic-report:v3\0", &identity)?;
        Ok(Self {
            schema_version: INTEGRATION_SCHEMA_VERSION,
            cluster_id,
            proposal_hash,
            critic_hash,
            findings,
            critic_recording_hash,
        })
    }
}

#[derive(Serialize)]
struct CriticIdentity<'a> {
    cluster_id: &'a str,
    proposal_hash: ContentHash,
    findings: &'a [CriticFinding],
    critic_recording_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingWaiver {
    pub finding_id: String,
    pub curator_id: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OmissionApproval {
    pub target: DispositionTarget,
    pub curator_id: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterApproval {
    pub cluster_id: String,
    pub taxonomy_hash: ContentHash,
    pub proposal_hash: ContentHash,
    pub critic_hash: ContentHash,
    pub revision_hash: ContentHash,
    pub approved: bool,
    pub curator_id: String,
    pub policy_version: String,
    pub omission_approvals: Vec<OmissionApproval>,
    pub minor_waivers: Vec<FindingWaiver>,
}

impl ClusterApproval {
    pub fn approve(
        proposal: &SynthesisProposal,
        critic: &CriticReport,
        curator_id: impl Into<String>,
        policy_version: impl Into<String>,
        omission_approvals: Vec<OmissionApproval>,
        minor_waivers: Vec<FindingWaiver>,
    ) -> Result<Self> {
        Ok(Self {
            cluster_id: proposal.cluster_id.clone(),
            taxonomy_hash: proposal.taxonomy_hash,
            proposal_hash: proposal.proposal_hash,
            critic_hash: critic.critic_hash,
            revision_hash: cluster_revision_hash(
                proposal,
                critic,
                &omission_approvals,
                &minor_waivers,
            )?,
            approved: true,
            curator_id: curator_id.into(),
            policy_version: policy_version.into(),
            omission_approvals,
            minor_waivers,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedClusterRevision {
    pub proposal: SynthesisProposal,
    pub critic: CriticReport,
    pub approval: ClusterApproval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedIntegrationPlan {
    pub format_family: String,
    pub schema_version: u32,
    pub integration_plan_id: String,
    pub corpus: IntegrationCorpus,
    pub taxonomy: TaxonomyProposal,
    pub taxonomy_approval: ApprovalBinding,
    pub clusters: Vec<ApprovedClusterRevision>,
    pub provider_recording_hashes: Vec<ContentHash>,
}

impl ApprovedIntegrationPlan {
    pub fn seal(
        corpus: IntegrationCorpus,
        taxonomy: TaxonomyProposal,
        taxonomy_approval: ApprovalBinding,
        mut clusters: Vec<ApprovedClusterRevision>,
        mut provider_recording_hashes: Vec<ContentHash>,
    ) -> Result<Self> {
        clusters.sort_by(|left, right| {
            left.proposal
                .cluster_id
                .as_bytes()
                .cmp(right.proposal.cluster_id.as_bytes())
        });
        provider_recording_hashes.sort_by_key(ContentHash::hex);
        provider_recording_hashes.dedup();
        let identity = IntegrationPlanIdentity {
            corpus_hash: corpus.corpus_hash,
            policy_hash: corpus.policy_hash,
            taxonomy_hash: taxonomy.taxonomy_hash,
            taxonomy_approval: &taxonomy_approval,
            clusters: &clusters,
            provider_recording_hashes: &provider_recording_hashes,
        };
        let integration_plan_id = format!(
            "integration_{}",
            canonical_hash("okc:integration-plan:v3\0", &identity)?.hex()
        );
        let plan = Self {
            format_family: "okc".into(),
            schema_version: INTEGRATION_SCHEMA_VERSION,
            integration_plan_id,
            corpus,
            taxonomy,
            taxonomy_approval,
            clusters,
            provider_recording_hashes,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_family != "okc" || self.schema_version != INTEGRATION_SCHEMA_VERSION {
            return Err(OkcError::PlanStale(
                "approved integration plan is not OKC schema 3".into(),
            ));
        }
        validate_corpus(&self.corpus)?;
        validate_taxonomy(&self.corpus, &self.taxonomy)?;
        validate_approval(
            "taxonomy",
            &self.taxonomy_approval,
            self.taxonomy.taxonomy_hash,
        )?;
        if self.provider_recording_hashes.is_empty() {
            return Err(OkcError::ApprovalStale(
                "Schema 3 integration requires sealed provider recordings".into(),
            ));
        }
        let recordings = self
            .provider_recording_hashes
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if recordings.len() != self.provider_recording_hashes.len() {
            return Err(OkcError::PlanStale(
                "provider recording hashes must be unique".into(),
            ));
        }
        if !recordings.contains(&self.taxonomy.organizer_recording_hash) {
            return Err(OkcError::ApprovalStale(
                "organizer recording is absent from the sealed recording set".into(),
            ));
        }
        let clusters_by_id = self
            .taxonomy
            .clusters
            .iter()
            .map(|cluster| (cluster.cluster_id.as_str(), cluster))
            .collect::<BTreeMap<_, _>>();
        if self.clusters.len() != clusters_by_id.len() {
            return Err(OkcError::ApprovalStale(
                "every taxonomy cluster, including singletons, requires one approved revision"
                    .into(),
            ));
        }
        let mut seen_clusters = BTreeSet::new();
        for revision in &self.clusters {
            let cluster_id = revision.proposal.cluster_id.as_str();
            let Some(cluster) = clusters_by_id.get(cluster_id) else {
                return Err(OkcError::ProposalInvalid(format!(
                    "proposal references unknown cluster `{cluster_id}`"
                )));
            };
            if !seen_clusters.insert(cluster_id) {
                return Err(OkcError::ProposalInvalid(format!(
                    "cluster `{cluster_id}` has duplicate revisions"
                )));
            }
            validate_cluster_revision(
                &self.corpus,
                &self.taxonomy,
                cluster,
                revision,
                &recordings,
            )?;
        }
        let identity = IntegrationPlanIdentity {
            corpus_hash: self.corpus.corpus_hash,
            policy_hash: self.corpus.policy_hash,
            taxonomy_hash: self.taxonomy.taxonomy_hash,
            taxonomy_approval: &self.taxonomy_approval,
            clusters: &self.clusters,
            provider_recording_hashes: &self.provider_recording_hashes,
        };
        let expected = format!(
            "integration_{}",
            canonical_hash("okc:integration-plan:v3\0", &identity)?.hex()
        );
        if self.integration_plan_id != expected {
            return Err(OkcError::PlanStale(
                "approved integration plan identity does not match its payload".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct IntegrationPlanIdentity<'a> {
    corpus_hash: ContentHash,
    policy_hash: ContentHash,
    taxonomy_hash: ContentHash,
    taxonomy_approval: &'a ApprovalBinding,
    clusters: &'a [ApprovedClusterRevision],
    provider_recording_hashes: &'a [ContentHash],
}

fn validate_corpus(corpus: &IntegrationCorpus) -> Result<()> {
    if corpus.schema_version != INTEGRATION_SCHEMA_VERSION {
        return Err(OkcError::PlanStale(
            "integration corpus is not schema 3".into(),
        ));
    }
    if corpus.documents.is_empty() {
        return Err(OkcError::ProposalInvalid(
            "Schema 3 integration corpus contains no Markdown documents".into(),
        ));
    }
    let expected = canonical_hash("okc:integration-corpus:v3\0", &corpus.documents)?;
    if expected != corpus.corpus_hash {
        return Err(OkcError::PlanStale(
            "integration corpus hash does not match its documents".into(),
        ));
    }
    let mut document_ids = BTreeSet::new();
    let mut source_paths = BTreeSet::new();
    let mut previous = None;
    for document in &corpus.documents {
        if previous.is_some_and(|previous| previous >= document.document_id) {
            return Err(OkcError::PlanStale(
                "integration documents are not strictly ID-sorted".into(),
            ));
        }
        previous = Some(document.document_id);
        if !document_ids.insert(document.document_id) {
            return Err(OkcError::ProposalInvalid(
                "integration corpus contains a duplicate document".into(),
            ));
        }
        validate_relative_path(&document.original_path, true)?;
        if !source_paths.insert((document.source_id.clone(), document.original_path.clone())) {
            return Err(OkcError::ProposalInvalid(
                "integration corpus contains a duplicate source path".into(),
            ));
        }
        let mut block_ids = BTreeSet::new();
        let mut previous_block = None;
        for block in &document.blocks {
            if previous_block.is_some_and(|previous| previous >= block.block_id)
                || !block_ids.insert(block.block_id)
            {
                return Err(OkcError::ProposalInvalid(
                    "document blocks must be unique and strictly ID-sorted".into(),
                ));
            }
            previous_block = Some(block.block_id);
            if block.content_hash
                != ContentHash::from_domain_bytes("okc:block-content:v3\0", block.text.as_bytes())
            {
                return Err(OkcError::PlanStale(
                    "integration block content hash does not match its text".into(),
                ));
            }
        }
        let mut metadata_ids = BTreeSet::new();
        let mut previous_metadata: Option<&str> = None;
        for metadata in &document.metadata {
            validate_bounded_data("metadata ID", &metadata.metadata_id, 1, 128)?;
            validate_bounded_data("metadata key", &metadata.key, 1, 1_024)?;
            if previous_metadata.is_some_and(|previous| previous >= metadata.metadata_id.as_str())
                || !metadata_ids.insert(metadata.metadata_id.as_str())
            {
                return Err(OkcError::ProposalInvalid(
                    "metadata values must be unique and strictly ID-sorted".into(),
                ));
            }
            previous_metadata = Some(&metadata.metadata_id);
            if MetadataValue::new(
                document.document_id,
                metadata.key.clone(),
                metadata.value_index,
                &metadata.value,
            )? != *metadata
            {
                return Err(OkcError::PlanStale(
                    "integration metadata identity does not match its value".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_taxonomy(corpus: &IntegrationCorpus, taxonomy: &TaxonomyProposal) -> Result<()> {
    if taxonomy.schema_version != INTEGRATION_SCHEMA_VERSION
        || taxonomy.corpus_hash != corpus.corpus_hash
    {
        return Err(OkcError::PlanStale(
            "taxonomy is stale for the integration corpus".into(),
        ));
    }
    let identity = TaxonomyIdentity {
        corpus_hash: taxonomy.corpus_hash,
        clusters: &taxonomy.clusters,
        organizer_recording_hash: taxonomy.organizer_recording_hash,
    };
    if taxonomy.taxonomy_hash != canonical_hash("okc:taxonomy:v3\0", &identity)? {
        return Err(OkcError::PlanStale(
            "taxonomy hash does not match its payload".into(),
        ));
    }
    let known_documents = corpus
        .documents
        .iter()
        .map(|document| document.document_id)
        .collect::<BTreeSet<_>>();
    let mut assigned = BTreeSet::new();
    let mut cluster_ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for cluster in &taxonomy.clusters {
        validate_bounded_data("cluster ID", &cluster.cluster_id, 1, 128)?;
        validate_bounded_data("cluster title", &cluster.title, 1, 1_024)?;
        if previous.is_some_and(|previous| previous >= cluster.cluster_id.as_str())
            || !cluster_ids.insert(cluster.cluster_id.as_str())
        {
            return Err(OkcError::ProposalInvalid(
                "taxonomy cluster IDs must be unique and strictly sorted".into(),
            ));
        }
        previous = Some(&cluster.cluster_id);
        validate_relative_path(&cluster.canonical_path, true)?;
        if !cluster.canonical_path.to_ascii_lowercase().ends_with(".md")
            || !paths.insert(cluster.canonical_path.to_ascii_lowercase())
        {
            return Err(OkcError::ProposalInvalid(
                "taxonomy canonical paths must be unique Markdown paths".into(),
            ));
        }
        if cluster.document_ids.is_empty() {
            return Err(OkcError::ProposalInvalid(
                "taxonomy clusters cannot be empty".into(),
            ));
        }
        let mut prior = None;
        for document_id in &cluster.document_ids {
            if prior.is_some_and(|prior| prior >= *document_id)
                || !known_documents.contains(document_id)
                || !assigned.insert(*document_id)
            {
                return Err(OkcError::ProposalInvalid(
                    "taxonomy must assign each known document exactly once".into(),
                ));
            }
            prior = Some(*document_id);
        }
    }
    if assigned != known_documents {
        return Err(OkcError::ProposalInvalid(
            "taxonomy does not cover every Markdown document".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_cluster_revision(
    corpus: &IntegrationCorpus,
    taxonomy: &TaxonomyProposal,
    cluster: &TaxonomyCluster,
    revision: &ApprovedClusterRevision,
    recordings: &BTreeSet<ContentHash>,
) -> Result<()> {
    let proposal = &revision.proposal;
    let critic = &revision.critic;
    let approval = &revision.approval;
    if proposal.schema_version != INTEGRATION_SCHEMA_VERSION
        || proposal.cluster_id != cluster.cluster_id
        || proposal.taxonomy_hash != taxonomy.taxonomy_hash
    {
        return Err(OkcError::ProposalInvalid(format!(
            "cluster `{}` proposal is stale",
            cluster.cluster_id
        )));
    }
    let proposal_identity = SynthesisIdentity {
        cluster_id: &proposal.cluster_id,
        taxonomy_hash: proposal.taxonomy_hash,
        revision: proposal.revision,
        sections: &proposal.sections,
        related_links: &proposal.related_links,
        dispositions: &proposal.dispositions,
        contradictions: &proposal.contradictions,
        synthesis_recording_hash: proposal.synthesis_recording_hash,
    };
    if proposal.proposal_hash != canonical_hash("okc:synthesis-proposal:v3\0", &proposal_identity)?
    {
        return Err(OkcError::ProposalInvalid(format!(
            "cluster `{}` proposal hash is forged",
            cluster.cluster_id
        )));
    }
    if !recordings.contains(&proposal.synthesis_recording_hash)
        || !recordings.contains(&critic.critic_recording_hash)
    {
        return Err(OkcError::ApprovalStale(format!(
            "cluster `{}` provider recording is absent",
            cluster.cluster_id
        )));
    }
    let documents = corpus
        .documents
        .iter()
        .filter(|document| cluster.document_ids.contains(&document.document_id))
        .map(|document| (document.document_id, document))
        .collect::<BTreeMap<_, _>>();
    let expected_targets =
        documents
            .values()
            .flat_map(|document| {
                document
                    .blocks
                    .iter()
                    .map(|block| DispositionTarget::block(document.document_id, block))
                    .chain(document.metadata.iter().map(|metadata| {
                        DispositionTarget::metadata(document.document_id, metadata)
                    }))
            })
            .collect::<BTreeSet<_>>();
    let actual_targets = proposal
        .dispositions
        .iter()
        .map(|disposition| disposition.target.clone())
        .collect::<BTreeSet<_>>();
    if actual_targets.len() != proposal.dispositions.len() || actual_targets != expected_targets {
        return Err(OkcError::ProposalInvalid(format!(
            "cluster `{}` must give every block and metadata value exactly one disposition",
            cluster.cluster_id
        )));
    }
    let omitted = proposal
        .dispositions
        .iter()
        .filter(|item| item.disposition == DispositionKind::OmissionProposed)
        .map(|item| item.target.clone())
        .collect::<BTreeSet<_>>();
    for disposition in &proposal.dispositions {
        if disposition.disposition == DispositionKind::OmissionProposed {
            validate_optional_rationale(
                "omission proposal",
                disposition.rationale.as_deref(),
                true,
            )?;
        }
    }
    let approved_omissions = approval
        .omission_approvals
        .iter()
        .map(|item| item.target.clone())
        .collect::<BTreeSet<_>>();
    if approved_omissions.len() != approval.omission_approvals.len()
        || approved_omissions != omitted
    {
        return Err(OkcError::ApprovalStale(format!(
            "cluster `{}` omission approvals are incomplete or excessive",
            cluster.cluster_id
        )));
    }
    for omission in &approval.omission_approvals {
        validate_bounded_data("omission curator", &omission.curator_id, 1, 256)?;
        validate_bounded_data("omission rationale", &omission.rationale, 1, 4_096)?;
    }
    let mut section_ids = BTreeSet::new();
    for section in &proposal.sections {
        validate_bounded_data("section ID", &section.section_id, 1, 128)?;
        validate_bounded_data("section heading", &section.heading, 1, 1_024)?;
        validate_bounded_data("section body", &section.markdown_body, 1, 16 * 1024 * 1024)?;
        if !section_ids.insert(section.section_id.as_str()) || section.evidence.is_empty() {
            return Err(OkcError::ProposalInvalid(format!(
                "cluster `{}` sections require unique IDs and evidence",
                cluster.cluster_id
            )));
        }
        validate_evidence(&documents, &section.evidence)?;
    }
    let integrated_blocks = proposal
        .dispositions
        .iter()
        .filter(|item| {
            item.target.kind == DispositionTargetKind::Block
                && item.disposition == DispositionKind::Integrated
        })
        .map(|item| (item.target.document_id, item.target.target_id.clone()))
        .collect::<BTreeSet<_>>();
    let evidenced_blocks = proposal
        .sections
        .iter()
        .flat_map(|section| &section.evidence)
        .chain(
            proposal
                .contradictions
                .iter()
                .flat_map(|set| &set.claims)
                .flat_map(|claim| &claim.evidence),
        )
        .map(|item| (item.document_id, item.block_id.to_string()))
        .collect::<BTreeSet<_>>();
    if integrated_blocks
        .iter()
        .any(|target| !evidenced_blocks.contains(target))
    {
        return Err(OkcError::ProposalInvalid(format!(
            "cluster `{}` marks a block integrated without citing it",
            cluster.cluster_id
        )));
    }
    let known_clusters = taxonomy
        .clusters
        .iter()
        .map(|candidate| candidate.cluster_id.as_str())
        .collect::<BTreeSet<_>>();
    for link in &proposal.related_links {
        if !known_clusters.contains(link.target_cluster_id.as_str()) {
            return Err(OkcError::ProposalInvalid(format!(
                "cluster `{}` has a related link to an unknown cluster",
                cluster.cluster_id
            )));
        }
        validate_evidence(&documents, &link.evidence)?;
    }
    let mut contradiction_ids = BTreeSet::new();
    for contradiction in &proposal.contradictions {
        validate_bounded_data("contradiction ID", &contradiction.contradiction_id, 1, 128)?;
        if !contradiction_ids.insert(contradiction.contradiction_id.as_str())
            || contradiction.claims.len() < 2
        {
            return Err(OkcError::ProposalInvalid(
                "contradiction sets require a unique ID and at least two claims".into(),
            ));
        }
        let mut claim_ids = BTreeSet::new();
        for claim in &contradiction.claims {
            validate_bounded_data("contradiction claim", &claim.rendered_claim, 1, 65_536)?;
            validate_bounded_data("contradiction context", &claim.context, 1, 4_096)?;
            if !claim_ids.insert(claim.claim_id.as_str()) || claim.evidence.is_empty() {
                return Err(OkcError::ProposalInvalid(
                    "contradiction claims require unique IDs and evidence".into(),
                ));
            }
            validate_evidence(&documents, &claim.evidence)?;
        }
    }
    validate_critic(proposal, critic, &documents, approval)?;
    validate_cluster_approval(proposal, critic, approval)?;
    Ok(())
}

fn validate_evidence(
    documents: &BTreeMap<DocumentId, &IntegrationDocument>,
    evidence: &[SectionEvidence],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for item in evidence {
        if !seen.insert((item.document_id, item.block_id, item.content_hash)) {
            return Err(OkcError::ProposalInvalid(
                "evidence references must be unique".into(),
            ));
        }
        let valid = documents.get(&item.document_id).is_some_and(|document| {
            document.blocks.iter().any(|block| {
                block.block_id == item.block_id && block.content_hash == item.content_hash
            })
        });
        if !valid {
            return Err(OkcError::ProposalInvalid(
                "evidence is forged, stale, or belongs to another cluster".into(),
            ));
        }
    }
    Ok(())
}

fn validate_critic(
    proposal: &SynthesisProposal,
    critic: &CriticReport,
    documents: &BTreeMap<DocumentId, &IntegrationDocument>,
    approval: &ClusterApproval,
) -> Result<()> {
    if critic.schema_version != INTEGRATION_SCHEMA_VERSION
        || critic.cluster_id != proposal.cluster_id
        || critic.proposal_hash != proposal.proposal_hash
    {
        return Err(OkcError::ApprovalStale(format!(
            "cluster `{}` critic is stale",
            proposal.cluster_id
        )));
    }
    let identity = CriticIdentity {
        cluster_id: &critic.cluster_id,
        proposal_hash: critic.proposal_hash,
        findings: &critic.findings,
        critic_recording_hash: critic.critic_recording_hash,
    };
    if critic.critic_hash != canonical_hash("okc:critic-report:v3\0", &identity)? {
        return Err(OkcError::ApprovalStale(format!(
            "cluster `{}` critic hash is forged",
            proposal.cluster_id
        )));
    }
    let mut finding_ids = BTreeSet::new();
    for finding in &critic.findings {
        validate_bounded_data("critic finding ID", &finding.finding_id, 1, 128)?;
        validate_bounded_data("critic finding message", &finding.message, 1, 8_192)?;
        if !finding_ids.insert(finding.finding_id.as_str()) {
            return Err(OkcError::ProposalInvalid(
                "critic finding IDs must be unique".into(),
            ));
        }
        validate_evidence(documents, &finding.evidence)?;
        if matches!(
            finding.severity,
            CriticSeverity::Major | CriticSeverity::Critical
        ) {
            return Err(OkcError::ApprovalStale(format!(
                "cluster `{}` has an unresolved {:?} critic finding",
                proposal.cluster_id, finding.severity
            )));
        }
    }
    let expected_waivers = critic
        .findings
        .iter()
        .filter(|finding| finding.severity == CriticSeverity::Minor)
        .map(|finding| finding.finding_id.as_str())
        .collect::<BTreeSet<_>>();
    let actual_waivers = approval
        .minor_waivers
        .iter()
        .map(|waiver| waiver.finding_id.as_str())
        .collect::<BTreeSet<_>>();
    if actual_waivers.len() != approval.minor_waivers.len() || actual_waivers != expected_waivers {
        return Err(OkcError::ApprovalStale(format!(
            "cluster `{}` minor critic waivers are incomplete or excessive",
            proposal.cluster_id
        )));
    }
    for waiver in &approval.minor_waivers {
        validate_bounded_data("finding waiver curator", &waiver.curator_id, 1, 256)?;
        validate_bounded_data("finding waiver rationale", &waiver.rationale, 1, 4_096)?;
    }
    Ok(())
}

fn validate_cluster_approval(
    proposal: &SynthesisProposal,
    critic: &CriticReport,
    approval: &ClusterApproval,
) -> Result<()> {
    if !approval.approved
        || approval.cluster_id != proposal.cluster_id
        || approval.taxonomy_hash != proposal.taxonomy_hash
        || approval.proposal_hash != proposal.proposal_hash
        || approval.critic_hash != critic.critic_hash
    {
        return Err(OkcError::ApprovalStale(format!(
            "cluster `{}` approval is absent or stale",
            proposal.cluster_id
        )));
    }
    validate_bounded_data("cluster approval curator", &approval.curator_id, 1, 256)?;
    validate_bounded_data(
        "cluster approval policy version",
        &approval.policy_version,
        1,
        256,
    )?;
    let expected = cluster_revision_hash(
        proposal,
        critic,
        &approval.omission_approvals,
        &approval.minor_waivers,
    )?;
    if approval.revision_hash != expected {
        return Err(OkcError::ApprovalStale(format!(
            "cluster `{}` revision hash is stale",
            proposal.cluster_id
        )));
    }
    Ok(())
}

fn cluster_revision_hash(
    proposal: &SynthesisProposal,
    critic: &CriticReport,
    omissions: &[OmissionApproval],
    waivers: &[FindingWaiver],
) -> Result<ContentHash> {
    #[derive(Serialize)]
    struct Revision<'a> {
        taxonomy_hash: ContentHash,
        proposal: &'a SynthesisProposal,
        critic: &'a CriticReport,
        omission_approvals: &'a [OmissionApproval],
        minor_waivers: &'a [FindingWaiver],
    }
    canonical_hash(
        "okc:cluster-revision:v3\0",
        &Revision {
            taxonomy_hash: proposal.taxonomy_hash,
            proposal,
            critic,
            omission_approvals: omissions,
            minor_waivers: waivers,
        },
    )
}

fn validate_approval(kind: &str, approval: &ApprovalBinding, expected: ContentHash) -> Result<()> {
    if !approval.approved || approval.target_hash != expected {
        return Err(OkcError::ApprovalStale(format!(
            "{kind} approval is absent or stale"
        )));
    }
    validate_bounded_data("approval curator", &approval.curator_id, 1, 256)?;
    validate_bounded_data("approval policy version", &approval.policy_version, 1, 256)?;
    validate_optional_rationale(kind, approval.rationale.as_deref(), false)
}

fn validate_optional_rationale(kind: &str, rationale: Option<&str>, required: bool) -> Result<()> {
    if required && rationale.is_none_or(str::is_empty) {
        return Err(OkcError::ProposalInvalid(format!(
            "{kind} requires a rationale"
        )));
    }
    if let Some(rationale) = rationale {
        validate_bounded_data("rationale", rationale, usize::from(required), 4_096)?;
    }
    Ok(())
}

fn validate_bounded_data(label: &str, value: &str, minimum: usize, maximum: usize) -> Result<()> {
    if value.len() < minimum || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(OkcError::ProposalInvalid(format!(
            "{label} must contain {minimum}..={maximum} non-control UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_relative_path(path: &str, require_file: bool) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path.chars().any(char::is_control)
        || path.len() > 1_024
    {
        return Err(OkcError::UnsafePath {
            path: path.into(),
            reason: "Schema 3 logical paths must be bounded portable relative UTF-8 paths".into(),
        });
    }
    let parsed = Path::new(path);
    if parsed.components().any(|component| {
        !matches!(component, Component::Normal(_))
            || component.as_os_str().len() > 240
            || matches!(component.as_os_str().to_str(), Some("." | ".."))
    }) || (require_file && parsed.file_name().is_none())
    {
        return Err(OkcError::UnsafePath {
            path: path.into(),
            reason: "Schema 3 logical path contains a non-portable component".into(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub path: String,
    pub byte_len: u64,
    pub raw_sha256: String,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledVaultManifest {
    pub format_family: String,
    pub schema_version: u32,
    pub product_version: String,
    pub integration_plan_id: String,
    pub corpus_hash: ContentHash,
    pub taxonomy_hash: ContentHash,
    pub pack_profile: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledArtifact {
    pub path: PathBuf,
    pub integration_plan_id: String,
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceRecord {
    pub schema_version: u32,
    pub record_id: String,
    pub kind: String,
    pub output_path: String,
    pub output_hash: ContentHash,
    pub integration_plan_id: String,
    pub cluster_id: Option<String>,
    pub proposal_hash: Option<ContentHash>,
    pub critic_hash: Option<ContentHash>,
    pub approval_hash: Option<ContentHash>,
    pub evidence: Vec<SectionEvidence>,
    pub source_document: Option<DocumentId>,
}

/// Materialize a fully approved Schema 3 plan. This function has no provider input
/// and performs no network access.
pub fn compile(
    plan: &ApprovedIntegrationPlan,
    destination: impl AsRef<Path>,
) -> Result<CompiledArtifact> {
    plan.validate()?;
    let destination = destination.as_ref();
    if fs::symlink_metadata(destination).is_ok() {
        return Err(OkcError::OutputExists(destination.to_path_buf()));
    }
    let parent = destination
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| OkcError::io(parent, error))?;
    let stage = tempfile::Builder::new()
        .prefix(".okc-stage-")
        .tempdir_in(parent)
        .map_err(|error| OkcError::io(parent, error))?;
    let files = materialized_files(plan)?;
    for (path, bytes) in &files {
        write_new_file(stage.path(), path, bytes)?;
    }
    let plan_bytes = to_canonical_json_pretty(plan)?;
    write_new_file(stage.path(), ".okc/integration-plan.json", &plan_bytes)?;
    let provenance = provenance_jsonl(plan, &files)?;
    write_new_file(stage.path(), ".okc/provenance.jsonl", &provenance)?;
    let mut inventory_files =
        inventory(stage.path(), &[".okc/manifest.json", ".okc/checksums.txt"])?;
    let manifest = CompiledVaultManifest {
        format_family: "okc".into(),
        schema_version: INTEGRATION_SCHEMA_VERSION,
        product_version: PRODUCT_VERSION.into(),
        integration_plan_id: plan.integration_plan_id.clone(),
        corpus_hash: plan.corpus.corpus_hash,
        taxonomy_hash: plan.taxonomy.taxonomy_hash,
        pack_profile: PACK_PROFILE.into(),
        files: inventory_files.clone(),
    };
    write_new_file(
        stage.path(),
        ".okc/manifest.json",
        &to_canonical_json_pretty(&manifest)?,
    )?;
    inventory_files = inventory(stage.path(), &[".okc/checksums.txt"])?;
    let checksums = inventory_files
        .iter()
        .fold(String::new(), |mut output, file| {
            writeln!(output, "{}  {}", file.raw_sha256, file.path)
                .expect("writing to a String cannot fail");
            output
        });
    write_new_file(stage.path(), ".okc/checksums.txt", checksums.as_bytes())?;
    verify(stage.path())?;
    sync_directory(stage.path()).map_err(|error| OkcError::io(stage.path(), error))?;
    let staging = stage.keep();
    match publish_directory_noreplace(&staging, destination) {
        Ok(()) => {}
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            if error.kind() == std::io::ErrorKind::AlreadyExists
                || fs::symlink_metadata(destination).is_ok()
            {
                return Err(OkcError::OutputExists(destination.to_path_buf()));
            }
            return Err(OkcError::io(destination, error));
        }
    }
    sync_directory(parent).map_err(|source| OkcError::PublishedButDurabilityUncertain {
        path: destination.to_path_buf(),
        source,
    })?;
    Ok(CompiledArtifact {
        path: destination.to_path_buf(),
        integration_plan_id: plan.integration_plan_id.clone(),
        files: inventory_files.len() + 1,
    })
}

pub fn verify(root: impl AsRef<Path>) -> Result<CompiledVaultManifest> {
    let root = root.as_ref();
    let manifest: CompiledVaultManifest = serde_json::from_slice(
        &fs::read(root.join(".okc/manifest.json"))
            .map_err(|error| OkcError::io(root.join(".okc/manifest.json"), error))?,
    )?;
    if manifest.format_family != "okc" || manifest.schema_version != INTEGRATION_SCHEMA_VERSION {
        return Err(OkcError::VerificationFailed(
            "artifact is not an OKC Schema 3 Compiled Vault".into(),
        ));
    }
    let plan: ApprovedIntegrationPlan = serde_json::from_slice(
        &fs::read(root.join(".okc/integration-plan.json"))
            .map_err(|error| OkcError::io(root.join(".okc/integration-plan.json"), error))?,
    )?;
    plan.validate()?;
    if manifest.integration_plan_id != plan.integration_plan_id
        || manifest.corpus_hash != plan.corpus.corpus_hash
        || manifest.taxonomy_hash != plan.taxonomy.taxonomy_hash
        || manifest.pack_profile != PACK_PROFILE
    {
        return Err(OkcError::VerificationFailed(
            "Schema 3 manifest does not match its approved integration plan".into(),
        ));
    }
    let expected_files = inventory(root, &[".okc/manifest.json", ".okc/checksums.txt"])?;
    if manifest.files != expected_files {
        return Err(OkcError::VerificationFailed(
            "Schema 3 manifest inventory does not match artifact files".into(),
        ));
    }
    let checksum_inventory = inventory(root, &[".okc/checksums.txt"])?;
    let expected_checksums = checksum_inventory
        .iter()
        .fold(String::new(), |mut output, file| {
            writeln!(output, "{}  {}", file.raw_sha256, file.path)
                .expect("writing to a String cannot fail");
            output
        });
    let observed_checksums = fs::read(root.join(".okc/checksums.txt"))
        .map_err(|error| OkcError::io(root.join(".okc/checksums.txt"), error))?;
    if observed_checksums != expected_checksums.as_bytes() {
        return Err(OkcError::VerificationFailed(
            "Schema 3 checksums are incomplete or stale".into(),
        ));
    }
    let expected_materialized = materialized_files(&plan)?;
    for (path, bytes) in expected_materialized {
        let observed =
            fs::read(root.join(&path)).map_err(|error| OkcError::io(root.join(&path), error))?;
        if observed != bytes {
            return Err(OkcError::VerificationFailed(format!(
                "Schema 3 materialized file `{path}` is not reproducible from the plan"
            )));
        }
    }
    let expected_provenance = provenance_jsonl(&plan, &materialized_files(&plan)?)?;
    if fs::read(root.join(".okc/provenance.jsonl"))
        .map_err(|error| OkcError::io(root.join(".okc/provenance.jsonl"), error))?
        != expected_provenance
    {
        return Err(OkcError::VerificationFailed(
            "Schema 3 provenance is not closed over the approved integration".into(),
        ));
    }
    Ok(manifest)
}

/// Verify a Schema 3 directory and return the provenance record for one materialized
/// path. Explanation is deliberately provider-free and rejects absent or
/// duplicate records.
pub fn explain(root: impl AsRef<Path>, output_path: &str) -> Result<ProvenanceRecord> {
    let root = root.as_ref();
    verify(root)?;
    validate_relative_path(output_path, true)?;
    let bytes = fs::read(root.join(".okc/provenance.jsonl"))
        .map_err(|error| OkcError::io(root.join(".okc/provenance.jsonl"), error))?;
    let mut matching = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice::<ProvenanceRecord>)
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|record| record.output_path == output_path);
    let record = matching.next().ok_or_else(|| {
        OkcError::VerificationFailed(format!(
            "Schema 3 artifact has no provenance for `{output_path}`"
        ))
    })?;
    if matching.next().is_some() {
        return Err(OkcError::VerificationFailed(format!(
            "Schema 3 artifact has duplicate provenance for `{output_path}`"
        )));
    }
    Ok(record)
}

fn materialized_files(plan: &ApprovedIntegrationPlan) -> Result<Vec<(String, Vec<u8>)>> {
    let taxonomy = plan
        .taxonomy
        .clusters
        .iter()
        .map(|cluster| (cluster.cluster_id.as_str(), cluster))
        .collect::<BTreeMap<_, _>>();
    let mut files = Vec::new();
    let mut document_targets = BTreeMap::new();
    for revision in &plan.clusters {
        let cluster = taxonomy[revision.proposal.cluster_id.as_str()];
        let canonical_path = format!("knowledge/{}", cluster.canonical_path);
        validate_relative_path(&canonical_path, true)?;
        for document_id in &cluster.document_ids {
            document_targets.insert(*document_id, canonical_path.clone());
        }
        files.push((
            canonical_path,
            render_canonical_note(plan, cluster, revision)?,
        ));
    }
    for document in &plan.corpus.documents {
        let canonical = document_targets.get(&document.document_id).ok_or_else(|| {
            OkcError::Internal("taxonomy document target disappeared during materialization".into())
        })?;
        let stub_path = format!("legacy/{}/{}", document.source_id, document.original_path);
        validate_relative_path(&stub_path, true)?;
        files.push((
            stub_path,
            render_legacy_stub(plan, document, canonical).into_bytes(),
        ));
    }
    files.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    if files.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(OkcError::UnsafePath {
            path: "materialized output".into(),
            reason: "Schema 3 output paths collide".into(),
        });
    }
    Ok(files)
}

#[allow(clippy::too_many_lines)]
fn render_canonical_note(
    plan: &ApprovedIntegrationPlan,
    cluster: &TaxonomyCluster,
    revision: &ApprovedClusterRevision,
) -> Result<Vec<u8>> {
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str("okc_generated: true\n");
    output.push_str("okc_schema: 3\n");
    writeln!(
        output,
        "okc_cluster_id: {}",
        yaml_scalar(&cluster.cluster_id)
    )
    .expect("writing to a String cannot fail");
    writeln!(
        output,
        "okc_integration_plan: {}",
        yaml_scalar(&plan.integration_plan_id)
    )
    .expect("writing to a String cannot fail");
    let documents = cluster
        .document_ids
        .iter()
        .filter_map(|document_id| {
            plan.corpus
                .documents
                .iter()
                .find(|document| document.document_id == *document_id)
        })
        .collect::<Vec<_>>();
    let disposition_by_target = revision
        .proposal
        .dispositions
        .iter()
        .map(|item| {
            (
                (item.target.document_id, item.target.target_id.as_str()),
                item.disposition,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let retained_metadata = documents
        .iter()
        .flat_map(|document| {
            document.metadata.iter().filter_map(|metadata| {
                let disposition = disposition_by_target
                    .get(&(document.document_id, metadata.metadata_id.as_str()))?;
                (*disposition != DispositionKind::OmissionProposed).then(|| {
                    json!({
                        "document_id": document.document_id,
                        "key": metadata.key,
                        "value_index": metadata.value_index,
                        "value": metadata.value,
                        "disposition": disposition
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    if !retained_metadata.is_empty() {
        output.push_str("okc_retained_metadata: ");
        output.push_str(&serde_json::to_string(&retained_metadata).map_err(OkcError::Json)?);
        output.push('\n');
    }
    output.push_str("---\n\n# ");
    output.push_str(&cluster.title);
    output.push('\n');
    for section in &revision.proposal.sections {
        output.push_str("\n## ");
        output.push_str(&section.heading);
        output.push_str(" {#");
        output.push_str(&section.section_id);
        output.push_str("}\n\n");
        output.push_str(section.markdown_body.trim_end());
        output.push('\n');
    }
    let preserved_blocks = documents
        .iter()
        .flat_map(|document| {
            document.blocks.iter().filter_map(|block| {
                (disposition_by_target
                    .get(&(document.document_id, block.block_id.to_string().as_str()))
                    == Some(&DispositionKind::PreservedVerbatim))
                .then_some((document.document_id, block))
            })
        })
        .collect::<Vec<_>>();
    if !preserved_blocks.is_empty() {
        output.push_str("\n## Preserved source material\n");
        for (document_id, block) in preserved_blocks {
            write!(
                output,
                "\n<!-- okc-source: {document_id} {} -->\n\n",
                block.block_id
            )
            .expect("writing to a String cannot fail");
            output.push_str(block.text.trim_end());
            output.push('\n');
        }
    }
    if !revision.proposal.contradictions.is_empty() {
        output.push_str("\n## Contradictions\n");
        for contradiction in &revision.proposal.contradictions {
            output.push_str("\n### ");
            output.push_str(&contradiction.summary);
            output.push('\n');
            for claim in &contradiction.claims {
                output.push_str("\n- **");
                output.push_str(&claim.context);
                output.push_str(":** ");
                output.push_str(&claim.rendered_claim);
                if let Some(observed_at) = &claim.observed_at {
                    output.push_str(" (`");
                    output.push_str(observed_at);
                    output.push_str("`)");
                }
                output.push('\n');
            }
        }
    }
    if !revision.proposal.related_links.is_empty() {
        let by_id = plan
            .taxonomy
            .clusters
            .iter()
            .map(|item| (item.cluster_id.as_str(), item))
            .collect::<BTreeMap<_, _>>();
        output.push_str("\n## Related\n");
        for link in &revision.proposal.related_links {
            let target = by_id.get(link.target_cluster_id.as_str()).ok_or_else(|| {
                OkcError::Internal("validated related cluster disappeared".into())
            })?;
            write!(
                output,
                "\n- {:?}: [[knowledge/{}|{}]]\n",
                link.kind, target.canonical_path, target.title
            )
            .expect("writing to a String cannot fail");
        }
    }
    Ok(output.into_bytes())
}

fn render_legacy_stub(
    plan: &ApprovedIntegrationPlan,
    document: &IntegrationDocument,
    canonical: &str,
) -> String {
    format!(
        "---\nokc_redirect: true\nokc_schema: 3\nokc_source_document: {}\nokc_integration_plan: {}\n---\n\nThis source note was integrated into [[{}]].\n",
        yaml_scalar(&document.document_id.to_string()),
        yaml_scalar(&plan.integration_plan_id),
        canonical.strip_suffix(".md").unwrap_or(canonical)
    )
}

fn yaml_scalar(value: &str) -> String {
    serde_json::to_string(value).expect("string JSON serialization cannot fail")
}

#[derive(Serialize)]
struct ProvenanceIdentity<'a> {
    output_path: &'a str,
    output_hash: ContentHash,
    integration_plan_id: &'a str,
    kind: &'a str,
}

#[allow(clippy::too_many_lines)]
fn provenance_jsonl(
    plan: &ApprovedIntegrationPlan,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<u8>> {
    let cluster_by_path = plan
        .taxonomy
        .clusters
        .iter()
        .map(|cluster| (format!("knowledge/{}", cluster.canonical_path), cluster))
        .collect::<BTreeMap<_, _>>();
    let revision_by_id = plan
        .clusters
        .iter()
        .map(|revision| (revision.proposal.cluster_id.as_str(), revision))
        .collect::<BTreeMap<_, _>>();
    let source_by_path = plan
        .corpus
        .documents
        .iter()
        .map(|document| {
            (
                format!("legacy/{}/{}", document.source_id, document.original_path),
                document,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut encoded = Vec::new();
    for (path, bytes) in files {
        let output_hash = ContentHash::from_domain_bytes("okc:output:v3\0", bytes);
        let (kind, cluster_id, proposal_hash, critic_hash, approval_hash, evidence, source) =
            if let Some(cluster) = cluster_by_path.get(path) {
                let revision = revision_by_id[cluster.cluster_id.as_str()];
                let evidence = revision
                    .proposal
                    .sections
                    .iter()
                    .flat_map(|section| section.evidence.iter().cloned())
                    .chain(
                        revision
                            .proposal
                            .contradictions
                            .iter()
                            .flat_map(|set| &set.claims)
                            .flat_map(|claim| claim.evidence.iter().cloned()),
                    )
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                (
                    "canonical_note",
                    Some(cluster.cluster_id.as_str()),
                    Some(revision.proposal.proposal_hash),
                    Some(revision.critic.critic_hash),
                    Some(revision.approval.revision_hash),
                    evidence,
                    None,
                )
            } else if let Some(document) = source_by_path.get(path) {
                let cluster = plan
                    .taxonomy
                    .clusters
                    .iter()
                    .find(|cluster| cluster.document_ids.contains(&document.document_id))
                    .ok_or_else(|| {
                        OkcError::Internal(
                            "validated legacy redirect lost its taxonomy cluster".into(),
                        )
                    })?;
                let revision = revision_by_id[cluster.cluster_id.as_str()];
                let evidence = document
                    .blocks
                    .iter()
                    .map(|block| SectionEvidence {
                        document_id: document.document_id,
                        block_id: block.block_id,
                        content_hash: block.content_hash,
                    })
                    .collect();
                (
                    "legacy_redirect",
                    Some(cluster.cluster_id.as_str()),
                    Some(revision.proposal.proposal_hash),
                    Some(revision.critic.critic_hash),
                    Some(revision.approval.revision_hash),
                    evidence,
                    Some(document.document_id),
                )
            } else {
                return Err(OkcError::Internal(
                    "unclassified Schema 3 materialized output".into(),
                ));
            };
        let record_id = format!(
            "record_{}",
            canonical_hash(
                "okc:provenance:v3\0",
                &ProvenanceIdentity {
                    output_path: path,
                    output_hash,
                    integration_plan_id: &plan.integration_plan_id,
                    kind,
                }
            )?
            .hex()
        );
        let record = ProvenanceRecord {
            schema_version: INTEGRATION_SCHEMA_VERSION,
            record_id,
            kind: kind.into(),
            output_path: path.clone(),
            output_hash,
            integration_plan_id: plan.integration_plan_id.clone(),
            cluster_id: cluster_id.map(str::to_owned),
            proposal_hash,
            critic_hash,
            approval_hash,
            evidence,
            source_document: source,
        };
        encoded.extend(to_canonical_json(&record)?);
        encoded.push(b'\n');
    }
    Ok(encoded)
}

fn inventory(root: &Path, excluded: &[&str]) -> Result<Vec<ManifestFile>> {
    fn walk(
        root: &Path,
        directory: &Path,
        excluded: &[&str],
        files: &mut Vec<ManifestFile>,
    ) -> Result<()> {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| OkcError::io(directory, error))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| OkcError::io(directory, error))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let metadata = entry
                .file_type()
                .map_err(|error| OkcError::io(entry.path(), error))?;
            if metadata.is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
                return Err(OkcError::VerificationFailed(
                    "Schema 3 artifacts may contain only regular files and directories".into(),
                ));
            }
            if metadata.is_dir() {
                walk(root, &entry.path(), excluded, files)?;
                continue;
            }
            let entry_path = entry.path();
            let relative = entry_path.strip_prefix(root).map_err(|_| {
                OkcError::VerificationFailed("artifact inventory escaped its root".into())
            })?;
            let path = relative
                .components()
                .map(|component| {
                    component.as_os_str().to_str().ok_or_else(|| {
                        OkcError::VerificationFailed("artifact path is not portable UTF-8".into())
                    })
                })
                .collect::<Result<Vec<_>>>()?
                .join("/");
            if excluded.contains(&path.as_str()) {
                continue;
            }
            validate_relative_path(&path, true)?;
            let bytes =
                fs::read(entry.path()).map_err(|error| OkcError::io(entry.path(), error))?;
            files.push(ManifestFile {
                path,
                byte_len: bytes.len() as u64,
                raw_sha256: format!("{:x}", Sha256::digest(&bytes)),
                content_hash: ContentHash::from_domain_bytes("okc:content:v3\0", &bytes),
            });
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, root, excluded, &mut files)?;
    files.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(files)
}

fn write_new_file(root: &Path, logical_path: &str, bytes: &[u8]) -> Result<()> {
    validate_relative_path(logical_path, true)?;
    let path = root.join(logical_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| OkcError::io(parent, error))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| OkcError::io(&path, error))?;
    file.write_all(bytes)
        .map_err(|error| OkcError::io(&path, error))?;
    file.sync_all().map_err(|error| OkcError::io(&path, error))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn publish_directory_noreplace(staging: &Path, destination: &Path) -> std::io::Result<()> {
    let staging_parent = staging.parent().unwrap_or_else(|| Path::new("."));
    let destination_parent = destination.parent().unwrap_or_else(|| Path::new("."));
    if staging_parent.canonicalize()? != destination_parent.canonicalize()? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "staging and destination are not siblings",
        ));
    }
    let staging_name = staging.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "staging has no leaf")
    })?;
    let destination_name = destination.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "destination has no leaf")
    })?;
    let parent = File::open(destination_parent)?;
    rustix::fs::renameat_with(
        &parent,
        staging_name,
        &parent,
        destination_name,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)
}

#[cfg(windows)]
fn publish_directory_noreplace(staging: &Path, destination: &Path) -> std::io::Result<()> {
    atomicwrites::move_atomic(staging, destination)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn publish_directory_noreplace(_staging: &Path, _destination: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace publication is unsupported on this platform",
    ))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        corpus: IntegrationCorpus,
        taxonomy: TaxonomyProposal,
        plan: ApprovedIntegrationPlan,
    }

    fn hash(label: &str) -> ContentHash {
        ContentHash::from_domain_bytes("okc:test:v3\0", label.as_bytes())
    }

    #[allow(clippy::too_many_lines)]
    fn fixture() -> Fixture {
        let document_id = DocumentId::from_hash(hash("document"));
        let block_a = SourceBlock {
            block_id: BlockId::from_hash(hash("block-a")),
            content_hash: ContentHash::from_domain_bytes("okc:block-content:v3\0", b"Block A"),
            text: "Block A".into(),
        };
        let block_b = SourceBlock {
            block_id: BlockId::from_hash(hash("block-b")),
            content_hash: ContentHash::from_domain_bytes("okc:block-content:v3\0", b"Block B"),
            text: "Block B".into(),
        };
        let metadata = MetadataValue::new(document_id, "tags", 0, &Value::String("rust".into()))
            .expect("metadata");
        let corpus = IntegrationCorpus::seal(
            hash("policy"),
            vec![IntegrationDocument {
                source_id: SourceId::new("alpha").expect("source"),
                document_id,
                original_path: "Notes/Topic.md".into(),
                document_hash: hash("document-body"),
                blocks: vec![block_a.clone(), block_b.clone()],
                metadata: vec![metadata.clone()],
            }],
        )
        .expect("corpus");
        let organizer_recording = hash("organizer-recording");
        let taxonomy = TaxonomyProposal::seal(
            &corpus,
            vec![TaxonomyCluster {
                cluster_id: "cluster-rust".into(),
                title: "Rust Ownership".into(),
                canonical_path: "rust/ownership.md".into(),
                document_ids: vec![document_id],
            }],
            organizer_recording,
        )
        .expect("taxonomy");
        let evidence_a = SectionEvidence {
            document_id,
            block_id: block_a.block_id,
            content_hash: block_a.content_hash,
        };
        let evidence_b = SectionEvidence {
            document_id,
            block_id: block_b.block_id,
            content_hash: block_b.content_hash,
        };
        let synthesis_recording = hash("synthesis-recording");
        let proposal = SynthesisProposal::seal(
            "cluster-rust",
            taxonomy.taxonomy_hash,
            1,
            vec![SynthesisSection {
                section_id: "ownership-model".into(),
                heading: "Ownership model".into(),
                markdown_body: "Ownership binds each value to one owner.".into(),
                evidence: vec![evidence_a.clone()],
            }],
            Vec::new(),
            vec![
                SourceDisposition {
                    target: DispositionTarget::block(document_id, &block_a),
                    disposition: DispositionKind::Integrated,
                    rationale: None,
                },
                SourceDisposition {
                    target: DispositionTarget::block(document_id, &block_b),
                    disposition: DispositionKind::Integrated,
                    rationale: None,
                },
                SourceDisposition {
                    target: DispositionTarget::metadata(document_id, &metadata),
                    disposition: DispositionKind::PreservedVerbatim,
                    rationale: None,
                },
            ],
            vec![ContradictionSet {
                contradiction_id: "compiler-versions".into(),
                summary: "Version-scoped behavior differs".into(),
                claims: vec![
                    ContradictionClaim {
                        claim_id: "old".into(),
                        rendered_claim: "Old behavior".into(),
                        observed_at: Some("2024".into()),
                        context: "old compiler".into(),
                        evidence: vec![evidence_a],
                    },
                    ContradictionClaim {
                        claim_id: "new".into(),
                        rendered_claim: "New behavior".into(),
                        observed_at: Some("2026".into()),
                        context: "new compiler".into(),
                        evidence: vec![evidence_b],
                    },
                ],
            }],
            synthesis_recording,
        )
        .expect("proposal");
        let critic_recording = hash("critic-recording");
        let critic = CriticReport::seal(
            "cluster-rust",
            proposal.proposal_hash,
            Vec::new(),
            critic_recording,
        )
        .expect("critic");
        let approval = ClusterApproval::approve(
            &proposal,
            &critic,
            "curator",
            "policy-v3",
            Vec::new(),
            Vec::new(),
        )
        .expect("approval");
        let plan = ApprovedIntegrationPlan::seal(
            corpus.clone(),
            taxonomy.clone(),
            ApprovalBinding {
                target_hash: taxonomy.taxonomy_hash,
                approved: true,
                curator_id: "curator".into(),
                policy_version: "policy-v3".into(),
                rationale: None,
            },
            vec![ApprovedClusterRevision {
                proposal,
                critic,
                approval,
            }],
            vec![organizer_recording, synthesis_recording, critic_recording],
        )
        .expect("integration plan");
        Fixture {
            corpus,
            taxonomy,
            plan,
        }
    }

    fn revision_for(
        proposal: SynthesisProposal,
        omission_approvals: Vec<OmissionApproval>,
    ) -> ApprovedClusterRevision {
        let critic = CriticReport::seal(
            &proposal.cluster_id,
            proposal.proposal_hash,
            Vec::new(),
            hash("critic-recording"),
        )
        .expect("critic");
        let approval = ClusterApproval::approve(
            &proposal,
            &critic,
            "curator",
            "policy-v3",
            omission_approvals,
            Vec::new(),
        )
        .expect("approval");
        ApprovedClusterRevision {
            proposal,
            critic,
            approval,
        }
    }

    #[test]
    fn complete_singleton_cluster_passes_all_gates() {
        fixture().plan.validate().expect("valid Schema 3 plan");
    }

    #[test]
    fn missing_disposition_fails_closed() {
        let mut fixture = fixture();
        fixture.plan.clusters[0].proposal.dispositions.pop();
        assert!(matches!(
            fixture.plan.validate(),
            Err(OkcError::ProposalInvalid(_))
        ));
    }

    #[test]
    fn integrated_blocks_must_be_cited_and_omissions_need_exact_approval() {
        let fixture = fixture();
        let original = &fixture.plan.clusters[0].proposal;
        let uncited = SynthesisProposal::seal(
            &original.cluster_id,
            original.taxonomy_hash,
            original.revision,
            original.sections.clone(),
            original.related_links.clone(),
            original.dispositions.clone(),
            Vec::new(),
            original.synthesis_recording_hash,
        )
        .expect("uncited proposal");
        let revision = revision_for(uncited, Vec::new());
        let recordings = fixture
            .plan
            .provider_recording_hashes
            .iter()
            .copied()
            .collect();
        assert!(matches!(
            validate_cluster_revision(
                &fixture.corpus,
                &fixture.taxonomy,
                &fixture.taxonomy.clusters[0],
                &revision,
                &recordings,
            ),
            Err(OkcError::ProposalInvalid(_))
        ));

        let mut dispositions = original.dispositions.clone();
        dispositions[1].disposition = DispositionKind::OmissionProposed;
        dispositions[1].rationale = Some("redundant in this approved context".into());
        let omission = SynthesisProposal::seal(
            &original.cluster_id,
            original.taxonomy_hash,
            original.revision,
            original.sections.clone(),
            original.related_links.clone(),
            dispositions,
            original.contradictions.clone(),
            original.synthesis_recording_hash,
        )
        .expect("omission proposal");
        let revision = revision_for(omission, Vec::new());
        assert!(matches!(
            validate_cluster_revision(
                &fixture.corpus,
                &fixture.taxonomy,
                &fixture.taxonomy.clusters[0],
                &revision,
                &recordings,
            ),
            Err(OkcError::ApprovalStale(_))
        ));
    }

    #[test]
    fn preserved_verbatim_blocks_are_materialized() {
        let fixture = fixture();
        let original = &fixture.plan.clusters[0].proposal;
        let mut dispositions = original.dispositions.clone();
        dispositions[1].disposition = DispositionKind::PreservedVerbatim;
        let proposal = SynthesisProposal::seal(
            &original.cluster_id,
            original.taxonomy_hash,
            original.revision,
            original.sections.clone(),
            original.related_links.clone(),
            dispositions,
            Vec::new(),
            original.synthesis_recording_hash,
        )
        .expect("preserved proposal");
        let revision = revision_for(proposal, Vec::new());
        let plan = ApprovedIntegrationPlan::seal(
            fixture.corpus,
            fixture.taxonomy.clone(),
            fixture.plan.taxonomy_approval,
            vec![revision],
            fixture.plan.provider_recording_hashes,
        )
        .expect("preserved plan");
        let temporary = tempfile::tempdir().expect("temporary");
        let output = temporary.path().join("Compiled");
        compile(&plan, &output).expect("compile");
        let note =
            fs::read_to_string(output.join("knowledge/rust/ownership.md")).expect("canonical note");
        assert!(note.contains("## Preserved source material"));
        assert!(note.contains("Block B"));
    }

    #[test]
    fn forged_evidence_and_stale_critic_fail_closed() {
        let mut forged_fixture = fixture();
        forged_fixture.plan.clusters[0].proposal.sections[0].evidence[0].content_hash =
            hash("forged");
        assert!(forged_fixture.plan.validate().is_err());

        let mut stale_fixture = fixture();
        stale_fixture.plan.clusters[0].critic.proposal_hash = hash("stale");
        assert!(matches!(
            stale_fixture.plan.validate(),
            Err(OkcError::ApprovalStale(_) | OkcError::ProposalInvalid(_))
        ));
    }

    #[test]
    fn major_critic_finding_cannot_be_waived() {
        let mut fixture = fixture();
        let proposal = &fixture.plan.clusters[0].proposal;
        fixture.plan.clusters[0].critic = CriticReport::seal(
            &proposal.cluster_id,
            proposal.proposal_hash,
            vec![CriticFinding {
                finding_id: "unsupported".into(),
                severity: CriticSeverity::Major,
                kind: CriticFindingKind::UnsupportedClaim,
                message: "claim is unsupported".into(),
                evidence: Vec::new(),
            }],
            hash("critic-recording"),
        )
        .expect("critic");
        assert!(matches!(
            fixture.plan.validate(),
            Err(OkcError::ApprovalStale(_))
        ));
    }

    #[test]
    fn taxonomy_must_cover_each_document_once() {
        let mut taxonomy = fixture().taxonomy;
        taxonomy.clusters[0].document_ids.clear();
        let corpus = fixture().corpus;
        assert!(validate_taxonomy(&corpus, &taxonomy).is_err());
    }

    #[test]
    fn provider_free_compile_and_verify_are_deterministic() {
        let plan = fixture().plan;
        let first_root = tempfile::tempdir().expect("first root");
        let second_root = tempfile::tempdir().expect("second root");
        let first = first_root.path().join("First");
        let second = second_root.path().join("Second");
        compile(&plan, &first).expect("first compile");
        compile(&plan, &second).expect("second compile");
        verify(&first).expect("verify first");
        verify(&second).expect("verify second");
        let left = inventory(&first, &[]).expect("first inventory");
        let right = inventory(&second, &[]).expect("second inventory");
        assert_eq!(left, right);
        assert!(first.join("knowledge/rust/ownership.md").is_file());
        assert!(first.join("legacy/alpha/Notes/Topic.md").is_file());
        let canonical =
            explain(&first, "knowledge/rust/ownership.md").expect("canonical provenance");
        assert_eq!(canonical.kind, "canonical_note");
        assert_eq!(canonical.evidence.len(), 2);
        let redirect = explain(&first, "legacy/alpha/Notes/Topic.md").expect("redirect provenance");
        assert_eq!(redirect.kind, "legacy_redirect");
        assert_eq!(
            redirect.source_document,
            Some(plan.corpus.documents[0].document_id)
        );
        assert_eq!(redirect.cluster_id.as_deref(), Some("cluster-rust"));
        assert_eq!(redirect.evidence.len(), 2);
        assert!(redirect.approval_hash.is_some());
        assert!(explain(&first, "missing.md").is_err());
    }

    #[test]
    fn verification_rejects_tampering_and_materialized_symlinks() {
        let plan = fixture().plan;
        let root = tempfile::tempdir().expect("root");
        let tampered = root.path().join("Tampered");
        compile(&plan, &tampered).expect("compile tamper fixture");
        fs::write(
            tampered.join("knowledge/rust/ownership.md"),
            b"untrusted replacement",
        )
        .expect("tamper materialized note");
        assert!(verify(&tampered).is_err());
        assert!(explain(&tampered, "knowledge/rust/ownership.md").is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let linked = root.path().join("Linked");
            compile(&plan, &linked).expect("compile symlink fixture");
            let note = linked.join("knowledge/rust/ownership.md");
            fs::remove_file(&note).expect("remove materialized note");
            let outside = root.path().join("outside.md");
            fs::write(&outside, b"outside").expect("outside note");
            symlink(&outside, &note).expect("replace note with symlink");
            assert!(verify(&linked).is_err());
            assert!(explain(&linked, "knowledge/rust/ownership.md").is_err());
        }
    }

    #[test]
    fn compile_rejects_plan_without_recordings_or_approval() {
        let mut plan = fixture().plan;
        plan.provider_recording_hashes.clear();
        let root = tempfile::tempdir().expect("root");
        let output = root.path().join("Compiled");
        assert!(compile(&plan, &output).is_err());
        assert!(!output.exists());
    }
}
