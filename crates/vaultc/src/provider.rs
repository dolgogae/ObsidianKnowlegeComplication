use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use vaultc_protocol::{
    DataBoundary, EvidenceRefWire, KnowledgeProposal, PROPOSAL_SCHEMA_VERSION, ProposalKind,
    ProviderCapabilities, ProviderOperation,
};

use crate::canonical::canonical_hash;
use crate::config::CompilerPolicy;
use crate::error::{Result, VaultcError};
use crate::identity::{BlockId, ContentHash, DocumentId, SnapshotId};
use crate::plan::DraftPlan;
use crate::snapshot::validate_output_logical_path;

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextGenerationRequest {
    pub prompt: String,
    pub max_output_bytes: u64,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextGenerationResponse {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub texts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub vectors: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankRequest {
    pub query: String,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankResponse {
    pub scores: Vec<f32>,
}

pub trait TextGenerator: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    fn generate(
        &self,
        request: &TextGenerationRequest,
        cancellation: &CancellationToken,
    ) -> Result<TextGenerationResponse>;
}

pub trait EmbeddingProvider: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    fn embed(
        &self,
        request: &EmbeddingRequest,
        cancellation: &CancellationToken,
    ) -> Result<EmbeddingResponse>;
}

pub trait RerankProvider: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    fn rerank(
        &self,
        request: &RerankRequest,
        cancellation: &CancellationToken,
    ) -> Result<RerankResponse>;
}

pub trait KnowledgeAugmentor: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    fn propose(
        &self,
        request: &vaultc_protocol::AugmentationRequest,
        cancellation: &CancellationToken,
    ) -> Result<Vec<KnowledgeProposal>>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalValidation {
    pub proposal: KnowledgeProposal,
    pub content_hash: ContentHash,
    pub valid: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ValidatedProposals {
    pub validations: Vec<ProposalValidation>,
    pub transcript: Vec<vaultc_protocol::TranscriptRecord>,
}

impl ValidatedProposals {
    pub fn valid(&self) -> impl Iterator<Item = &ProposalValidation> {
        self.validations
            .iter()
            .filter(|validation| validation.valid)
    }
}

pub fn validate_capabilities(
    capabilities: &ProviderCapabilities,
    policy: &CompilerPolicy,
) -> Result<()> {
    if !capabilities.supports(ProviderOperation::KnowledgeAugmentation) {
        return Err(VaultcError::Provider(
            "provider does not support protocol V1 knowledge augmentation".into(),
        ));
    }
    if matches!(capabilities.data_boundary, DataBoundary::Remote { .. })
        && !policy.augmentation.allow_remote_providers
    {
        return Err(VaultcError::Provider(
            "remote providers are denied by policy".into(),
        ));
    }
    Ok(())
}

// Proposal validation deliberately keeps every fail-closed rule in one pass so
// callers receive a complete, deterministically ordered rejection record.
#[allow(clippy::too_many_lines)]
pub fn validate_proposals(
    plan: &DraftPlan,
    proposals: Vec<KnowledgeProposal>,
    policy: &CompilerPolicy,
) -> Result<ValidatedProposals> {
    if proposals.len() > policy.augmentation.max_proposals as usize {
        return Err(VaultcError::ResourceLimit(format!(
            "provider returned {} proposals; maximum is {}",
            proposals.len(),
            policy.augmentation.max_proposals
        )));
    }
    let mut proposal_id_counts = BTreeMap::new();
    for proposal in &proposals {
        *proposal_id_counts
            .entry(proposal.proposal_id.clone())
            .or_insert(0_usize) += 1;
    }
    let mut validations = Vec::with_capacity(proposals.len());
    for proposal in proposals {
        let content_hash = canonical_hash("vaultc:proposal:v1\0", &proposal)?;
        let mut reasons = Vec::new();
        if proposal.schema_version != PROPOSAL_SCHEMA_VERSION {
            reasons.push(format!(
                "unsupported proposal schema version {}",
                proposal.schema_version
            ));
        }
        if proposal.plan_id != plan.plan_id.to_string() {
            reasons.push("proposal plan ID does not match sealed plan".into());
        }
        if proposal.projection_hash != plan.projection_hash.hex() {
            reasons.push("proposal projection hash does not match plan projection".into());
        }
        if proposal.proposal_id.is_empty() || proposal.proposal_id.len() > 256 {
            reasons.push("proposal ID must contain 1..=256 bytes".into());
        }
        if proposal_id_counts
            .get(proposal.proposal_id.as_str())
            .is_some_and(|count| *count > 1)
        {
            reasons.push("proposal ID is duplicated in the provider response".into());
        }
        validate_provider_identity(&proposal.provider, &mut reasons);
        if proposal
            .uncertainty
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            reasons.push("uncertainty must be finite and between 0 and 1".into());
        }
        match &proposal.kind {
            ProposalKind::CreateGeneratedNote {
                title,
                markdown_body,
                suggested_path,
            } => {
                if title.trim().is_empty() {
                    reasons.push("generated note title cannot be empty".into());
                }
                if title.len() > 1024 {
                    reasons.push("generated note title exceeds 1024 bytes".into());
                }
                if markdown_body.len() as u64 > policy.augmentation.max_generated_bytes {
                    reasons.push("generated note exceeds configured byte limit".into());
                }
                if proposal.evidence.is_empty() {
                    reasons.push("generated note requires at least one evidence reference".into());
                }
                if let Some(path) = suggested_path
                    && !path.to_ascii_lowercase().ends_with(".md")
                {
                    reasons.push("generated note path must end in .md".into());
                }
                if let Some(full) = crate::provenance::generated_output_path(&proposal)
                    && let Err(error) = validate_output_logical_path(&full, policy)
                {
                    reasons.push(error.to_string());
                }
            }
            ProposalKind::ExplainConflict {
                conflict_id,
                explanation,
            } => {
                if !plan
                    .conflicts
                    .iter()
                    .any(|conflict| conflict.conflict_id == *conflict_id)
                {
                    reasons.push("explanation references an unknown conflict".into());
                }
                if explanation.trim().is_empty() {
                    reasons.push("conflict explanation cannot be empty".into());
                }
            }
        }
        for evidence in &proposal.evidence {
            validate_evidence(plan, evidence, &mut reasons);
        }
        validations.push(ProposalValidation {
            proposal,
            content_hash,
            valid: reasons.is_empty(),
            reasons,
        });
    }
    validations.sort_by(|left, right| {
        left.proposal
            .proposal_id
            .as_bytes()
            .cmp(right.proposal.proposal_id.as_bytes())
    });
    Ok(ValidatedProposals {
        validations,
        transcript: Vec::new(),
    })
}

fn validate_provider_identity(
    identity: &vaultc_protocol::ProviderIdentity,
    reasons: &mut Vec<String>,
) {
    for (field, value) in [
        ("provider", identity.provider.as_str()),
        ("model", identity.model.as_str()),
    ] {
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            reasons.push(format!(
                "provider identity {field} must contain 1..=256 non-control UTF-8 bytes"
            ));
        }
    }
    if identity.version.as_deref().is_some_and(|value| {
        value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
    }) {
        reasons
            .push("provider identity version must contain 1..=256 non-control UTF-8 bytes".into());
    }
}

fn validate_evidence(plan: &DraftPlan, evidence: &EvidenceRefWire, reasons: &mut Vec<String>) {
    let snapshot = evidence.snapshot_id.parse::<SnapshotId>();
    let document = evidence.document_id.parse::<DocumentId>();
    let content_hash = ContentHash::parse_hex(&evidence.content_hash);
    let (Ok(snapshot_id), Ok(document_id), Ok(content_hash)) = (snapshot, document, content_hash)
    else {
        reasons.push("evidence contains a malformed identity".into());
        return;
    };
    let Some(document) = plan.workspace.documents.get(&document_id) else {
        reasons.push(format!(
            "evidence document `{document_id}` is not in the plan"
        ));
        return;
    };
    if document.source_file.snapshot_id != snapshot_id {
        reasons.push("evidence snapshot does not own the referenced document".into());
    }
    let Some(block_id) = evidence.block_id.as_deref() else {
        if document.source_file.content_hash != content_hash && document.body_hash != content_hash {
            reasons.push("evidence document content hash is stale".into());
        }
        if evidence.byte_start.is_some() || evidence.byte_end.is_some() {
            reasons.push("file-level evidence must not include a byte span".into());
        }
        return;
    };
    let Ok(block_id) = block_id.parse::<BlockId>() else {
        reasons.push("evidence block ID is malformed".into());
        return;
    };
    let Some(block) = document
        .blocks
        .iter()
        .find(|item| item.block_id == block_id)
    else {
        reasons.push("evidence block is not in the document".into());
        return;
    };
    if block.content_hash != content_hash {
        reasons.push("evidence block content hash is stale".into());
    }
    match (evidence.byte_start, evidence.byte_end) {
        (None, None) => {}
        (Some(start), Some(end))
            if start == block.span.byte_start && end == block.span.byte_end => {}
        _ => reasons.push("evidence span does not match the referenced block".into()),
    }
}

pub fn default_provider_timeout() -> Duration {
    Duration::from_mins(2)
}
