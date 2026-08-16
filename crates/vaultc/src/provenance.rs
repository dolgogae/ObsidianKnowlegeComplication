use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;
use std::fs;
use std::path::Path;

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use vaultc_protocol::{ProposalKindName, ProviderIdentity};

use crate::approval::{ApprovalDecision, ApprovedPlan, ConflictDecision, ProposalMaterialization};
use crate::canonical::to_canonical_json;
use crate::error::{Result, VaultcError};
use crate::identity::{
    BlockId, ContentHash, DocumentId, EvidenceId, OperationId, PlanId, RecordId, SnapshotId,
    SourceFileId,
};
use crate::ir::{FileKind, SourceFile};
use crate::plan::{ConflictKind, ConflictSubject, OutputOperation};
use crate::source::SourceId;

pub const PROVENANCE_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_PAGE_RECORDS: usize = 256;
pub const MAX_PAGE_RECORDS: usize = 4_096;
pub const DEFAULT_PAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PAGE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_LEDGER_RECORD_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_LEDGER_RECORDS: usize = 2_000_000;
pub(crate) const MAX_LEDGER_BYTES: usize = 512 * 1024 * 1024;
const ENVELOPE_PATHS: [&str; 3] = [
    ".vaultc/provenance.jsonl",
    ".vaultc/manifest.json",
    ".vaultc/checksums.txt",
];
const STORED_AUDIT_PATHS: [&str; 4] = [
    ".vaultc/plan.json",
    ".vaultc/conflicts.json",
    ".vaultc/diagnostics.json",
    ".vaultc/ai-transcript.jsonl",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProvenanceSubject {
    ArtifactPath { path: String },
    Package,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceQuery {
    pub subject: ProvenanceSubject,
    pub limit: usize,
    pub max_bytes: usize,
    pub cursor: Option<String>,
}

impl ProvenanceQuery {
    #[must_use]
    pub fn artifact_path(path: impl Into<String>) -> Self {
        Self {
            subject: ProvenanceSubject::ArtifactPath { path: path.into() },
            limit: DEFAULT_PAGE_RECORDS,
            max_bytes: DEFAULT_PAGE_BYTES,
            cursor: None,
        }
    }

    #[must_use]
    pub fn package() -> Self {
        Self {
            subject: ProvenanceSubject::Package,
            limit: DEFAULT_PAGE_RECORDS,
            max_bytes: DEFAULT_PAGE_BYTES,
            cursor: None,
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Result<Self> {
        if !(1..=MAX_PAGE_RECORDS).contains(&limit) {
            return Err(VaultcError::InvalidConfig(format!(
                "provenance page limit must be within 1..={MAX_PAGE_RECORDS}"
            )));
        }
        self.limit = limit;
        Ok(self)
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Result<Self> {
        if !(1..=MAX_PAGE_BYTES).contains(&max_bytes) {
            return Err(VaultcError::InvalidConfig(format!(
                "provenance page byte limit must be within 1..={MAX_PAGE_BYTES}"
            )));
        }
        self.max_bytes = max_bytes;
        Ok(self)
    }

    #[must_use]
    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenancePage {
    pub schema_version: u32,
    pub graph_hash: ContentHash,
    pub subject: ProvenanceSubject,
    pub records: Vec<ProvenanceRecord>,
    pub next_cursor: Option<String>,
    pub complete: bool,
}

impl ProvenancePage {
    /// Exact compact canonical response bytes governed by
    /// `ProvenanceQuery::max_bytes`. Presentation-specific pretty printing is
    /// intentionally outside this SDK response bound.
    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>> {
        to_canonical_json(self)
    }

    pub fn canonical_json_len(&self) -> Result<usize> {
        self.canonical_json_bytes().map(|bytes| bytes.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceExplanation {
    pub schema_version: u32,
    pub graph_hash: ContentHash,
    pub subject: ProvenanceSubject,
    pub records: Vec<ProvenanceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceRecord {
    pub schema_version: u32,
    pub record_id: RecordId,
    pub kind: ProvenanceRecordKind,
}

impl ProvenanceRecord {
    pub fn new(kind: ProvenanceRecordKind) -> Result<Self> {
        let record_id = calculate_record_id(PROVENANCE_SCHEMA_VERSION, &kind)?;
        Ok(Self {
            schema_version: PROVENANCE_SCHEMA_VERSION,
            record_id,
            kind,
        })
    }

    pub fn validate_identity(&self) -> Result<()> {
        if self.schema_version != PROVENANCE_SCHEMA_VERSION {
            return Err(VaultcError::VerificationFailed(format!(
                "unsupported provenance record schema {}",
                self.schema_version
            )));
        }
        let expected = calculate_record_id(self.schema_version, &self.kind)?;
        if self.record_id != expected {
            return Err(VaultcError::VerificationFailed(format!(
                "provenance record {} has a stale identity",
                self.record_id
            )));
        }
        Ok(())
    }

    #[must_use]
    pub fn type_order(&self) -> u8 {
        self.kind.type_order()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ProvenanceRecordKind {
    Source(SourceRecord),
    Operation(OperationRecord),
    Decision(DecisionRecord),
    Proposal(ProposalRecord),
    Approval(ApprovalRecord),
    Output(OutputRecord),
    Edge(EdgeRecord),
}

impl ProvenanceRecordKind {
    #[must_use]
    pub fn type_order(&self) -> u8 {
        match self {
            Self::Source(_) => 0,
            Self::Operation(_) => 1,
            Self::Decision(_) => 2,
            Self::Proposal(_) => 3,
            Self::Approval(_) => 4,
            Self::Output(_) => 5,
            Self::Edge(_) => 6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SourceRecord {
    VaultFile(VaultFileSourceRecord),
    Evidence(EvidenceSourceRecord),
    BuildInput(BuildInputSourceRecord),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultFileSourceRecord {
    pub source_id: SourceId,
    pub snapshot_id: SnapshotId,
    pub source_file_id: SourceFileId,
    pub source_path: String,
    pub file_kind: FileKind,
    pub byte_len: u64,
    pub content_hash: ContentHash,
    pub document_id: Option<DocumentId>,
    pub attribution: AttributionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSourceRecord {
    pub evidence_id: EvidenceId,
    pub snapshot_id: SnapshotId,
    pub document_id: DocumentId,
    pub source_file_id: SourceFileId,
    pub source_path: String,
    pub source_content_hash: ContentHash,
    pub block_id: Option<BlockId>,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub content_hash: ContentHash,
    pub attribution: AttributionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildInputKind {
    Plan,
    Policy,
    Toolchain,
    InnerArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildInputSourceRecord {
    pub input_kind: BuildInputKind,
    pub identity: String,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributionKind {
    Author,
    License,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributionDeclaration {
    pub kind: AttributionKind,
    pub source_key: String,
    pub value: Value,
    pub source_file_id: SourceFileId,
    pub document_id: DocumentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AttributionState {
    NotDeclared,
    Declared {
        declarations: Vec<AttributionDeclaration>,
    },
    Opaque {
        frontmatter_hash: ContentHash,
    },
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OperationRecord {
    Copy {
        operation: OutputOperation,
    },
    RewriteMarkdown {
        operation: OutputOperation,
    },
    RewriteCanvas {
        operation: OutputOperation,
    },
    Deduplicate {
        operation: OutputOperation,
        member_count: u64,
    },
    Generate {
        operation_id: OperationId,
        plan_id: PlanId,
        proposal_id: String,
        proposal_content_hash: ContentHash,
        destination: String,
        body_hash: ContentHash,
        expected_output_hash: ContentHash,
        evidence_ids: Vec<EvidenceId>,
    },
    SerializeAudit {
        role: OutputRole,
        plan_id: PlanId,
        commitment: ContentHash,
        compiler_version: String,
        policy_hash: ContentHash,
    },
    Package {
        outer_raw_sha256: String,
        byte_len: u64,
        inner_artifact_id: ContentHash,
        profile: String,
        members: Vec<PackageMemberCommitment>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMemberCommitment {
    pub path: String,
    pub byte_len: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    pub decision: ConflictDecision,
    pub conflict_kind: ConflictKind,
    pub subject: ConflictSubject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalRecord {
    pub proposal_id: String,
    pub plan_id: PlanId,
    pub projection_hash: ContentHash,
    pub content_hash: ContentHash,
    pub provider: ProviderIdentity,
    pub proposal_kind: ProposalKindName,
    pub transcript_hash: ContentHash,
    pub valid: bool,
    pub reasons: Vec<String>,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRecord {
    pub decision: ApprovalDecision,
    pub materialization: ProposalMaterialization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputRole {
    Content,
    AuditPlan,
    AuditConflicts,
    AuditDiagnostics,
    AuditTranscript,
    Provenance,
    Manifest,
    Checksums,
    Package,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStorage {
    Stored,
    VirtualAuditEnvelope,
    VirtualPackage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputRecord {
    pub subject: ProvenanceSubject,
    pub content_hash: ContentHash,
    pub byte_len: u64,
    pub role: OutputRole,
    pub storage: OutputStorage,
    pub producing_operation: RecordId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRelation {
    DerivedFrom,
    SupportedBy,
    RewrittenFrom,
    Deduplicates,
    ApprovedBy,
    DecidedBy,
    PackagedAs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EdgePosition {
    Unordered,
    Ordered { index: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeRecord {
    pub relation: EdgeRelation,
    pub from: RecordId,
    pub to: RecordId,
    pub position: EdgePosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GraphFile {
    pub path: String,
    pub byte_len: u64,
    pub content_hash: ContentHash,
}

#[derive(Serialize)]
struct RecordIdentity<'a> {
    schema_version: u32,
    kind: &'a ProvenanceRecordKind,
}

fn calculate_record_id(schema_version: u32, kind: &ProvenanceRecordKind) -> Result<RecordId> {
    let identity = RecordIdentity {
        schema_version,
        kind,
    };
    let bytes = to_canonical_json(&identity)?;
    Ok(RecordId::from_hash(ContentHash::from_domain_bytes(
        "vaultc:provenance:v1\0",
        &bytes,
    )))
}

/// ADR-0010's stored-graph commitment over the exact canonical JSONL bytes.
#[must_use]
pub fn stored_graph_hash(jsonl: &[u8]) -> ContentHash {
    ContentHash::from_domain_bytes("vaultc:provenance-graph:v1\0", jsonl)
}

fn explanation_graph_hash(jsonl: &[u8]) -> ContentHash {
    ContentHash::from_domain_bytes("vaultc:provenance-explanation:v1\0", jsonl)
}

fn subject_hash(subject: &ProvenanceSubject) -> Result<ContentHash> {
    let mut payload = Vec::new();
    match subject {
        ProvenanceSubject::ArtifactPath { path } => {
            payload.push(0);
            let len = u64::try_from(path.len()).map_err(|_| {
                VaultcError::ResourceLimit("provenance subject path length overflow".into())
            })?;
            payload.extend(len.to_be_bytes());
            payload.extend(path.as_bytes());
        }
        ProvenanceSubject::Package => payload.push(1),
    }
    Ok(ContentHash::from_domain_bytes(
        "vaultc:provenance-subject:v1\0",
        &payload,
    ))
}

fn cursor_value(
    graph_hash: ContentHash,
    subject: &ProvenanceSubject,
    last: &ProvenanceRecord,
) -> Result<String> {
    let subject_hash = subject_hash(subject)?;
    let mut hasher = Sha256::new();
    hasher.update(b"vaultc:provenance-cursor:v1\0");
    hasher.update(graph_hash.as_bytes());
    hasher.update(subject_hash.as_bytes());
    hasher.update([last.type_order()]);
    hasher.update(last.record_id.hash().as_bytes());
    Ok(format!("cursor_{}", hex::encode(hasher.finalize())))
}

fn record_cmp(left: &ProvenanceRecord, right: &ProvenanceRecord) -> std::cmp::Ordering {
    left.type_order().cmp(&right.type_order()).then_with(|| {
        left.record_id
            .hash()
            .as_bytes()
            .cmp(right.record_id.hash().as_bytes())
    })
}

pub(crate) fn encode_jsonl(records: &[ProvenanceRecord]) -> Result<Vec<u8>> {
    if records.len() > MAX_LEDGER_RECORDS {
        return Err(VaultcError::ResourceLimit(format!(
            "provenance graph exceeds {MAX_LEDGER_RECORDS} records"
        )));
    }
    let mut output = Vec::new();
    for record in records {
        let encoded = to_canonical_json(record)?;
        if encoded.len() > MAX_LEDGER_RECORD_BYTES {
            return Err(VaultcError::ResourceLimit(format!(
                "provenance record {} exceeds {MAX_LEDGER_RECORD_BYTES} bytes",
                record.record_id
            )));
        }
        let next_len = output
            .len()
            .checked_add(encoded.len())
            .and_then(|length| length.checked_add(1))
            .ok_or_else(|| VaultcError::ResourceLimit("provenance graph length overflow".into()))?;
        if next_len > MAX_LEDGER_BYTES {
            return Err(VaultcError::ResourceLimit(format!(
                "provenance graph exceeds {MAX_LEDGER_BYTES} bytes"
            )));
        }
        output.extend(encoded);
        output.push(b'\n');
    }
    Ok(output)
}

struct UniqueJsonValue(Value);

impl<'de> Deserialize<'de> for UniqueJsonValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor)
    }
}

struct UniqueJsonVisitor;

impl<'de> Visitor<'de> for UniqueJsonVisitor {
    type Value = UniqueJsonValue;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let number = serde_json::Number::from_f64(value)
            .ok_or_else(|| E::custom("JSON number must be finite"))?;
        Ok(UniqueJsonValue(Value::Number(number)))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(UniqueJsonValue(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(UniqueJsonValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut object: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate JSON object key `{key}`"
                )));
            }
            let UniqueJsonValue(value) = object.next_value()?;
            values.insert(key, value);
        }
        Ok(UniqueJsonValue(Value::Object(values)))
    }
}

pub(crate) fn decode_jsonl(bytes: &[u8], path: &Path) -> Result<Vec<ProvenanceRecord>> {
    if bytes.len() > MAX_LEDGER_BYTES {
        return Err(VaultcError::ResourceLimit(format!(
            "provenance graph exceeds {MAX_LEDGER_BYTES} bytes"
        )));
    }
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        return Err(VaultcError::VerificationFailed(
            "provenance JSONL must be non-empty and end with LF".into(),
        ));
    }
    if bytes.contains(&b'\r') {
        return Err(VaultcError::VerificationFailed(
            "provenance JSONL must not contain CR bytes".into(),
        ));
    }
    let mut records = Vec::new();
    for (index, raw) in bytes[..bytes.len() - 1]
        .split(|byte| *byte == b'\n')
        .enumerate()
    {
        if index >= MAX_LEDGER_RECORDS {
            return Err(VaultcError::ResourceLimit(format!(
                "provenance graph exceeds {MAX_LEDGER_RECORDS} records"
            )));
        }
        if raw.is_empty() {
            return Err(VaultcError::VerificationFailed(format!(
                "blank provenance line {}",
                index + 1
            )));
        }
        if raw.len() > MAX_LEDGER_RECORD_BYTES {
            return Err(VaultcError::ResourceLimit(format!(
                "provenance line {} exceeds {MAX_LEDGER_RECORD_BYTES} bytes",
                index + 1
            )));
        }
        let mut deserializer = serde_json::Deserializer::from_slice(raw);
        let UniqueJsonValue(value) =
            UniqueJsonValue::deserialize(&mut deserializer).map_err(|error| {
                VaultcError::MalformedInput {
                    path: path.display().to_string(),
                    reason: format!("line {}: {error}", index + 1),
                }
            })?;
        deserializer
            .end()
            .map_err(|error| VaultcError::MalformedInput {
                path: path.display().to_string(),
                reason: format!("line {}: {error}", index + 1),
            })?;
        let record: ProvenanceRecord =
            serde_json::from_value(value).map_err(|error| VaultcError::MalformedInput {
                path: path.display().to_string(),
                reason: format!("line {}: {error}", index + 1),
            })?;
        if to_canonical_json(&record)? != raw {
            return Err(VaultcError::VerificationFailed(format!(
                "provenance line {} is not canonical JSON",
                index + 1
            )));
        }
        record.validate_identity()?;
        if let Some(previous) = records.last()
            && record_cmp(previous, &record) != std::cmp::Ordering::Less
        {
            return Err(VaultcError::VerificationFailed(format!(
                "provenance line {} violates strict type/RecordId order",
                index + 1
            )));
        }
        records.push(record);
    }
    Ok(records)
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

// The graph builder, structural validator, virtual envelope, and paged query
// implementation follow below. Keeping their public data model above makes
// schema review independent from construction details.

#[derive(Default)]
struct GraphBuilder {
    records: BTreeMap<RecordId, ProvenanceRecord>,
}

impl GraphBuilder {
    fn insert(&mut self, kind: ProvenanceRecordKind) -> Result<RecordId> {
        let record = ProvenanceRecord::new(kind)?;
        match self.records.entry(record.record_id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                let record_id = record.record_id;
                entry.insert(record);
                Ok(record_id)
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                if entry.get() != &record {
                    return Err(VaultcError::Internal(format!(
                        "provenance RecordId collision at {}",
                        record.record_id
                    )));
                }
                Ok(record.record_id)
            }
        }
    }

    fn edge(
        &mut self,
        relation: EdgeRelation,
        from: RecordId,
        to: RecordId,
        position: EdgePosition,
    ) -> Result<RecordId> {
        if from == to {
            return Err(VaultcError::Internal(
                "provenance builder attempted a self-edge".into(),
            ));
        }
        self.insert(ProvenanceRecordKind::Edge(EdgeRecord {
            relation,
            from,
            to,
            position,
        }))
    }

    fn finish(self) -> Vec<ProvenanceRecord> {
        let mut records: Vec<_> = self.records.into_values().collect();
        records.sort_by(record_cmp);
        records
    }
}

fn attribution_state(document: &crate::ir::Document) -> Result<AttributionState> {
    let absent_hash = ContentHash::from_domain_bytes("vaultc:frontmatter:v1\0", b"null");
    let Some(Value::Object(frontmatter)) = &document.frontmatter else {
        return Ok(if document.frontmatter_hash == absent_hash {
            AttributionState::NotDeclared
        } else {
            AttributionState::Opaque {
                frontmatter_hash: document.frontmatter_hash,
            }
        });
    };

    let mut declarations = Vec::new();
    for (source_key, value) in frontmatter {
        let kind = if source_key.eq_ignore_ascii_case("author")
            || source_key.eq_ignore_ascii_case("authors")
        {
            Some(AttributionKind::Author)
        } else if source_key.eq_ignore_ascii_case("license")
            || source_key.eq_ignore_ascii_case("licenses")
        {
            Some(AttributionKind::License)
        } else {
            None
        };
        if let Some(kind) = kind {
            declarations.push(AttributionDeclaration {
                kind,
                source_key: source_key.clone(),
                value: value.clone(),
                source_file_id: document.source_file.file_id,
                document_id: document.document_id,
            });
        }
    }
    if declarations.is_empty() {
        return Ok(AttributionState::NotDeclared);
    }
    let mut keyed = declarations
        .into_iter()
        .map(|declaration| {
            let canonical = to_canonical_json(&declaration.value)?;
            Ok((
                declaration.kind,
                declaration.source_key.as_bytes().to_vec(),
                canonical,
                declaration,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    keyed.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    keyed.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1 && left.2 == right.2);
    Ok(AttributionState::Declared {
        declarations: keyed
            .into_iter()
            .map(|(_, _, _, declaration)| declaration)
            .collect(),
    })
}

fn vault_file_source(
    file: &SourceFile,
    document: Option<&crate::ir::Document>,
) -> Result<SourceRecord> {
    Ok(SourceRecord::VaultFile(VaultFileSourceRecord {
        source_id: file.source_id.clone(),
        snapshot_id: file.snapshot_id,
        source_file_id: file.file_id,
        source_path: file.logical_path.clone(),
        file_kind: file.kind.clone(),
        byte_len: file.byte_len,
        content_hash: file.content_hash,
        document_id: document.map(|document| document.document_id),
        attribution: match document {
            Some(document) => attribution_state(document)?,
            None => AttributionState::NotApplicable,
        },
    }))
}

fn evidence_source(
    approved: &ApprovedPlan,
    evidence: &vaultc_protocol::EvidenceRefWire,
) -> Result<SourceRecord> {
    let snapshot_id = evidence.snapshot_id.parse::<SnapshotId>()?;
    let document_id = evidence.document_id.parse::<DocumentId>()?;
    let document = approved
        .plan
        .workspace
        .documents
        .get(&document_id)
        .ok_or_else(|| {
            VaultcError::ApprovalStale(format!(
                "evidence references unknown document {document_id}"
            ))
        })?;
    if document.source_file.snapshot_id != snapshot_id {
        return Err(VaultcError::ApprovalStale(format!(
            "evidence snapshot is stale for document {document_id}"
        )));
    }
    Ok(SourceRecord::Evidence(EvidenceSourceRecord {
        evidence_id: EvidenceId::from_evidence(evidence)?,
        snapshot_id,
        document_id,
        source_file_id: document.source_file.file_id,
        source_path: document.source_file.logical_path.clone(),
        source_content_hash: document.source_file.content_hash,
        block_id: evidence
            .block_id
            .as_deref()
            .map(str::parse::<BlockId>)
            .transpose()?,
        byte_start: evidence.byte_start,
        byte_end: evidence.byte_end,
        content_hash: ContentHash::parse_hex(&evidence.content_hash)?,
        attribution: attribution_state(document)?,
    }))
}

fn output_role(path: &str) -> OutputRole {
    match path {
        ".vaultc/plan.json" => OutputRole::AuditPlan,
        ".vaultc/conflicts.json" => OutputRole::AuditConflicts,
        ".vaultc/diagnostics.json" => OutputRole::AuditDiagnostics,
        ".vaultc/ai-transcript.jsonl" => OutputRole::AuditTranscript,
        ".vaultc/provenance.jsonl" => OutputRole::Provenance,
        ".vaultc/manifest.json" => OutputRole::Manifest,
        ".vaultc/checksums.txt" => OutputRole::Checksums,
        _ => OutputRole::Content,
    }
}

fn file_map(files: &[GraphFile]) -> Result<BTreeMap<&str, &GraphFile>> {
    let mut output = BTreeMap::new();
    let mut previous: Option<&str> = None;
    for file in files {
        if previous.is_some_and(|path| path.as_bytes() >= file.path.as_bytes()) {
            return Err(VaultcError::Internal(
                "graph input files must be strictly path-sorted".into(),
            ));
        }
        previous = Some(&file.path);
        if ENVELOPE_PATHS.contains(&file.path.as_str()) {
            return Err(VaultcError::Internal(format!(
                "stored graph input unexpectedly contains envelope path `{}`",
                file.path
            )));
        }
        output.insert(file.path.as_str(), file);
    }
    Ok(output)
}

struct ProposalGraphInfo {
    record_id: RecordId,
    evidence_sources: Vec<RecordId>,
}

struct DecisionGraphInfo {
    record_id: RecordId,
    subject: ConflictSubject,
}

fn add_decision_records(
    approved: &ApprovedPlan,
    builder: &mut GraphBuilder,
) -> Result<BTreeMap<String, DecisionGraphInfo>> {
    let mut output = BTreeMap::new();
    for decision in &approved.conflict_decisions {
        let conflict = approved
            .plan
            .conflicts
            .iter()
            .find(|conflict| conflict.conflict_id == decision.conflict_id)
            .ok_or_else(|| {
                VaultcError::ApprovalStale(format!(
                    "decision `{}` references an unknown conflict",
                    decision.conflict_id
                ))
            })?;
        let subject = conflict.subject.clone().ok_or_else(|| {
            VaultcError::ApprovalStale(format!(
                "decision `{}` has no typed conflict subject",
                decision.conflict_id
            ))
        })?;
        let record_id = builder.insert(ProvenanceRecordKind::Decision(DecisionRecord {
            decision: decision.clone(),
            conflict_kind: conflict.kind,
            subject: subject.clone(),
        }))?;
        if output
            .insert(
                decision.conflict_id.clone(),
                DecisionGraphInfo { record_id, subject },
            )
            .is_some()
        {
            return Err(VaultcError::ApprovalStale(format!(
                "duplicate decision `{}`",
                decision.conflict_id
            )));
        }
    }
    Ok(output)
}

fn add_proposal_records(
    approved: &ApprovedPlan,
    transcript_hash: ContentHash,
    builder: &mut GraphBuilder,
) -> Result<BTreeMap<String, ProposalGraphInfo>> {
    let mut output = BTreeMap::new();
    for validation in approved
        .proposal_validations
        .iter()
        .filter(|validation| validation.valid)
    {
        let evidence_ids = validation
            .proposal
            .evidence
            .iter()
            .map(EvidenceId::from_evidence)
            .collect::<Result<Vec<_>>>()?;
        let record_id = builder.insert(ProvenanceRecordKind::Proposal(ProposalRecord {
            proposal_id: validation.proposal.proposal_id.clone(),
            plan_id: validation.proposal.plan_id.parse::<PlanId>()?,
            projection_hash: ContentHash::parse_hex(&validation.proposal.projection_hash)?,
            content_hash: validation.content_hash,
            provider: validation.proposal.provider.clone(),
            proposal_kind: validation.proposal.kind.name(),
            transcript_hash,
            valid: validation.valid,
            reasons: validation.reasons.clone(),
            evidence_ids: evidence_ids.clone(),
        }))?;
        let mut evidence_sources = Vec::with_capacity(evidence_ids.len());
        for (index, evidence) in validation.proposal.evidence.iter().enumerate() {
            let source_id = builder.insert(ProvenanceRecordKind::Source(evidence_source(
                approved, evidence,
            )?))?;
            builder.edge(
                EdgeRelation::SupportedBy,
                record_id,
                source_id,
                EdgePosition::Ordered {
                    index: u64::try_from(index).map_err(|_| {
                        VaultcError::ResourceLimit("proposal evidence count overflow".into())
                    })?,
                },
            )?;
            evidence_sources.push(source_id);
        }
        if output
            .insert(
                validation.proposal.proposal_id.clone(),
                ProposalGraphInfo {
                    record_id,
                    evidence_sources,
                },
            )
            .is_some()
        {
            return Err(VaultcError::ApprovalStale(format!(
                "duplicate valid proposal `{}`",
                validation.proposal.proposal_id
            )));
        }
    }
    Ok(output)
}

fn add_approval_records(
    approved: &ApprovedPlan,
    builder: &mut GraphBuilder,
) -> Result<BTreeMap<String, RecordId>> {
    let mut output = BTreeMap::new();
    for proposal in &approved.approved_proposals {
        let record_id = builder.insert(ProvenanceRecordKind::Approval(ApprovalRecord {
            decision: proposal.approval.clone(),
            materialization: proposal.materialization.clone(),
        }))?;
        if output
            .insert(proposal.proposal.proposal_id.clone(), record_id)
            .is_some()
        {
            return Err(VaultcError::ApprovalStale(format!(
                "duplicate approved proposal `{}`",
                proposal.proposal.proposal_id
            )));
        }
    }
    Ok(output)
}

struct OperationSources {
    source_records: Vec<SourceRecord>,
    document_ids: BTreeSet<DocumentId>,
    canvas_id: Option<crate::identity::CanvasId>,
}

#[allow(
    clippy::too_many_lines,
    reason = "all canonical IR source kinds share one fail-closed operation-source classifier"
)]
fn planned_operation_sources(
    approved: &ApprovedPlan,
    operation: &OutputOperation,
) -> Result<OperationSources> {
    let (source_id, snapshot_id, source_path) = match operation {
        OutputOperation::Copy {
            source_id,
            snapshot_id,
            source_path,
            ..
        }
        | OutputOperation::RewriteMarkdown {
            source_id,
            snapshot_id,
            source_path,
            ..
        }
        | OutputOperation::RewriteCanvas {
            source_id,
            snapshot_id,
            source_path,
            ..
        } => (source_id, snapshot_id, source_path),
    };

    if let Some(document) = approved.plan.workspace.documents.values().find(|document| {
        &document.source_file.source_id == source_id
            && &document.source_file.snapshot_id == snapshot_id
            && &document.source_file.logical_path == source_path
    }) {
        let members = approved
            .plan
            .exact_groups
            .iter()
            .find(|group| group.members.contains(&document.document_id))
            .map_or_else(|| vec![document.document_id], |group| group.members.clone());
        let mut source_records = Vec::with_capacity(members.len());
        let mut document_ids = BTreeSet::new();
        for document_id in members {
            let member = approved
                .plan
                .workspace
                .documents
                .get(&document_id)
                .ok_or_else(|| {
                    VaultcError::PlanStale(format!(
                        "exact group references missing document {document_id}"
                    ))
                })?;
            source_records.push(vault_file_source(&member.source_file, Some(member))?);
            document_ids.insert(document_id);
        }
        return Ok(OperationSources {
            source_records,
            document_ids,
            canvas_id: None,
        });
    }

    if let Some(asset) = approved.plan.workspace.assets.values().find(|asset| {
        asset.sources.iter().any(|file| {
            &file.source_id == source_id
                && &file.snapshot_id == snapshot_id
                && &file.logical_path == source_path
        })
    }) {
        return Ok(OperationSources {
            source_records: asset
                .sources
                .iter()
                .map(|file| vault_file_source(file, None))
                .collect::<Result<Vec<_>>>()?,
            document_ids: BTreeSet::new(),
            canvas_id: None,
        });
    }

    if let Some(canvas) = approved.plan.workspace.canvases.values().find(|canvas| {
        &canvas.source_file.source_id == source_id
            && &canvas.source_file.snapshot_id == snapshot_id
            && &canvas.source_file.logical_path == source_path
    }) {
        return Ok(OperationSources {
            source_records: vec![vault_file_source(&canvas.source_file, None)?],
            document_ids: BTreeSet::new(),
            canvas_id: Some(canvas.canvas_id),
        });
    }

    if let Some(base) = approved.plan.workspace.bases.iter().find(|base| {
        &base.source_file.source_id == source_id
            && &base.source_file.snapshot_id == snapshot_id
            && &base.source_file.logical_path == source_path
    }) {
        return Ok(OperationSources {
            source_records: vec![vault_file_source(&base.source_file, None)?],
            document_ids: BTreeSet::new(),
            canvas_id: None,
        });
    }

    let file = approved
        .plan
        .snapshots
        .iter()
        .find(|snapshot| &snapshot.snapshot_id == snapshot_id)
        .and_then(|snapshot| {
            snapshot
                .files
                .iter()
                .find(|file| &file.source_id == source_id && &file.logical_path == source_path)
        })
        .ok_or_else(|| {
            VaultcError::PlanStale(format!(
                "operation {} references an unknown source file",
                operation.operation_id()
            ))
        })?;
    Ok(OperationSources {
        source_records: vec![vault_file_source(file, None)?],
        document_ids: BTreeSet::new(),
        canvas_id: None,
    })
}

fn expected_operation_hash(operation: &OutputOperation) -> ContentHash {
    match operation {
        OutputOperation::Copy { expected_hash, .. } => *expected_hash,
        OutputOperation::RewriteMarkdown {
            expected_output_hash,
            ..
        }
        | OutputOperation::RewriteCanvas {
            expected_output_hash,
            ..
        } => *expected_output_hash,
    }
}

fn add_planned_output(
    approved: &ApprovedPlan,
    operation: &OutputOperation,
    file: &GraphFile,
    decisions: &BTreeMap<String, DecisionGraphInfo>,
    builder: &mut GraphBuilder,
) -> Result<()> {
    if file.content_hash != expected_operation_hash(operation) {
        return Err(VaultcError::IdentityMismatch(format!(
            "output `{}` disagrees with sealed operation {}",
            file.path,
            operation.operation_id()
        )));
    }
    let sources = planned_operation_sources(approved, operation)?;
    if sources.source_records.is_empty() {
        return Err(VaultcError::PlanStale(format!(
            "operation {} has no source closure",
            operation.operation_id()
        )));
    }
    let source_count = u64::try_from(sources.source_records.len())
        .map_err(|_| VaultcError::ResourceLimit("provenance source count overflow".into()))?;
    let deduplicates = source_count > 1;
    let operation_kind = if deduplicates {
        OperationRecord::Deduplicate {
            operation: operation.clone(),
            member_count: source_count,
        }
    } else {
        match operation {
            OutputOperation::Copy { .. } => OperationRecord::Copy {
                operation: operation.clone(),
            },
            OutputOperation::RewriteMarkdown { .. } => OperationRecord::RewriteMarkdown {
                operation: operation.clone(),
            },
            OutputOperation::RewriteCanvas { .. } => OperationRecord::RewriteCanvas {
                operation: operation.clone(),
            },
        }
    };
    let operation_record = builder.insert(ProvenanceRecordKind::Operation(operation_kind))?;
    let output_record = builder.insert(ProvenanceRecordKind::Output(OutputRecord {
        subject: ProvenanceSubject::ArtifactPath {
            path: file.path.clone(),
        },
        content_hash: file.content_hash,
        byte_len: file.byte_len,
        role: OutputRole::Content,
        storage: OutputStorage::Stored,
        producing_operation: operation_record,
    }))?;
    builder.edge(
        EdgeRelation::DerivedFrom,
        output_record,
        operation_record,
        EdgePosition::Unordered,
    )?;

    let source_relation = if deduplicates {
        EdgeRelation::Deduplicates
    } else if matches!(
        operation,
        OutputOperation::RewriteMarkdown { .. } | OutputOperation::RewriteCanvas { .. }
    ) {
        EdgeRelation::RewrittenFrom
    } else {
        EdgeRelation::DerivedFrom
    };
    for source in sources.source_records {
        let source_record = builder.insert(ProvenanceRecordKind::Source(source))?;
        builder.edge(
            source_relation,
            operation_record,
            source_record,
            EdgePosition::Unordered,
        )?;
    }

    for decision in decisions.values() {
        let applies = match &decision.subject {
            ConflictSubject::MarkdownLink { document_id, .. } => {
                sources.document_ids.contains(document_id)
            }
            ConflictSubject::CanvasReference { canvas_id, .. } => {
                sources.canvas_id == Some(*canvas_id)
            }
        };
        if applies {
            builder.edge(
                EdgeRelation::DecidedBy,
                operation_record,
                decision.record_id,
                EdgePosition::Unordered,
            )?;
        }
    }
    Ok(())
}

fn add_generated_outputs(
    approved: &ApprovedPlan,
    files: &BTreeMap<&str, &GraphFile>,
    proposals: &BTreeMap<String, ProposalGraphInfo>,
    approvals: &BTreeMap<String, RecordId>,
    builder: &mut GraphBuilder,
) -> Result<()> {
    for proposal in &approved.approved_proposals {
        let ProposalMaterialization::GeneratedNote {
            destination,
            body_hash,
            expected_output_hash,
            evidence_ids,
            operation_id,
        } = &proposal.materialization
        else {
            continue;
        };
        let file = files.get(destination.as_str()).ok_or_else(|| {
            VaultcError::IdentityMismatch(format!(
                "generated output `{destination}` is absent from graph inventory"
            ))
        })?;
        if file.content_hash != *expected_output_hash {
            return Err(VaultcError::IdentityMismatch(format!(
                "generated output `{destination}` disagrees with its approval"
            )));
        }
        let proposal_info = proposals
            .get(&proposal.proposal.proposal_id)
            .ok_or_else(|| {
                VaultcError::ApprovalStale(format!(
                    "approved proposal `{}` has no valid proposal record",
                    proposal.proposal.proposal_id
                ))
            })?;
        if proposal_info.evidence_sources.len() != evidence_ids.len() {
            return Err(VaultcError::ApprovalStale(format!(
                "approved proposal `{}` evidence closure is stale",
                proposal.proposal.proposal_id
            )));
        }
        let approval_record = approvals
            .get(&proposal.proposal.proposal_id)
            .copied()
            .ok_or_else(|| {
                VaultcError::ApprovalStale(format!(
                    "approved proposal `{}` has no approval record",
                    proposal.proposal.proposal_id
                ))
            })?;
        let operation_record =
            builder.insert(ProvenanceRecordKind::Operation(OperationRecord::Generate {
                operation_id: *operation_id,
                plan_id: approved.plan.plan_id,
                proposal_id: proposal.proposal.proposal_id.clone(),
                proposal_content_hash: proposal.content_hash,
                destination: destination.clone(),
                body_hash: *body_hash,
                expected_output_hash: *expected_output_hash,
                evidence_ids: evidence_ids.clone(),
            }))?;
        let output_record = builder.insert(ProvenanceRecordKind::Output(OutputRecord {
            subject: ProvenanceSubject::ArtifactPath {
                path: destination.clone(),
            },
            content_hash: file.content_hash,
            byte_len: file.byte_len,
            role: OutputRole::Content,
            storage: OutputStorage::Stored,
            producing_operation: operation_record,
        }))?;
        builder.edge(
            EdgeRelation::DerivedFrom,
            output_record,
            operation_record,
            EdgePosition::Unordered,
        )?;
        builder.edge(
            EdgeRelation::DerivedFrom,
            operation_record,
            proposal_info.record_id,
            EdgePosition::Unordered,
        )?;
        builder.edge(
            EdgeRelation::ApprovedBy,
            operation_record,
            approval_record,
            EdgePosition::Unordered,
        )?;
    }
    Ok(())
}

fn add_build_input(
    builder: &mut GraphBuilder,
    input_kind: BuildInputKind,
    identity: String,
    content_hash: ContentHash,
) -> Result<RecordId> {
    builder.insert(ProvenanceRecordKind::Source(SourceRecord::BuildInput(
        BuildInputSourceRecord {
            input_kind,
            identity,
            content_hash,
        },
    )))
}

struct AdminInputs<'a> {
    plan: RecordId,
    policy: RecordId,
    toolchain: RecordId,
    decisions: &'a BTreeMap<String, DecisionGraphInfo>,
    proposals: &'a BTreeMap<String, ProposalGraphInfo>,
    approvals: &'a BTreeMap<String, RecordId>,
}

fn add_stored_audit_outputs(
    approved: &ApprovedPlan,
    files: &BTreeMap<&str, &GraphFile>,
    inputs: &AdminInputs<'_>,
    builder: &mut GraphBuilder,
) -> Result<()> {
    let policy_hash = approved.plan.policy.semantic_hash()?;
    for path in STORED_AUDIT_PATHS {
        let file = files.get(path).ok_or_else(|| {
            VaultcError::IdentityMismatch(format!("stored audit output `{path}` is missing"))
        })?;
        let role = output_role(path);
        let operation_record = builder.insert(ProvenanceRecordKind::Operation(
            OperationRecord::SerializeAudit {
                role,
                plan_id: approved.plan.plan_id,
                commitment: file.content_hash,
                compiler_version: approved.plan.compiler_version.clone(),
                policy_hash,
            },
        ))?;
        let output_record = builder.insert(ProvenanceRecordKind::Output(OutputRecord {
            subject: ProvenanceSubject::ArtifactPath { path: path.into() },
            content_hash: file.content_hash,
            byte_len: file.byte_len,
            role,
            storage: OutputStorage::Stored,
            producing_operation: operation_record,
        }))?;
        builder.edge(
            EdgeRelation::DerivedFrom,
            output_record,
            operation_record,
            EdgePosition::Unordered,
        )?;
        for prerequisite in [inputs.plan, inputs.policy, inputs.toolchain] {
            builder.edge(
                EdgeRelation::DerivedFrom,
                operation_record,
                prerequisite,
                EdgePosition::Unordered,
            )?;
        }
        if matches!(role, OutputRole::AuditPlan | OutputRole::AuditConflicts) {
            for decision in inputs.decisions.values() {
                builder.edge(
                    EdgeRelation::DerivedFrom,
                    operation_record,
                    decision.record_id,
                    EdgePosition::Unordered,
                )?;
            }
        }
        if matches!(role, OutputRole::AuditPlan | OutputRole::AuditTranscript) {
            for proposal in inputs.proposals.values() {
                builder.edge(
                    EdgeRelation::DerivedFrom,
                    operation_record,
                    proposal.record_id,
                    EdgePosition::Unordered,
                )?;
            }
        }
        if role == OutputRole::AuditPlan {
            for approval in inputs.approvals.values() {
                builder.edge(
                    EdgeRelation::DerivedFrom,
                    operation_record,
                    *approval,
                    EdgePosition::Unordered,
                )?;
            }
        }
    }
    Ok(())
}

pub(crate) fn build_stored_records(
    approved: &ApprovedPlan,
    files: &[GraphFile],
) -> Result<Vec<ProvenanceRecord>> {
    crate::approval::validate_approved_plan(approved, &approved.plan.policy)?;
    let files = file_map(files)?;
    let mut expected_paths: BTreeSet<String> = approved
        .plan
        .operations
        .iter()
        .map(|operation| operation.destination().to_owned())
        .collect();
    for proposal in &approved.approved_proposals {
        if let ProposalMaterialization::GeneratedNote { destination, .. } =
            &proposal.materialization
        {
            expected_paths.insert(destination.clone());
        }
    }
    expected_paths.extend(STORED_AUDIT_PATHS.into_iter().map(str::to_owned));
    let actual_paths: BTreeSet<_> = files.keys().map(|path| (*path).to_owned()).collect();
    if actual_paths != expected_paths {
        return Err(VaultcError::IdentityMismatch(
            "stored graph inventory does not exactly cover pre-envelope outputs".into(),
        ));
    }

    let transcript_hash = files[".vaultc/ai-transcript.jsonl"].content_hash;
    let mut builder = GraphBuilder::default();
    let decisions = add_decision_records(approved, &mut builder)?;
    let proposals = add_proposal_records(approved, transcript_hash, &mut builder)?;
    let approvals = add_approval_records(approved, &mut builder)?;

    for operation in &approved.plan.operations {
        let file = files.get(operation.destination()).ok_or_else(|| {
            VaultcError::IdentityMismatch(format!(
                "operation output `{}` is missing",
                operation.destination()
            ))
        })?;
        add_planned_output(approved, operation, file, &decisions, &mut builder)?;
    }
    add_generated_outputs(approved, &files, &proposals, &approvals, &mut builder)?;

    let policy_hash = approved.plan.policy.semantic_hash()?;
    let plan_input = add_build_input(
        &mut builder,
        BuildInputKind::Plan,
        approved.plan.plan_id.to_string(),
        files[".vaultc/plan.json"].content_hash,
    )?;
    let policy_input = add_build_input(
        &mut builder,
        BuildInputKind::Policy,
        policy_hash.hex(),
        policy_hash,
    )?;
    let toolchain_hash = ContentHash::from_domain_bytes(
        "vaultc:toolchain:v1\0",
        approved.plan.compiler_version.as_bytes(),
    );
    let toolchain_input = add_build_input(
        &mut builder,
        BuildInputKind::Toolchain,
        approved.plan.compiler_version.clone(),
        toolchain_hash,
    )?;
    add_stored_audit_outputs(
        approved,
        &files,
        &AdminInputs {
            plan: plan_input,
            policy: policy_input,
            toolchain: toolchain_input,
            decisions: &decisions,
            proposals: &proposals,
            approvals: &approvals,
        },
        &mut builder,
    )?;
    let records = builder.finish();
    validate_graph(&records, GraphStorageMode::Stored)?;
    Ok(records)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GraphStorageMode {
    Stored,
    Logical,
}

fn validate_attribution(
    state: &AttributionState,
    expected_owner: Option<(SourceFileId, DocumentId)>,
) -> Result<()> {
    match (state, expected_owner) {
        (AttributionState::NotApplicable, None)
        | (AttributionState::NotDeclared | AttributionState::Opaque { .. }, Some(_)) => Ok(()),
        (AttributionState::Declared { declarations }, Some((file_id, document_id))) => {
            if declarations.is_empty() {
                return Err(VaultcError::VerificationFailed(
                    "declared attribution state is empty".into(),
                ));
            }
            let mut previous: Option<(AttributionKind, Vec<u8>, Vec<u8>)> = None;
            for declaration in declarations {
                if declaration.source_file_id != file_id
                    || declaration.document_id != document_id
                    || declaration.source_key.is_empty()
                    || declaration.source_key.chars().any(char::is_control)
                {
                    return Err(VaultcError::VerificationFailed(
                        "attribution declaration has stale owner or key".into(),
                    ));
                }
                let recognized = match declaration.kind {
                    AttributionKind::Author => {
                        declaration.source_key.eq_ignore_ascii_case("author")
                            || declaration.source_key.eq_ignore_ascii_case("authors")
                    }
                    AttributionKind::License => {
                        declaration.source_key.eq_ignore_ascii_case("license")
                            || declaration.source_key.eq_ignore_ascii_case("licenses")
                    }
                };
                if !recognized {
                    return Err(VaultcError::VerificationFailed(
                        "attribution declaration uses an unrecognized key".into(),
                    ));
                }
                let current = (
                    declaration.kind,
                    declaration.source_key.as_bytes().to_vec(),
                    to_canonical_json(&declaration.value)?,
                );
                if previous
                    .as_ref()
                    .is_some_and(|previous| previous >= &current)
                {
                    return Err(VaultcError::VerificationFailed(
                        "attribution declarations are not strictly canonical".into(),
                    ));
                }
                previous = Some(current);
            }
            Ok(())
        }
        (AttributionState::NotApplicable, Some(_))
        | (
            AttributionState::NotDeclared
            | AttributionState::Declared { .. }
            | AttributionState::Opaque { .. },
            None,
        ) => Err(VaultcError::VerificationFailed(
            "attribution state is invalid for its source kind".into(),
        )),
    }
}

fn validate_source_record(source: &SourceRecord) -> Result<()> {
    match source {
        SourceRecord::VaultFile(source) => {
            if source.source_path.contains('\0') {
                return Err(VaultcError::VerificationFailed(
                    "provenance source path contains NUL".into(),
                ));
            }
            validate_attribution(
                &source.attribution,
                source
                    .document_id
                    .map(|document_id| (source.source_file_id, document_id)),
            )
        }
        SourceRecord::Evidence(source) => {
            if source.source_path.contains('\0')
                || EvidenceId::from_evidence(&vaultc_protocol::EvidenceRefWire {
                    snapshot_id: source.snapshot_id.to_string(),
                    document_id: source.document_id.to_string(),
                    block_id: source.block_id.map(|block_id| block_id.to_string()),
                    byte_start: source.byte_start,
                    byte_end: source.byte_end,
                    content_hash: source.content_hash.hex(),
                })? != source.evidence_id
            {
                return Err(VaultcError::VerificationFailed(
                    "evidence source identity is invalid".into(),
                ));
            }
            validate_attribution(
                &source.attribution,
                Some((source.source_file_id, source.document_id)),
            )
        }
        SourceRecord::BuildInput(source) => {
            if source.identity.is_empty() || source.identity.chars().any(char::is_control) {
                return Err(VaultcError::VerificationFailed(
                    "build input identity is invalid".into(),
                ));
            }
            Ok(())
        }
    }
}

fn validate_operation_record(operation: &OperationRecord) -> Result<()> {
    let correct_variant = match operation {
        OperationRecord::Copy { operation } => matches!(operation, OutputOperation::Copy { .. }),
        OperationRecord::RewriteMarkdown { operation } => {
            matches!(operation, OutputOperation::RewriteMarkdown { .. })
        }
        OperationRecord::RewriteCanvas { operation } => {
            matches!(operation, OutputOperation::RewriteCanvas { .. })
        }
        OperationRecord::Deduplicate {
            operation,
            member_count,
        } => *member_count >= 2 && !operation.destination().is_empty(),
        OperationRecord::Generate {
            proposal_id,
            destination,
            evidence_ids,
            ..
        } => !proposal_id.is_empty() && !destination.is_empty() && !evidence_ids.is_empty(),
        OperationRecord::SerializeAudit { role, .. } => matches!(
            role,
            OutputRole::AuditPlan
                | OutputRole::AuditConflicts
                | OutputRole::AuditDiagnostics
                | OutputRole::AuditTranscript
                | OutputRole::Provenance
                | OutputRole::Manifest
                | OutputRole::Checksums
        ),
        OperationRecord::Package {
            outer_raw_sha256,
            profile,
            members,
            ..
        } => {
            ContentHash::parse_hex(outer_raw_sha256).is_ok()
                && profile == crate::pack::PACK_PROFILE
                && !members.is_empty()
                && members.windows(2).all(|pair| pair[0].path < pair[1].path)
        }
    };
    if !correct_variant {
        return Err(VaultcError::VerificationFailed(
            "provenance operation has an invalid tagged payload".into(),
        ));
    }
    Ok(())
}

fn operation_accepts_relation(
    operation: &OperationRecord,
    target: &ProvenanceRecordKind,
    relation: EdgeRelation,
) -> bool {
    matches!(
        (operation, relation, target),
        (
            OperationRecord::Copy { .. },
            EdgeRelation::DerivedFrom,
            ProvenanceRecordKind::Source(_)
        ) | (
            OperationRecord::RewriteMarkdown { .. } | OperationRecord::RewriteCanvas { .. },
            EdgeRelation::RewrittenFrom,
            ProvenanceRecordKind::Source(_),
        ) | (
            OperationRecord::Deduplicate { .. },
            EdgeRelation::Deduplicates,
            ProvenanceRecordKind::Source(_),
        ) | (
            OperationRecord::Generate { .. },
            EdgeRelation::DerivedFrom,
            ProvenanceRecordKind::Proposal(_),
        ) | (
            OperationRecord::Generate { .. },
            EdgeRelation::ApprovedBy,
            ProvenanceRecordKind::Approval(_),
        ) | (
            OperationRecord::Copy { .. }
                | OperationRecord::RewriteMarkdown { .. }
                | OperationRecord::RewriteCanvas { .. }
                | OperationRecord::Deduplicate { .. },
            EdgeRelation::DecidedBy,
            ProvenanceRecordKind::Decision(_),
        ) | (
            OperationRecord::SerializeAudit { .. },
            EdgeRelation::DerivedFrom,
            ProvenanceRecordKind::Source(_)
                | ProvenanceRecordKind::Operation(_)
                | ProvenanceRecordKind::Decision(_)
                | ProvenanceRecordKind::Proposal(_)
                | ProvenanceRecordKind::Approval(_)
                | ProvenanceRecordKind::Output(_),
        ) | (
            OperationRecord::Package { .. },
            EdgeRelation::PackagedAs,
            ProvenanceRecordKind::Source(SourceRecord::BuildInput(BuildInputSourceRecord {
                input_kind: BuildInputKind::InnerArtifact,
                ..
            })),
        )
    )
}

fn validate_edge_matrix(
    edge: &EdgeRecord,
    from: &ProvenanceRecordKind,
    to: &ProvenanceRecordKind,
) -> Result<()> {
    let allowed = match (from, edge.relation, to) {
        (
            ProvenanceRecordKind::Output(_),
            EdgeRelation::DerivedFrom,
            ProvenanceRecordKind::Operation(_),
        )
        | (
            ProvenanceRecordKind::Proposal(_),
            EdgeRelation::SupportedBy,
            ProvenanceRecordKind::Source(SourceRecord::Evidence(_)),
        ) => true,
        (ProvenanceRecordKind::Operation(operation), relation, target) => {
            operation_accepts_relation(operation, target, relation)
        }
        _ => false,
    };
    let ordered = matches!(edge.relation, EdgeRelation::SupportedBy);
    if !allowed
        || ordered != matches!(edge.position, EdgePosition::Ordered { .. })
        || (!ordered && edge.position != EdgePosition::Unordered)
    {
        return Err(VaultcError::VerificationFailed(format!(
            "provenance edge {} has an invalid relation/type/position",
            edge.from
        )));
    }
    Ok(())
}

fn validate_output_record(output: &OutputRecord, mode: GraphStorageMode) -> Result<()> {
    match (&output.subject, output.storage, output.role, mode) {
        (
            ProvenanceSubject::ArtifactPath { path },
            OutputStorage::Stored,
            role,
            GraphStorageMode::Stored | GraphStorageMode::Logical,
        ) if !ENVELOPE_PATHS.contains(&path.as_str()) && output_role(path) == role => Ok(()),
        (
            ProvenanceSubject::ArtifactPath { path },
            OutputStorage::VirtualAuditEnvelope,
            role,
            GraphStorageMode::Logical,
        ) if ENVELOPE_PATHS.contains(&path.as_str()) && output_role(path) == role => Ok(()),
        (
            ProvenanceSubject::Package,
            OutputStorage::VirtualPackage,
            OutputRole::Package,
            GraphStorageMode::Logical,
        ) => Ok(()),
        _ => Err(VaultcError::VerificationFailed(
            "provenance output has an invalid subject/role/storage combination".into(),
        )),
    }
}

fn operation_matches_output(operation: &OperationRecord, output: &OutputRecord) -> bool {
    match (operation, &output.subject) {
        (
            OperationRecord::Copy { operation }
            | OperationRecord::RewriteMarkdown { operation }
            | OperationRecord::RewriteCanvas { operation }
            | OperationRecord::Deduplicate { operation, .. },
            ProvenanceSubject::ArtifactPath { path },
        ) => {
            output.role == OutputRole::Content
                && output.storage == OutputStorage::Stored
                && path == operation.destination()
                && output.content_hash == expected_operation_hash(operation)
        }
        (
            OperationRecord::Generate {
                destination,
                expected_output_hash,
                ..
            },
            ProvenanceSubject::ArtifactPath { path },
        ) => {
            output.role == OutputRole::Content
                && output.storage == OutputStorage::Stored
                && path == destination
                && output.content_hash == *expected_output_hash
        }
        (
            OperationRecord::SerializeAudit {
                role, commitment, ..
            },
            ProvenanceSubject::ArtifactPath { .. },
        ) => {
            output.role == *role
                && output.storage != OutputStorage::VirtualPackage
                // The provenance operation commits the domain-separated stored
                // graph hash; every other audit serialization commits its own
                // exact emitted bytes.
                && (*role == OutputRole::Provenance || output.content_hash == *commitment)
        }
        (OperationRecord::Package { byte_len, .. }, ProvenanceSubject::Package) => {
            output.role == OutputRole::Package
                && output.storage == OutputStorage::VirtualPackage
                && output.byte_len == *byte_len
        }
        _ => false,
    }
}

fn validate_operation_cardinality(
    operation: &OperationRecord,
    outgoing: &[&EdgeRecord],
    nodes: &BTreeMap<RecordId, &ProvenanceRecordKind>,
) -> Result<()> {
    let count = |relation| {
        outgoing
            .iter()
            .filter(|edge| edge.relation == relation)
            .count()
    };
    let valid = match operation {
        OperationRecord::Copy { .. } => count(EdgeRelation::DerivedFrom) == 1,
        OperationRecord::RewriteMarkdown { .. } | OperationRecord::RewriteCanvas { .. } => {
            count(EdgeRelation::RewrittenFrom) == 1
        }
        OperationRecord::Deduplicate { member_count, .. } => {
            u64::try_from(count(EdgeRelation::Deduplicates)) == Ok(*member_count)
        }
        OperationRecord::Generate {
            operation_id,
            plan_id,
            proposal_id,
            proposal_content_hash,
            destination,
            body_hash,
            expected_output_hash,
            evidence_ids,
            ..
        } => {
            let proposals: Vec<_> = outgoing
                .iter()
                .filter(|edge| edge.relation == EdgeRelation::DerivedFrom)
                .filter_map(|edge| match nodes[&edge.to] {
                    ProvenanceRecordKind::Proposal(proposal) => Some(proposal),
                    _ => None,
                })
                .collect();
            let approvals: Vec<_> = outgoing
                .iter()
                .filter(|edge| edge.relation == EdgeRelation::ApprovedBy)
                .filter_map(|edge| match nodes[&edge.to] {
                    ProvenanceRecordKind::Approval(approval) => Some(approval),
                    _ => None,
                })
                .collect();
            proposals.len() == 1
                && approvals.len() == 1
                && proposals[0].proposal_id == *proposal_id
                && proposals[0].plan_id == *plan_id
                && proposals[0].content_hash == *proposal_content_hash
                && proposals[0].proposal_kind
                    == vaultc_protocol::ProposalKindName::CreateGeneratedNote
                && proposals[0].valid
                && proposals[0].evidence_ids == *evidence_ids
                && approvals[0].decision.plan_id == plan_id.to_string()
                && approvals[0].decision.proposal_id == *proposal_id
                && approvals[0].decision.proposal_content_hash == *proposal_content_hash
                && approvals[0].decision.approved
                && matches!(
                    &approvals[0].materialization,
                    ProposalMaterialization::GeneratedNote {
                        destination: approved_destination,
                        body_hash: approved_body_hash,
                        expected_output_hash: approved_output_hash,
                        evidence_ids: approved_evidence_ids,
                        operation_id: approved_operation_id,
                    } if approved_destination == destination
                        && approved_body_hash == body_hash
                        && approved_output_hash == expected_output_hash
                        && approved_evidence_ids == evidence_ids
                        && approved_operation_id == operation_id
                )
        }
        OperationRecord::SerializeAudit { .. } => count(EdgeRelation::DerivedFrom) >= 1,
        OperationRecord::Package { .. } => count(EdgeRelation::PackagedAs) == 1,
    };
    if !valid {
        return Err(VaultcError::VerificationFailed(
            "provenance operation edge cardinality is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_proposal_cardinality(
    proposal: &ProposalRecord,
    outgoing: &[&EdgeRecord],
    nodes: &BTreeMap<RecordId, &ProvenanceRecordKind>,
) -> Result<()> {
    let mut evidence = outgoing
        .iter()
        .filter(|edge| edge.relation == EdgeRelation::SupportedBy)
        .map(|edge| {
            let EdgePosition::Ordered { index } = edge.position else {
                return Err(VaultcError::VerificationFailed(
                    "proposal evidence edge is unordered".into(),
                ));
            };
            let ProvenanceRecordKind::Source(SourceRecord::Evidence(source)) = nodes[&edge.to]
            else {
                return Err(VaultcError::VerificationFailed(
                    "proposal evidence edge targets a non-evidence source".into(),
                ));
            };
            Ok((index, source.evidence_id))
        })
        .collect::<Result<Vec<_>>>()?;
    evidence.sort_by_key(|(index, _)| *index);
    if evidence.len() != proposal.evidence_ids.len()
        || evidence.iter().enumerate().any(|(index, (position, id))| {
            usize::try_from(*position) != Ok(index) || proposal.evidence_ids[index] != *id
        })
    {
        return Err(VaultcError::VerificationFailed(
            "proposal ordered evidence closure is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_graph_topology(
    nodes: &BTreeMap<RecordId, &ProvenanceRecordKind>,
    edge_records: &[(RecordId, &EdgeRecord)],
    outgoing: &BTreeMap<RecordId, Vec<&EdgeRecord>>,
    output_roots: Vec<RecordId>,
) -> Result<()> {
    let non_edge_ids: BTreeSet<_> = nodes
        .iter()
        .filter_map(|(id, kind)| (!matches!(kind, ProvenanceRecordKind::Edge(_))).then_some(*id))
        .collect();
    let mut indegree: BTreeMap<_, usize> = non_edge_ids.iter().copied().map(|id| (id, 0)).collect();
    for (_, edge) in edge_records {
        *indegree.get_mut(&edge.to).ok_or_else(|| {
            VaultcError::VerificationFailed("edge target missing from DAG index".into())
        })? += 1;
    }
    let mut ready: Vec<_> = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut visited_count = 0_usize;
    while let Some(id) = ready.pop() {
        visited_count += 1;
        if let Some(edges) = outgoing.get(&id) {
            for edge in edges {
                let degree = indegree.get_mut(&edge.to).ok_or_else(|| {
                    VaultcError::VerificationFailed("edge target missing from DAG index".into())
                })?;
                *degree -= 1;
                if *degree == 0 {
                    ready.push(edge.to);
                }
            }
        }
    }
    if visited_count != non_edge_ids.len() {
        return Err(VaultcError::VerificationFailed(
            "provenance derivation graph contains a cycle".into(),
        ));
    }

    let mut reachable = BTreeSet::new();
    let mut stack = output_roots;
    while let Some(id) = stack.pop() {
        if !reachable.insert(id) {
            continue;
        }
        if let Some(edges) = outgoing.get(&id) {
            for edge in edges {
                stack.push(edge.to);
            }
        }
    }
    if reachable != non_edge_ids
        || edge_records
            .iter()
            .any(|(_, edge)| !reachable.contains(&edge.from))
    {
        return Err(VaultcError::VerificationFailed(
            "provenance graph contains records unreachable from every output".into(),
        ));
    }
    Ok(())
}

fn validate_output_producer(
    output: &OutputRecord,
    edges: &[&EdgeRecord],
    nodes: &BTreeMap<RecordId, &ProvenanceRecordKind>,
) -> Result<()> {
    let producers: Vec<_> = edges
        .iter()
        .filter(|edge| edge.relation == EdgeRelation::DerivedFrom)
        .collect();
    if producers.len() != 1 || producers[0].to != output.producing_operation {
        return Err(VaultcError::VerificationFailed(
            "provenance output does not have exactly its sealed producer".into(),
        ));
    }
    let Some(ProvenanceRecordKind::Operation(operation)) =
        nodes.get(&output.producing_operation).copied()
    else {
        return Err(VaultcError::VerificationFailed(
            "provenance output producer is not an operation".into(),
        ));
    };
    if !operation_matches_output(operation, output) {
        return Err(VaultcError::VerificationFailed(
            "provenance output disagrees with its producing operation".into(),
        ));
    }
    Ok(())
}

fn validate_graph(records: &[ProvenanceRecord], mode: GraphStorageMode) -> Result<()> {
    if records.is_empty() {
        return Err(VaultcError::VerificationFailed(
            "provenance graph is empty".into(),
        ));
    }
    let mut nodes = BTreeMap::new();
    let mut edge_records = Vec::new();
    let mut previous: Option<&ProvenanceRecord> = None;
    for record in records {
        record.validate_identity()?;
        if previous.is_some_and(|previous| record_cmp(previous, record) != std::cmp::Ordering::Less)
        {
            return Err(VaultcError::VerificationFailed(
                "provenance records are not in strict canonical order".into(),
            ));
        }
        previous = Some(record);
        if nodes.insert(record.record_id, &record.kind).is_some() {
            return Err(VaultcError::VerificationFailed(format!(
                "duplicate provenance RecordId {}",
                record.record_id
            )));
        }
        match &record.kind {
            ProvenanceRecordKind::Source(source) => validate_source_record(source)?,
            ProvenanceRecordKind::Operation(operation) => {
                validate_operation_record(operation)?;
            }
            ProvenanceRecordKind::Output(output) => validate_output_record(output, mode)?,
            ProvenanceRecordKind::Edge(edge) => edge_records.push((record.record_id, edge)),
            ProvenanceRecordKind::Decision(_)
            | ProvenanceRecordKind::Proposal(_)
            | ProvenanceRecordKind::Approval(_) => {}
        }
    }

    let mut outgoing: BTreeMap<RecordId, Vec<&EdgeRecord>> = BTreeMap::new();
    for (_, edge) in &edge_records {
        if edge.from == edge.to {
            return Err(VaultcError::VerificationFailed(
                "provenance graph contains a self-edge".into(),
            ));
        }
        let from = nodes.get(&edge.from).ok_or_else(|| {
            VaultcError::VerificationFailed(format!(
                "provenance edge has dangling from endpoint {}",
                edge.from
            ))
        })?;
        let to = nodes.get(&edge.to).ok_or_else(|| {
            VaultcError::VerificationFailed(format!(
                "provenance edge has dangling to endpoint {}",
                edge.to
            ))
        })?;
        if matches!(from, ProvenanceRecordKind::Edge(_))
            || matches!(to, ProvenanceRecordKind::Edge(_))
        {
            return Err(VaultcError::VerificationFailed(
                "provenance edges cannot have edge-record endpoints".into(),
            ));
        }
        validate_edge_matrix(edge, from, to)?;
        outgoing.entry(edge.from).or_default().push(edge);
    }

    let mut output_roots = Vec::new();
    for (record_id, kind) in &nodes {
        let edges = outgoing.get(record_id).map_or(&[][..], Vec::as_slice);
        match kind {
            ProvenanceRecordKind::Output(output) => {
                output_roots.push(*record_id);
                validate_output_producer(output, edges, &nodes)?;
            }
            ProvenanceRecordKind::Operation(operation) => {
                validate_operation_cardinality(operation, edges, &nodes)?;
            }
            ProvenanceRecordKind::Proposal(proposal) => {
                validate_proposal_cardinality(proposal, edges, &nodes)?;
            }
            ProvenanceRecordKind::Source(_)
            | ProvenanceRecordKind::Decision(_)
            | ProvenanceRecordKind::Approval(_) => {
                if !edges.is_empty() {
                    return Err(VaultcError::VerificationFailed(
                        "leaf provenance record unexpectedly has outgoing edges".into(),
                    ));
                }
            }
            ProvenanceRecordKind::Edge(_) => {}
        }
    }

    validate_graph_topology(&nodes, &edge_records, &outgoing, output_roots)
}

pub fn validate_stored_graph(records: &[ProvenanceRecord]) -> Result<()> {
    validate_graph(records, GraphStorageMode::Stored)
}

fn add_virtual_output(
    builder: &mut GraphBuilder,
    path: &str,
    bytes: &[u8],
    operation: OperationRecord,
) -> Result<(RecordId, RecordId)> {
    let operation_id = builder.insert(ProvenanceRecordKind::Operation(operation))?;
    let output_id = builder.insert(ProvenanceRecordKind::Output(OutputRecord {
        subject: ProvenanceSubject::ArtifactPath { path: path.into() },
        content_hash: ContentHash::from_bytes(bytes),
        byte_len: u64::try_from(bytes.len()).map_err(|_| {
            VaultcError::ResourceLimit(format!("audit envelope `{path}` length overflow"))
        })?,
        role: output_role(path),
        storage: OutputStorage::VirtualAuditEnvelope,
        producing_operation: operation_id,
    }))?;
    builder.edge(
        EdgeRelation::DerivedFrom,
        output_id,
        operation_id,
        EdgePosition::Unordered,
    )?;
    Ok((output_id, operation_id))
}

#[allow(
    clippy::too_many_lines,
    reason = "the three ordered audit-envelope layers form one non-circular construction invariant"
)]
pub(crate) fn build_logical_graph(
    root: &Path,
    manifest: &crate::compile::ArtifactManifest,
    stored: &[ProvenanceRecord],
) -> Result<Vec<ProvenanceRecord>> {
    validate_stored_graph(stored)?;
    let provenance_path = root.join(".vaultc/provenance.jsonl");
    let manifest_path = root.join(".vaultc/manifest.json");
    let checksums_path = root.join(".vaultc/checksums.txt");
    let provenance_bytes =
        fs::read(&provenance_path).map_err(|error| VaultcError::io(&provenance_path, error))?;
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|error| VaultcError::io(&manifest_path, error))?;
    let checksums_bytes =
        fs::read(&checksums_path).map_err(|error| VaultcError::io(&checksums_path, error))?;
    if stored_graph_hash(&provenance_bytes) != manifest.provenance_graph_hash {
        return Err(VaultcError::VerificationFailed(
            "virtual envelope received a stale stored graph hash".into(),
        ));
    }

    let mut builder = GraphBuilder::default();
    let mut stored_non_edges = Vec::new();
    let mut stored_outputs = Vec::new();
    for record in stored {
        let id = builder.insert(record.kind.clone())?;
        if id != record.record_id {
            return Err(VaultcError::VerificationFailed(
                "stored record changed while constructing the audit envelope".into(),
            ));
        }
        match &record.kind {
            ProvenanceRecordKind::Edge(_) => {}
            ProvenanceRecordKind::Output(_) => {
                stored_non_edges.push(id);
                stored_outputs.push(id);
            }
            _ => stored_non_edges.push(id),
        }
    }
    let plan_id = manifest.plan_id.parse::<PlanId>()?;
    let (provenance_output, provenance_operation) = add_virtual_output(
        &mut builder,
        ".vaultc/provenance.jsonl",
        &provenance_bytes,
        OperationRecord::SerializeAudit {
            role: OutputRole::Provenance,
            plan_id,
            commitment: manifest.provenance_graph_hash,
            compiler_version: manifest.compiler_version.clone(),
            policy_hash: manifest.policy_hash,
        },
    )?;
    for prerequisite in &stored_non_edges {
        builder.edge(
            EdgeRelation::DerivedFrom,
            provenance_operation,
            *prerequisite,
            EdgePosition::Unordered,
        )?;
    }

    let (manifest_output, manifest_operation) = add_virtual_output(
        &mut builder,
        ".vaultc/manifest.json",
        &manifest_bytes,
        OperationRecord::SerializeAudit {
            role: OutputRole::Manifest,
            plan_id,
            commitment: ContentHash::from_bytes(&manifest_bytes),
            compiler_version: manifest.compiler_version.clone(),
            policy_hash: manifest.policy_hash,
        },
    )?;
    for prerequisite in stored_outputs
        .iter()
        .copied()
        .chain(std::iter::once(provenance_output))
    {
        builder.edge(
            EdgeRelation::DerivedFrom,
            manifest_operation,
            prerequisite,
            EdgePosition::Unordered,
        )?;
    }

    let (checksums_output, checksums_operation) = add_virtual_output(
        &mut builder,
        ".vaultc/checksums.txt",
        &checksums_bytes,
        OperationRecord::SerializeAudit {
            role: OutputRole::Checksums,
            plan_id,
            commitment: ContentHash::from_bytes(&checksums_bytes),
            compiler_version: manifest.compiler_version.clone(),
            policy_hash: manifest.policy_hash,
        },
    )?;
    for prerequisite in stored_outputs
        .iter()
        .copied()
        .chain([provenance_output, manifest_output])
    {
        builder.edge(
            EdgeRelation::DerivedFrom,
            checksums_operation,
            prerequisite,
            EdgePosition::Unordered,
        )?;
    }
    let _ = checksums_output;
    let records = builder.finish();
    validate_graph(&records, GraphStorageMode::Logical)?;
    Ok(records)
}

pub(crate) fn validate_audit_envelope(
    root: &Path,
    manifest: &crate::compile::ArtifactManifest,
    stored: &[ProvenanceRecord],
) -> Result<()> {
    build_logical_graph(root, manifest, stored).map(|_| ())
}

fn induced_explanation(
    records: &[ProvenanceRecord],
    subject: &ProvenanceSubject,
) -> Result<Vec<ProvenanceRecord>> {
    let mut root = None;
    let mut outgoing: BTreeMap<RecordId, Vec<(RecordId, RecordId)>> = BTreeMap::new();
    for record in records {
        match &record.kind {
            ProvenanceRecordKind::Output(output) if &output.subject == subject => {
                if root.replace(record.record_id).is_some() {
                    return Err(VaultcError::VerificationFailed(
                        "provenance subject has more than one output root".into(),
                    ));
                }
            }
            ProvenanceRecordKind::Edge(edge) => {
                outgoing
                    .entry(edge.from)
                    .or_default()
                    .push((edge.to, record.record_id));
            }
            _ => {}
        }
    }
    let root = root.ok_or_else(|| {
        VaultcError::VerificationFailed(match subject {
            ProvenanceSubject::ArtifactPath { path } => {
                format!("no provenance found for `{path}`")
            }
            ProvenanceSubject::Package => "no package provenance found".into(),
        })
    })?;
    let mut included = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(record_id) = stack.pop() {
        if !included.insert(record_id) {
            continue;
        }
        if let Some(edges) = outgoing.get(&record_id) {
            for (target, edge_id) in edges {
                included.insert(*edge_id);
                stack.push(*target);
            }
        }
    }
    let explanation: Vec<_> = records
        .iter()
        .filter(|record| included.contains(&record.record_id))
        .cloned()
        .collect();
    if explanation.is_empty() {
        return Err(VaultcError::VerificationFailed(
            "provenance explanation closure is empty".into(),
        ));
    }
    Ok(explanation)
}

fn read_directory_graph(root: &Path) -> Result<Vec<ProvenanceRecord>> {
    crate::verify::verify_directory(root)?;
    let manifest_path = root.join(".vaultc/manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|error| VaultcError::io(&manifest_path, error))?;
    let manifest: crate::compile::ArtifactManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| {
            VaultcError::VerificationFailed(format!("manifest is malformed: {error}"))
        })?;
    let provenance_path = root.join(".vaultc/provenance.jsonl");
    let provenance =
        fs::read(&provenance_path).map_err(|error| VaultcError::io(&provenance_path, error))?;
    let stored = decode_jsonl(&provenance, &provenance_path)?;
    build_logical_graph(root, &manifest, &stored)
}

#[cfg(feature = "archives")]
fn package_graph(pack: &Path, extracted: &Path) -> Result<Vec<ProvenanceRecord>> {
    crate::verify::verify_directory(extracted)?;
    let observation = crate::pack::verify_canonical_pack(pack, extracted)?;
    let manifest_path = extracted.join(".vaultc/manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|error| VaultcError::io(&manifest_path, error))?;
    let manifest: crate::compile::ArtifactManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| {
            VaultcError::VerificationFailed(format!("manifest is malformed: {error}"))
        })?;
    let inventory = crate::compile::inventory(extracted, &[])?;
    let members = inventory
        .into_iter()
        .map(|file| PackageMemberCommitment {
            path: file.path,
            byte_len: file.byte_len,
            sha256: file.sha256,
        })
        .collect::<Vec<_>>();

    let mut builder = GraphBuilder::default();
    let inner = add_build_input(
        &mut builder,
        BuildInputKind::InnerArtifact,
        manifest.artifact_id.hex(),
        manifest.artifact_id,
    )?;
    let operation = builder.insert(ProvenanceRecordKind::Operation(OperationRecord::Package {
        outer_raw_sha256: observation.raw_sha256,
        byte_len: observation.byte_len,
        inner_artifact_id: manifest.artifact_id,
        profile: crate::pack::PACK_PROFILE.into(),
        members,
    }))?;
    let output = builder.insert(ProvenanceRecordKind::Output(OutputRecord {
        subject: ProvenanceSubject::Package,
        content_hash: observation.content_hash,
        byte_len: observation.byte_len,
        role: OutputRole::Package,
        storage: OutputStorage::VirtualPackage,
        producing_operation: operation,
    }))?;
    builder.edge(
        EdgeRelation::DerivedFrom,
        output,
        operation,
        EdgePosition::Unordered,
    )?;
    builder.edge(
        EdgeRelation::PackagedAs,
        operation,
        inner,
        EdgePosition::Unordered,
    )?;
    let records = builder.finish();
    validate_graph(&records, GraphStorageMode::Logical)?;
    Ok(records)
}

fn artifact_is_pack(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("vaultpack"))
}

fn complete_explanation(
    artifact: &Path,
    subject: &ProvenanceSubject,
) -> Result<Vec<ProvenanceRecord>> {
    match (artifact_is_pack(artifact), subject) {
        (false, ProvenanceSubject::Package) => Err(VaultcError::VerificationFailed(
            "a package provenance subject requires a .vaultpack artifact".into(),
        )),
        (false, ProvenanceSubject::ArtifactPath { path }) => {
            crate::snapshot::validate_output_logical_path(
                path,
                &crate::config::CompilerPolicy::default(),
            )?;
            induced_explanation(&read_directory_graph(artifact)?, subject)
        }
        (true, _) => {
            #[cfg(feature = "archives")]
            {
                let temporary =
                    tempfile::tempdir().map_err(|error| VaultcError::io(artifact, error))?;
                crate::pack::extract_pack_safely(artifact, temporary.path())?;
                let records = match subject {
                    ProvenanceSubject::ArtifactPath { path } => {
                        crate::snapshot::validate_output_logical_path(
                            path,
                            &crate::config::CompilerPolicy::default(),
                        )?;
                        read_directory_graph(temporary.path())?
                    }
                    ProvenanceSubject::Package => package_graph(artifact, temporary.path())?,
                };
                induced_explanation(&records, subject)
            }
            #[cfg(not(feature = "archives"))]
            {
                Err(VaultcError::UnsupportedSource(artifact.to_path_buf()))
            }
        }
    }
}

fn valid_cursor_syntax(cursor: &str) -> bool {
    cursor.strip_prefix("cursor_").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
}

fn canonical_page_size(
    graph_hash: ContentHash,
    subject: &ProvenanceSubject,
    record_bytes: usize,
    record_count: usize,
    next_cursor: Option<String>,
    complete: bool,
) -> Result<usize> {
    let empty = ProvenancePage {
        schema_version: PROVENANCE_SCHEMA_VERSION,
        graph_hash,
        subject: subject.clone(),
        records: Vec::new(),
        next_cursor,
        complete,
    };
    empty
        .canonical_json_len()?
        .checked_add(record_bytes)
        .and_then(|length| length.checked_add(record_count.saturating_sub(1)))
        .ok_or_else(|| VaultcError::ResourceLimit("provenance page length overflow".into()))
}

pub fn explain_page(artifact: &Path, query: &ProvenanceQuery) -> Result<ProvenancePage> {
    if !(1..=MAX_PAGE_RECORDS).contains(&query.limit)
        || !(1..=MAX_PAGE_BYTES).contains(&query.max_bytes)
    {
        return Err(VaultcError::InvalidConfig(
            "provenance page limits exceed the public hard bounds".into(),
        ));
    }
    let records = complete_explanation(artifact, &query.subject)?;
    let encoded = encode_jsonl(&records)?;
    let graph_hash = explanation_graph_hash(&encoded);
    let start = if let Some(cursor) = &query.cursor {
        if !valid_cursor_syntax(cursor) {
            return Err(VaultcError::VerificationFailed(
                "malformed provenance cursor".into(),
            ));
        }
        records
            .iter()
            .position(|record| {
                cursor_value(graph_hash, &query.subject, record)
                    .is_ok_and(|candidate| candidate == *cursor)
            })
            .map(|index| index + 1)
            .ok_or_else(|| {
                VaultcError::VerificationFailed(
                    "provenance cursor is stale or belongs to another subject".into(),
                )
            })?
    } else {
        0
    };

    let mut page = Vec::new();
    let mut page_record_bytes = 0_usize;
    let mut end = start;
    for record in records.iter().skip(start) {
        if page.len() == query.limit {
            break;
        }
        let record_bytes = to_canonical_json(record)?.len();
        let candidate_record_bytes = page_record_bytes
            .checked_add(record_bytes)
            .ok_or_else(|| VaultcError::ResourceLimit("provenance page length overflow".into()))?;
        let candidate_end = end + 1;
        let candidate_complete = candidate_end == records.len();
        let candidate_cursor = if candidate_complete {
            None
        } else {
            Some(cursor_value(graph_hash, &query.subject, record)?)
        };
        let candidate_size = canonical_page_size(
            graph_hash,
            &query.subject,
            candidate_record_bytes,
            page.len() + 1,
            candidate_cursor,
            candidate_complete,
        )?;
        if candidate_size > query.max_bytes {
            if page.is_empty() {
                return Err(VaultcError::ResourceLimit(
                    "one provenance record and its response envelope exceed the requested byte limit"
                        .into(),
                ));
            }
            break;
        }
        page_record_bytes = candidate_record_bytes;
        page.push(record.clone());
        end = candidate_end;
    }
    let complete = end == records.len();
    let next_cursor = if complete {
        None
    } else {
        page.last()
            .map(|record| cursor_value(graph_hash, &query.subject, record))
            .transpose()?
    };
    let result = ProvenancePage {
        schema_version: PROVENANCE_SCHEMA_VERSION,
        graph_hash,
        subject: query.subject.clone(),
        records: page,
        next_cursor,
        complete,
    };
    if result.canonical_json_len()? > query.max_bytes {
        return Err(VaultcError::ResourceLimit(
            "provenance response envelope exceeds the requested byte limit".into(),
        ));
    }
    Ok(result)
}

pub fn explain(artifact: &Path, requested_path: &str) -> Result<ProvenanceExplanation> {
    let query = ProvenanceQuery::artifact_path(requested_path)
        .with_limit(MAX_PAGE_RECORDS)?
        .with_max_bytes(MAX_PAGE_BYTES)?;
    let page = explain_page(artifact, &query)?;
    if !page.complete {
        return Err(VaultcError::ResourceLimit(
            "complete provenance explanation exceeds the public hard bounds".into(),
        ));
    }
    Ok(ProvenanceExplanation {
        schema_version: page.schema_version,
        graph_hash: page.graph_hash,
        subject: page.subject,
        records: page.records,
    })
}
