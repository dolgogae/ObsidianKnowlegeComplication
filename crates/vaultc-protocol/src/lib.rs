//! Provider-neutral wire types for Vault Compiler augmentation.
//!
//! The protocol is intentionally independent from the compiler crate. Object
//! identities cross the boundary as validated strings and are resolved by
//! `vaultc` against a sealed plan.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const PROPOSAL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub protocol_version: u32,
    pub request_id: String,
    pub message_type: MessageType,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(request_id: impl Into<String>, message_type: MessageType, payload: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            message_type,
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    CapabilitiesRequest,
    CapabilitiesResponse,
    AugmentationRequest,
    AugmentationResponse,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider: ProviderIdentity,
    pub protocol_versions: Vec<u32>,
    pub operations: Vec<ProviderOperation>,
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    pub structured_output: bool,
    pub streaming: bool,
    pub deterministic_controls: bool,
    pub data_boundary: DataBoundary,
}

impl ProviderCapabilities {
    pub fn supports(&self, operation: ProviderOperation) -> bool {
        self.protocol_versions.contains(&PROTOCOL_VERSION) && self.operations.contains(&operation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOperation {
    TextGeneration,
    Embedding,
    Rerank,
    KnowledgeAugmentation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataBoundary {
    Local,
    Remote { endpoint_label: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderIdentity {
    pub provider: String,
    pub model: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectRef {
    pub kind: ObjectKind,
    pub id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Snapshot,
    Document,
    Block,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRefWire {
    pub snapshot_id: String,
    pub document_id: String,
    pub block_id: Option<String>,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentProjection {
    pub snapshot_id: String,
    pub document: ObjectRef,
    pub logical_path: String,
    pub title: Option<String>,
    pub selected_blocks: Vec<ProjectedBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedBlock {
    pub block: ObjectRef,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AugmentationLimits {
    pub max_proposals: u32,
    pub max_generated_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AugmentationRequest {
    pub plan_id: String,
    pub projection_hash: String,
    pub allowed_proposal_kinds: Vec<ProposalKindName>,
    pub documents: Vec<DocumentProjection>,
    pub limits: AugmentationLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKindName {
    CreateGeneratedNote,
    ExplainConflict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeProposal {
    pub schema_version: u32,
    pub proposal_id: String,
    pub plan_id: String,
    pub projection_hash: String,
    pub provider: ProviderIdentity,
    pub kind: ProposalKind,
    pub evidence: Vec<EvidenceRefWire>,
    pub uncertainty: Option<f64>,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalKind {
    CreateGeneratedNote {
        title: String,
        markdown_body: String,
        suggested_path: Option<String>,
    },
    ExplainConflict {
        conflict_id: String,
        explanation: String,
    },
}

impl ProposalKind {
    pub fn name(&self) -> ProposalKindName {
        match self {
            Self::CreateGeneratedNote { .. } => ProposalKindName::CreateGeneratedNote,
            Self::ExplainConflict { .. } => ProposalKindName::ExplainConflict,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AugmentationResponse {
    pub proposals: Vec<KnowledgeProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptRecord {
    pub sequence: u64,
    pub request_id: String,
    pub direction: TranscriptDirection,
    pub message_type: MessageType,
    pub canonical_payload_hash: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptDirection {
    Request,
    Response,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_round_trip() {
        let capabilities = ProviderCapabilities {
            provider: ProviderIdentity {
                provider: "local".into(),
                model: "fixture".into(),
                version: Some("1".into()),
            },
            protocol_versions: vec![PROTOCOL_VERSION],
            operations: vec![ProviderOperation::KnowledgeAugmentation],
            max_input_bytes: 1024,
            max_output_bytes: 1024,
            structured_output: true,
            streaming: false,
            deterministic_controls: true,
            data_boundary: DataBoundary::Local,
        };

        let encoded = serde_json::to_vec(&capabilities).expect("serialize capabilities");
        let decoded: ProviderCapabilities =
            serde_json::from_slice(&encoded).expect("deserialize capabilities");
        assert_eq!(decoded, capabilities);
        assert!(decoded.supports(ProviderOperation::KnowledgeAugmentation));
    }

    #[test]
    fn document_projection_requires_snapshot_binding() {
        let projection = DocumentProjection {
            snapshot_id: "snap_fixture".into(),
            document: ObjectRef {
                kind: ObjectKind::Document,
                id: "doc_fixture".into(),
                content_hash: "00".repeat(32),
            },
            logical_path: "Topic.md".into(),
            title: Some("Topic".into()),
            selected_blocks: Vec::new(),
        };
        let encoded = serde_json::to_value(&projection).expect("serialize document projection");
        assert_eq!(encoded["snapshot_id"], "snap_fixture");
        assert_eq!(
            serde_json::from_value::<DocumentProjection>(encoded.clone())
                .expect("deserialize document projection"),
            projection
        );

        let mut missing_snapshot = encoded;
        missing_snapshot
            .as_object_mut()
            .expect("projection object")
            .remove("snapshot_id");
        assert!(
            serde_json::from_value::<DocumentProjection>(missing_snapshot).is_err(),
            "snapshot binding is a required V1 field"
        );
    }
}
