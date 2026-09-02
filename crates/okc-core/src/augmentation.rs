use std::collections::BTreeSet;
use std::sync::Arc;

use okc_protocol::{
    AugmentationLimits, AugmentationRequest, AugmentationResponse, DataBoundary,
    DocumentProjection, Envelope, MessageType, ObjectKind, ObjectRef, ProjectedBlock,
    ProposalKindName, ProviderCapabilities, TranscriptDirection, TranscriptRecord,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical::{canonical_hash, to_canonical_json};
use crate::config::CompilerPolicy;
use crate::error::{OkcError, Result};
use crate::identity::{ContentHash, DocumentId, PlanId};
use crate::plan::DraftPlan;
use crate::provider::{
    CancellationToken, KnowledgeAugmentor, ProposalValidation, ValidatedProposals,
    validate_capabilities, validate_proposals,
};

pub const AUGMENTATION_SCHEMA_VERSION: u32 = 2;
pub const AUGMENTATION_MAX_LINE_BYTES: usize = 64 * 1024 * 1024;
pub const AUGMENTATION_MAX_TOTAL_BYTES: usize = 1024 * 1024 * 1024;

const TRANSCRIPT_HASH_DOMAIN: &str = "okc:provider-transcript-payload:v2\0";
const REDACTED_BLOCK_TEXT: &str = "[redacted]";
const FIXED_RECORDS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentSelection {
    Explicit(Vec<DocumentId>),
    All,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RemoteProviderConsent {
    #[default]
    Denied,
    Granted,
}

/// Opaque proof that a transport may disclose one exact sealed projection to
/// the negotiated provider. It is intentionally not serializable: runtime
/// consent is authority for one live exchange, never a durable permission.
#[derive(Debug)]
pub struct AuthorizedAugmentationExchange {
    plan_id: PlanId,
    projection_hash: ContentHash,
    request: AugmentationRequest,
    capabilities: ProviderCapabilities,
}

impl AuthorizedAugmentationExchange {
    #[must_use]
    pub fn request(&self) -> &AugmentationRequest {
        &self.request
    }

    #[must_use]
    pub fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordedAugmentation {
    schema_version: u32,
    plan_id: PlanId,
    projection_hash: ContentHash,
    provider: okc_protocol::ProviderIdentity,
    transcript: Arc<Vec<TranscriptRecord>>,
    validations: Arc<Vec<ProposalValidation>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AugmentationRecord {
    Header {
        schema_version: u32,
        plan_id: PlanId,
        projection_hash: ContentHash,
        provider: okc_protocol::ProviderIdentity,
    },
    Transcript {
        record: TranscriptRecord,
    },
    Proposal {
        validation: Box<ProposalValidation>,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AugmentationRecordRef<'a> {
    Header {
        schema_version: u32,
        plan_id: PlanId,
        projection_hash: ContentHash,
        provider: &'a okc_protocol::ProviderIdentity,
    },
    Transcript {
        record: &'a TranscriptRecord,
    },
    Proposal {
        validation: &'a ProposalValidation,
    },
}

impl RecordedAugmentation {
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub fn plan_id(&self) -> PlanId {
        self.plan_id
    }

    #[must_use]
    pub fn projection_hash(&self) -> ContentHash {
        self.projection_hash
    }

    #[must_use]
    pub fn provider(&self) -> &okc_protocol::ProviderIdentity {
        &self.provider
    }

    #[must_use]
    pub fn transcript(&self) -> &[TranscriptRecord] {
        &self.transcript
    }

    #[must_use]
    pub fn validations(&self) -> &[ProposalValidation] {
        &self.validations
    }

    #[must_use]
    pub fn into_validated(self) -> ValidatedProposals {
        ValidatedProposals {
            validations: Arc::try_unwrap(self.validations)
                .unwrap_or_else(|validations| (*validations).clone()),
            transcript: Arc::try_unwrap(self.transcript)
                .unwrap_or_else(|transcript| (*transcript).clone()),
        }
    }

    pub fn to_canonical_jsonl(&self) -> Result<Vec<u8>> {
        validate_recording_shape(self)?;
        let total_bytes = validate_codec_bounds(self)?;
        let mut output = Vec::with_capacity(total_bytes);
        for record in self.record_refs() {
            let encoded = to_canonical_json(&record)?;
            output.extend(encoded);
            output.push(b'\n');
        }
        Ok(output)
    }

    pub fn from_canonical_jsonl(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(provider_error("augmentation JSONL is empty"));
        }
        if bytes.len() > AUGMENTATION_MAX_TOTAL_BYTES {
            return Err(provider_error(
                "augmentation JSONL exceeds the 1 GiB control-file limit",
            ));
        }
        if bytes.last() != Some(&b'\n') {
            return Err(provider_error(
                "augmentation JSONL must end with a final LF",
            ));
        }
        if bytes.contains(&b'\r') {
            return Err(provider_error("augmentation JSONL must not contain CRLF"));
        }

        let mut records = Vec::new();
        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            if line.len() > AUGMENTATION_MAX_LINE_BYTES {
                return Err(provider_error(
                    "augmentation JSONL record exceeds the 64 MiB line limit",
                ));
            }
            let payload = &line[..line.len() - 1];
            if payload.is_empty() {
                return Err(provider_error(
                    "augmentation JSONL must not contain blank records",
                ));
            }
            let record: AugmentationRecord = serde_json::from_slice(payload).map_err(|error| {
                provider_error(format!("augmentation JSONL record is malformed: {error}"))
            })?;
            let canonical = to_canonical_json(&record)?;
            if canonical != payload {
                return Err(provider_error(
                    "augmentation JSONL record is not canonical or contains unknown/duplicate fields",
                ));
            }
            records.push(record);
        }
        Self::from_records(records)
    }

    fn record_refs(&self) -> impl Iterator<Item = AugmentationRecordRef<'_>> {
        std::iter::once(AugmentationRecordRef::Header {
            schema_version: self.schema_version,
            plan_id: self.plan_id,
            projection_hash: self.projection_hash,
            provider: &self.provider,
        })
        .chain(
            self.transcript
                .iter()
                .map(|record| AugmentationRecordRef::Transcript { record }),
        )
        .chain(
            self.validations
                .iter()
                .map(|validation| AugmentationRecordRef::Proposal { validation }),
        )
    }

    fn from_records(records: Vec<AugmentationRecord>) -> Result<Self> {
        if records.len() < FIXED_RECORDS {
            return Err(provider_error(
                "augmentation JSONL requires one header and four transcript records",
            ));
        }
        let mut records = records.into_iter();
        let Some(AugmentationRecord::Header {
            schema_version,
            plan_id,
            projection_hash,
            provider,
        }) = records.next()
        else {
            return Err(provider_error(
                "augmentation header must be the first record",
            ));
        };
        let mut transcript = Vec::with_capacity(4);
        for _ in 0..4 {
            let Some(AugmentationRecord::Transcript { record }) = records.next() else {
                return Err(provider_error(
                    "augmentation header must be followed by exactly four transcript records",
                ));
            };
            transcript.push(record);
        }
        let mut validations = Vec::new();
        for record in records {
            let AugmentationRecord::Proposal { validation } = record else {
                return Err(provider_error(
                    "augmentation validation records must follow the transcript",
                ));
            };
            validations.push(*validation);
        }
        let recording = Self {
            schema_version,
            plan_id,
            projection_hash,
            provider,
            transcript: Arc::new(transcript),
            validations: Arc::new(validations),
        };
        validate_recording_shape(&recording)?;
        Ok(recording)
    }
}

pub fn build_augmentation_request(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    selection: &DocumentSelection,
) -> Result<AugmentationRequest> {
    plan.validate_integrity()?;
    if plan.policy.semantic_hash()? != policy.semantic_hash()? {
        return Err(OkcError::PlanStale(
            "augmentation compiler policy does not match the sealed plan".into(),
        ));
    }

    let selected = match selection {
        DocumentSelection::All => None,
        DocumentSelection::Explicit(document_ids) => {
            if document_ids.is_empty() {
                return Err(OkcError::InvalidConfig(
                    "explicit augmentation selection cannot be empty".into(),
                ));
            }
            let selected: BTreeSet<_> = document_ids.iter().copied().collect();
            if selected.len() != document_ids.len() {
                return Err(OkcError::InvalidConfig(
                    "explicit augmentation selection contains a duplicate document".into(),
                ));
            }
            for document_id in &selected {
                if !plan.workspace.documents.contains_key(document_id) {
                    return Err(OkcError::ProposalInvalid(format!(
                        "augmentation selection references unknown document `{document_id}`"
                    )));
                }
            }
            Some(selected)
        }
    };

    let documents: Vec<_> = plan
        .workspace
        .documents
        .values()
        .filter(|document| {
            selected
                .as_ref()
                .is_none_or(|ids| ids.contains(&document.document_id))
        })
        .map(|document| DocumentProjection {
            snapshot_id: document.source_file.snapshot_id.to_string(),
            document: ObjectRef {
                kind: ObjectKind::Document,
                id: document.document_id.to_string(),
                content_hash: document.body_hash.hex(),
            },
            logical_path: document.source_file.logical_path.clone(),
            title: document.title.clone(),
            selected_blocks: document
                .blocks
                .iter()
                .map(|block| ProjectedBlock {
                    block: ObjectRef {
                        kind: ObjectKind::Block,
                        id: block.block_id.to_string(),
                        content_hash: block.content_hash.hex(),
                    },
                    text: block.comparison_text.clone(),
                })
                .collect(),
        })
        .collect();
    if documents.is_empty() {
        return Err(OkcError::InvalidConfig(
            "augmentation selection resolves to no documents".into(),
        ));
    }

    Ok(AugmentationRequest {
        plan_id: plan.plan_id.to_string(),
        projection_hash: plan.projection_hash.hex(),
        allowed_proposal_kinds: vec![
            ProposalKindName::CreateGeneratedNote,
            ProposalKindName::ExplainConflict,
        ],
        documents,
        limits: AugmentationLimits {
            max_proposals: policy.augmentation.max_proposals,
            max_generated_bytes: policy.augmentation.max_generated_bytes,
        },
    })
}

pub fn augment(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    selection: &DocumentSelection,
    augmentor: &(impl KnowledgeAugmentor + ?Sized),
    cancellation: &CancellationToken,
    consent: RemoteProviderConsent,
) -> Result<RecordedAugmentation> {
    ensure_not_cancelled(cancellation)?;
    let request = build_augmentation_request(plan, policy, selection)?;
    ensure_not_cancelled(cancellation)?;
    let capabilities = augmentor.capabilities();
    let authorization =
        authorize_augmentation_exchange(plan, policy, &request, &capabilities, consent)?;
    ensure_not_cancelled(cancellation)?;
    let proposals = augmentor.propose(authorization.request(), cancellation)?;
    ensure_not_cancelled(cancellation)?;
    let response = AugmentationResponse { proposals };
    ensure_payload_within_limit(
        &response,
        authorization.capabilities().max_output_bytes,
        "output",
    )?;
    let recording = record_augmentation_exchange(plan, policy, authorization, &response)?;
    ensure_not_cancelled(cancellation)?;
    Ok(recording)
}

pub fn authorize_augmentation_exchange(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    request: &AugmentationRequest,
    capabilities: &ProviderCapabilities,
    consent: RemoteProviderConsent,
) -> Result<AuthorizedAugmentationExchange> {
    validate_unredacted_request(plan, policy, request)?;
    authorize_provider(capabilities, policy, consent)?;
    ensure_payload_within_limit(request, capabilities.max_input_bytes, "input")?;
    Ok(AuthorizedAugmentationExchange {
        plan_id: plan.plan_id,
        projection_hash: plan.projection_hash,
        request: request.clone(),
        capabilities: capabilities.clone(),
    })
}

pub fn record_augmentation_exchange(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    authorization: AuthorizedAugmentationExchange,
    response: &AugmentationResponse,
) -> Result<RecordedAugmentation> {
    if authorization.plan_id != plan.plan_id
        || authorization.projection_hash != plan.projection_hash
    {
        return Err(OkcError::ApprovalStale(
            "augmentation authorization belongs to another sealed plan".into(),
        ));
    }
    let request = authorization.request;
    let capabilities = authorization.capabilities;
    validate_unredacted_request(plan, policy, &request)?;
    authorize_replayed_provider(&capabilities, policy)?;
    ensure_payload_within_limit(&request, capabilities.max_input_bytes, "input")?;
    ensure_payload_within_limit(response, capabilities.max_output_bytes, "output")?;
    if response
        .proposals
        .iter()
        .any(|proposal| proposal.provider != capabilities.provider)
    {
        return Err(provider_error(
            "proposal identity does not match the negotiated provider",
        ));
    }

    let validated = validate_proposals(plan, response.proposals.clone(), policy)?;
    let transcript = transcript_for_exchange(&request, &capabilities, response)?;
    let recording = RecordedAugmentation {
        schema_version: AUGMENTATION_SCHEMA_VERSION,
        plan_id: plan.plan_id,
        projection_hash: plan.projection_hash,
        provider: capabilities.provider,
        transcript: Arc::new(transcript),
        validations: Arc::new(validated.validations),
    };
    validate_recording(plan, policy, &recording)?;
    validate_codec_bounds(&recording)?;
    Ok(recording)
}

pub fn replay_augmentation(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    recording: &RecordedAugmentation,
) -> Result<RecordedAugmentation> {
    validate_recording(plan, policy, recording)?;
    let response: AugmentationResponse =
        serde_json::from_value(recording.transcript[3].payload.clone()).map_err(|error| {
            provider_error(format!(
                "recorded augmentation response is malformed: {error}"
            ))
        })?;
    let revalidated = validate_proposals(plan, response.proposals, policy)?;
    if revalidated.validations != *recording.validations {
        return Err(OkcError::ApprovalStale(
            "recorded proposal validations do not match deterministic replay".into(),
        ));
    }
    Ok(recording.clone())
}

pub(crate) fn validate_transcript(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    transcript: &[TranscriptRecord],
    validations: &[ProposalValidation],
) -> Result<()> {
    if transcript.is_empty() {
        if validations.is_empty() {
            return Ok(());
        }
        return Err(OkcError::ApprovalStale(
            "proposal validations require a complete provider transcript".into(),
        ));
    }
    validate_transcript_exchange(plan, policy, transcript, validations).map(|_| ())
}

fn validate_recording(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    recording: &RecordedAugmentation,
) -> Result<()> {
    plan.validate_integrity()?;
    if plan.policy.semantic_hash()? != policy.semantic_hash()? {
        return Err(OkcError::PlanStale(
            "augmentation compiler policy does not match the sealed plan".into(),
        ));
    }
    validate_recording_shape(recording)?;
    if recording.plan_id != plan.plan_id || recording.projection_hash != plan.projection_hash {
        return Err(OkcError::ApprovalStale(
            "augmentation recording belongs to another sealed plan".into(),
        ));
    }
    let capabilities =
        validate_transcript_exchange(plan, policy, &recording.transcript, &recording.validations)?;
    if capabilities.provider != recording.provider {
        return Err(OkcError::ApprovalStale(
            "augmentation header provider does not match the transcript".into(),
        ));
    }
    if recording.validations.len() > policy.augmentation.max_proposals as usize {
        return Err(OkcError::ResourceLimit(
            "augmentation recording exceeds the sealed proposal limit".into(),
        ));
    }
    Ok(())
}

// Keep the serialized header/transcript/validation envelope checks in one
// fail-closed pass so public decoding cannot expose a partially bound record.
#[allow(clippy::too_many_lines)]
fn validate_recording_shape(recording: &RecordedAugmentation) -> Result<()> {
    if recording.schema_version != AUGMENTATION_SCHEMA_VERSION {
        return Err(provider_error(format!(
            "unsupported augmentation schema version {}",
            recording.schema_version
        )));
    }
    if recording.transcript.len() != 4 {
        return Err(provider_error(
            "augmentation recording must contain exactly four transcript records",
        ));
    }
    let records = &recording.transcript;
    let expected_types = [
        MessageType::CapabilitiesRequest,
        MessageType::CapabilitiesResponse,
        MessageType::AugmentationRequest,
        MessageType::AugmentationResponse,
    ];
    let expected_directions = [
        TranscriptDirection::Request,
        TranscriptDirection::Response,
        TranscriptDirection::Request,
        TranscriptDirection::Response,
    ];
    let expected_request_ids = [
        "capabilities-1",
        "capabilities-1",
        "augmentation-1",
        "augmentation-1",
    ];
    for (index, record) in records.iter().enumerate() {
        if record.sequence != index as u64
            || record.message_type != expected_types[index]
            || record.direction != expected_directions[index]
            || record.request_id != expected_request_ids[index]
            || invalid_metadata(&record.request_id)
            || ContentHash::parse_hex(&record.canonical_payload_hash).is_err()
        {
            return Err(provider_error(
                "augmentation transcript sequence, direction, type, request ID, or payload hash is invalid",
            ));
        }
    }
    if records[0].payload != serde_json::json!({}) {
        return Err(provider_error(
            "augmentation capabilities request must contain the empty object",
        ));
    }

    let capabilities: ProviderCapabilities =
        decode_exact_payload(&records[1].payload, "capabilities response")?;
    validate_capability_identity(&capabilities)?;
    let request: AugmentationRequest =
        decode_exact_payload(&records[2].payload, "augmentation request")?;
    let response: AugmentationResponse =
        decode_exact_payload(&records[3].payload, "augmentation response")?;
    if capabilities.provider != recording.provider
        || request.plan_id != recording.plan_id.to_string()
        || request.projection_hash != recording.projection_hash.hex()
    {
        return Err(provider_error(
            "augmentation header does not match the transcript provider, plan, or projection",
        ));
    }
    if request.documents.is_empty()
        || request.documents.iter().any(|document| {
            document
                .selected_blocks
                .iter()
                .any(|block| block.text != REDACTED_BLOCK_TEXT)
        })
    {
        return Err(provider_error(
            "augmentation request must contain a non-empty canonically redacted projection",
        ));
    }
    if response
        .proposals
        .iter()
        .any(|proposal| proposal.provider != recording.provider)
    {
        return Err(provider_error(
            "augmentation response proposal identity does not match the header provider",
        ));
    }
    for pair in recording.validations.windows(2) {
        let left = pair[0].proposal.proposal_id.as_bytes();
        let right = pair[1].proposal.proposal_id.as_bytes();
        if left >= right {
            return Err(provider_error(
                "augmentation validation records must be strictly proposal-ID sorted",
            ));
        }
    }
    let mut response_proposals = response.proposals;
    response_proposals.sort_by(|left, right| {
        left.proposal_id
            .as_bytes()
            .cmp(right.proposal_id.as_bytes())
    });
    let recorded_proposals: Vec<_> = recording
        .validations
        .iter()
        .map(|validation| validation.proposal.clone())
        .collect();
    if response_proposals != recorded_proposals {
        return Err(provider_error(
            "augmentation response does not match the validation records",
        ));
    }
    for index in [0, 1, 3] {
        let expected = canonical_hash(TRANSCRIPT_HASH_DOMAIN, &records[index].payload)?.hex();
        if records[index].canonical_payload_hash != expected {
            return Err(provider_error(format!(
                "augmentation transcript record {index} has a stale payload hash"
            )));
        }
    }
    Ok(())
}

fn decode_exact_payload<T>(payload: &Value, name: &str) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let decoded: T = serde_json::from_value(payload.clone())
        .map_err(|error| provider_error(format!("{name} schema is invalid: {error}")))?;
    if serde_json::to_value(&decoded)? != *payload {
        return Err(provider_error(format!(
            "{name} contains unknown or non-schema fields"
        )));
    }
    Ok(decoded)
}

fn validate_codec_bounds(recording: &RecordedAugmentation) -> Result<usize> {
    let mut total_bytes = 0usize;
    for record in recording.record_refs() {
        let record_bytes = to_canonical_json(&record)?
            .len()
            .checked_add(1)
            .ok_or_else(|| provider_error("augmentation JSONL record length overflow"))?;
        if record_bytes > AUGMENTATION_MAX_LINE_BYTES {
            return Err(provider_error(
                "augmentation JSONL record exceeds the 64 MiB line limit",
            ));
        }
        total_bytes = total_bytes
            .checked_add(record_bytes)
            .ok_or_else(|| provider_error("augmentation JSONL total length overflow"))?;
        if total_bytes > AUGMENTATION_MAX_TOTAL_BYTES {
            return Err(provider_error(
                "augmentation JSONL exceeds the 1 GiB control-file limit",
            ));
        }
    }
    Ok(total_bytes)
}

// Transcript validation is intentionally kept as one fail-closed protocol
// pass so ordering, payload hashes, negotiation, and proposal binding cannot
// drift into independently callable partial validators.
#[allow(clippy::too_many_lines)]
fn validate_transcript_exchange(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    records: &[TranscriptRecord],
    validations: &[ProposalValidation],
) -> Result<ProviderCapabilities> {
    let expected_types = [
        MessageType::CapabilitiesRequest,
        MessageType::CapabilitiesResponse,
        MessageType::AugmentationRequest,
        MessageType::AugmentationResponse,
    ];
    let expected_directions = [
        TranscriptDirection::Request,
        TranscriptDirection::Response,
        TranscriptDirection::Request,
        TranscriptDirection::Response,
    ];
    let expected_request_ids = [
        "capabilities-1",
        "capabilities-1",
        "augmentation-1",
        "augmentation-1",
    ];
    if records.len() != 4 {
        return Err(OkcError::ApprovalStale(
            "provider transcript must contain exactly four records".into(),
        ));
    }
    for (index, record) in records.iter().enumerate() {
        if record.sequence != index as u64
            || record.message_type != expected_types[index]
            || record.direction != expected_directions[index]
            || record.request_id != expected_request_ids[index]
            || invalid_metadata(&record.request_id)
        {
            return Err(OkcError::ApprovalStale(
                "provider transcript sequence, direction, type, or request ID is invalid".into(),
            ));
        }
    }
    if records[0].payload != serde_json::json!({}) {
        return Err(OkcError::ApprovalStale(
            "capabilities request payload must be the empty object".into(),
        ));
    }

    let capabilities: ProviderCapabilities = serde_json::from_value(records[1].payload.clone())
        .map_err(|error| {
            OkcError::ApprovalStale(format!(
                "provider transcript capabilities are malformed: {error}"
            ))
        })?;
    if serde_json::to_value(&capabilities)? != records[1].payload {
        return Err(OkcError::ApprovalStale(
            "provider transcript capabilities contain unknown or non-schema fields".into(),
        ));
    }
    authorize_replayed_provider(&capabilities, policy)?;
    let request = hydrate_redacted_request(plan, policy, &records[2].payload)?;
    let response: AugmentationResponse = serde_json::from_value(records[3].payload.clone())
        .map_err(|error| {
            OkcError::ApprovalStale(format!(
                "provider transcript response is malformed: {error}"
            ))
        })?;
    if serde_json::to_value(&response)? != records[3].payload {
        return Err(OkcError::ApprovalStale(
            "provider transcript response contains unknown or non-schema fields".into(),
        ));
    }

    let payloads = [
        records[0].payload.clone(),
        records[1].payload.clone(),
        serde_json::to_value(&request)?,
        records[3].payload.clone(),
    ];
    for (record, payload) in records.iter().zip(payloads) {
        let expected = canonical_hash(TRANSCRIPT_HASH_DOMAIN, &payload)?.hex();
        if record.canonical_payload_hash != expected {
            return Err(OkcError::ApprovalStale(format!(
                "provider transcript record {} has a stale payload hash",
                record.sequence
            )));
        }
    }

    ensure_payload_within_limit(&request, capabilities.max_input_bytes, "input")?;
    ensure_payload_within_limit(&response, capabilities.max_output_bytes, "output")?;
    if response
        .proposals
        .iter()
        .any(|proposal| proposal.provider != capabilities.provider)
    {
        return Err(OkcError::ApprovalStale(
            "provider transcript proposal identity does not match capabilities".into(),
        ));
    }
    let fresh_validations = validate_proposals(plan, response.proposals.clone(), policy)
        .map_err(|error| {
            OkcError::ApprovalStale(format!(
                "recorded proposals no longer pass deterministic validation: {error}"
            ))
        })?
        .validations;
    if fresh_validations != validations {
        return Err(OkcError::ApprovalStale(
            "recorded proposal validations do not match deterministic validation".into(),
        ));
    }

    let mut response_proposals = response.proposals;
    response_proposals.sort_by(|left, right| {
        left.proposal_id
            .as_bytes()
            .cmp(right.proposal_id.as_bytes())
    });
    let recorded: Vec<_> = validations
        .iter()
        .map(|validation| validation.proposal.clone())
        .collect();
    if response_proposals != recorded
        || recorded.iter().any(|proposal| {
            !request
                .allowed_proposal_kinds
                .contains(&proposal.kind.name())
        })
    {
        return Err(OkcError::ApprovalStale(
            "provider transcript response does not match recorded validations".into(),
        ));
    }
    Ok(capabilities)
}

fn validate_unredacted_request(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    request: &AugmentationRequest,
) -> Result<()> {
    let selection = selection_from_request(request)?;
    let expected = build_augmentation_request(plan, policy, &selection)?;
    if expected != *request {
        return Err(OkcError::ApprovalStale(
            "augmentation request does not match the sealed projection builder".into(),
        ));
    }
    Ok(())
}

fn hydrate_redacted_request(
    plan: &DraftPlan,
    policy: &CompilerPolicy,
    payload: &Value,
) -> Result<AugmentationRequest> {
    let stored: AugmentationRequest = serde_json::from_value(payload.clone()).map_err(|error| {
        OkcError::ApprovalStale(format!(
            "augmentation transcript request schema is invalid: {error}"
        ))
    })?;
    if serde_json::to_value(&stored)? != *payload {
        return Err(OkcError::ApprovalStale(
            "augmentation transcript request contains unknown or non-schema fields".into(),
        ));
    }
    if stored.documents.iter().any(|document| {
        document
            .selected_blocks
            .iter()
            .any(|block| block.text != REDACTED_BLOCK_TEXT)
    }) {
        return Err(OkcError::ApprovalStale(
            "augmentation transcript block text is not canonically redacted".into(),
        ));
    }
    let selection = selection_from_request(&stored)?;
    let expected = build_augmentation_request(plan, policy, &selection)?;
    let mut redacted = expected.clone();
    redact_projection_text(&mut redacted);
    if stored != redacted {
        return Err(OkcError::ApprovalStale(
            "augmentation transcript request is stale for the sealed projection".into(),
        ));
    }
    Ok(expected)
}

fn selection_from_request(request: &AugmentationRequest) -> Result<DocumentSelection> {
    if request.documents.is_empty() {
        return Err(OkcError::ApprovalStale(
            "augmentation request contains no document selection".into(),
        ));
    }
    let mut ids = Vec::with_capacity(request.documents.len());
    for projection in &request.documents {
        let document_id = projection.document.id.parse::<DocumentId>().map_err(|_| {
            OkcError::ApprovalStale("augmentation request contains a malformed document ID".into())
        })?;
        ids.push(document_id);
    }
    Ok(DocumentSelection::Explicit(ids))
}

fn transcript_for_exchange(
    request: &AugmentationRequest,
    capabilities: &ProviderCapabilities,
    response: &AugmentationResponse,
) -> Result<Vec<TranscriptRecord>> {
    let capabilities_request = Envelope::new(
        "capabilities-1",
        MessageType::CapabilitiesRequest,
        serde_json::json!({}),
    );
    let capabilities_response = Envelope::new(
        "capabilities-1",
        MessageType::CapabilitiesResponse,
        capabilities.clone(),
    );
    let augmentation_request = Envelope::new(
        "augmentation-1",
        MessageType::AugmentationRequest,
        request.clone(),
    );
    let augmentation_response = Envelope::new(
        "augmentation-1",
        MessageType::AugmentationResponse,
        response.clone(),
    );
    Ok(vec![
        transcript_record(0, &capabilities_request, TranscriptDirection::Request)?,
        transcript_record(1, &capabilities_response, TranscriptDirection::Response)?,
        transcript_record(2, &augmentation_request, TranscriptDirection::Request)?,
        transcript_record(3, &augmentation_response, TranscriptDirection::Response)?,
    ])
}

fn transcript_record<T: Serialize>(
    sequence: u64,
    envelope: &Envelope<T>,
    direction: TranscriptDirection,
) -> Result<TranscriptRecord> {
    let mut payload = serde_json::to_value(&envelope.payload)?;
    let canonical_payload_hash = canonical_hash(TRANSCRIPT_HASH_DOMAIN, &payload)?.hex();
    if envelope.message_type == MessageType::AugmentationRequest {
        let mut request: AugmentationRequest = serde_json::from_value(payload)?;
        redact_projection_text(&mut request);
        payload = serde_json::to_value(request)?;
    }
    Ok(TranscriptRecord {
        sequence,
        request_id: envelope.request_id.clone(),
        direction,
        message_type: envelope.message_type,
        canonical_payload_hash,
        payload,
    })
}

fn redact_projection_text(request: &mut AugmentationRequest) {
    for document in &mut request.documents {
        for block in &mut document.selected_blocks {
            block.text = REDACTED_BLOCK_TEXT.into();
        }
    }
}

fn authorize_provider(
    capabilities: &ProviderCapabilities,
    policy: &CompilerPolicy,
    consent: RemoteProviderConsent,
) -> Result<()> {
    validate_capabilities(capabilities, policy)?;
    validate_capability_identity(capabilities)?;
    if !capabilities.structured_output {
        return Err(provider_error(
            "knowledge augmentation requires structured provider output",
        ));
    }
    if matches!(capabilities.data_boundary, DataBoundary::Remote { .. })
        && consent != RemoteProviderConsent::Granted
    {
        return Err(provider_error(
            "remote provider disclosure requires explicit runtime consent",
        ));
    }
    Ok(())
}

fn authorize_replayed_provider(
    capabilities: &ProviderCapabilities,
    policy: &CompilerPolicy,
) -> Result<()> {
    validate_capabilities(capabilities, policy).map_err(|error| {
        OkcError::ApprovalStale(format!("recorded provider is not permitted: {error}"))
    })?;
    validate_capability_identity(capabilities).map_err(|error| {
        OkcError::ApprovalStale(format!("recorded provider identity is invalid: {error}"))
    })?;
    if !capabilities.structured_output {
        return Err(OkcError::ApprovalStale(
            "recorded provider did not negotiate structured output".into(),
        ));
    }
    Ok(())
}

fn ensure_payload_within_limit<T: Serialize>(
    payload: &T,
    limit: u64,
    direction: &str,
) -> Result<()> {
    let length = u64::try_from(to_canonical_json(payload)?.len())
        .map_err(|_| OkcError::ResourceLimit("augmentation payload byte length overflow".into()))?;
    if length > limit {
        return Err(OkcError::ResourceLimit(format!(
            "augmentation {direction} is {length} bytes, exceeding provider limit {limit}"
        )));
    }
    Ok(())
}

fn validate_capability_identity(capabilities: &ProviderCapabilities) -> Result<()> {
    for (field, value) in [
        ("provider", capabilities.provider.provider.as_str()),
        ("model", capabilities.provider.model.as_str()),
    ] {
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(provider_error(format!(
                "provider capability {field} must contain 1..=256 non-control UTF-8 bytes"
            )));
        }
    }
    if capabilities
        .provider
        .version
        .as_deref()
        .is_some_and(|value| {
            value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        })
    {
        return Err(provider_error(
            "provider capability version must contain 1..=256 non-control UTF-8 bytes",
        ));
    }
    if let DataBoundary::Remote { endpoint_label } = &capabilities.data_boundary
        && (endpoint_label.is_empty()
            || endpoint_label.len() > 1024
            || endpoint_label.chars().any(char::is_control))
    {
        return Err(provider_error(
            "remote provider endpoint label must contain 1..=1024 non-control UTF-8 bytes",
        ));
    }
    Ok(())
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        return Err(provider_error("provider operation was cancelled"));
    }
    Ok(())
}

fn invalid_metadata(value: &str) -> bool {
    value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control)
}

fn provider_error(message: impl Into<String>) -> OkcError {
    OkcError::Provider(message.into())
}
