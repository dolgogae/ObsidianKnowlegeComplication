mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vaultc::augmentation::{DocumentSelection, RecordedAugmentation, RemoteProviderConsent};
use vaultc::canonical::{canonical_hash, to_canonical_json};
use vaultc::identity::{ContentHash, DocumentId};
use vaultc::pack::create_pack;
use vaultc::provider::CancellationToken;
use vaultc::{
    ApprovalDecision, ApprovalLog, CompilerPolicy, DraftPlan, KnowledgeAugmentor, SourceSpec,
    VaultCompiler, VaultcError,
};
use vaultc_protocol::{
    AugmentationRequest, AugmentationResponse, DataBoundary, EvidenceRefWire, KnowledgeProposal,
    MessageType, PROPOSAL_SCHEMA_VERSION, PROTOCOL_VERSION, ProposalKind, ProviderCapabilities,
    ProviderIdentity, ProviderOperation, TranscriptDirection,
};

const TRANSCRIPT_HASH_DOMAIN: &str = "vaultc:provider-transcript-payload:v1\0";
const SELECTED_SENTINEL: &str = "SELECTED_DISCLOSURE_SENTINEL_7F3D1A";
const UNSELECTED_SENTINEL: &str = "UNSELECTED_MUST_NOT_DISCLOSE_SENTINEL_91C4E2";

struct FixtureContext {
    compiler: VaultCompiler,
    plan: DraftPlan,
    selected_id: DocumentId,
}

#[derive(Clone, Copy)]
enum ReplyMode {
    Empty,
    OneValid,
    TwoValidReverseOrder,
    OneInvalid,
    CancelAndReturn,
}

#[derive(Default)]
struct AugmentorObservations {
    capability_calls: AtomicUsize,
    proposal_calls: AtomicUsize,
    requests: Mutex<Vec<AugmentationRequest>>,
}

struct FixtureAugmentor {
    capabilities: ProviderCapabilities,
    reply: ReplyMode,
    observations: Arc<AugmentorObservations>,
}

impl FixtureAugmentor {
    fn new(boundary: DataBoundary, reply: ReplyMode) -> Self {
        Self {
            capabilities: capabilities(boundary),
            reply,
            observations: Arc::new(AugmentorObservations::default()),
        }
    }

    fn capability_calls(&self) -> usize {
        self.observations.capability_calls.load(Ordering::Acquire)
    }

    fn proposal_calls(&self) -> usize {
        self.observations.proposal_calls.load(Ordering::Acquire)
    }

    fn request(&self) -> AugmentationRequest {
        self.observations
            .requests
            .lock()
            .expect("fixture request lock")
            .last()
            .expect("provider received a request")
            .clone()
    }
}

impl KnowledgeAugmentor for FixtureAugmentor {
    fn capabilities(&self) -> ProviderCapabilities {
        self.observations
            .capability_calls
            .fetch_add(1, Ordering::AcqRel);
        self.capabilities.clone()
    }

    fn propose(
        &self,
        request: &AugmentationRequest,
        cancellation: &CancellationToken,
    ) -> vaultc::Result<Vec<KnowledgeProposal>> {
        self.observations
            .proposal_calls
            .fetch_add(1, Ordering::AcqRel);
        self.observations
            .requests
            .lock()
            .expect("fixture request lock")
            .push(request.clone());
        let proposals = match self.reply {
            ReplyMode::Empty | ReplyMode::CancelAndReturn => Vec::new(),
            ReplyMode::OneValid => vec![proposal(
                request,
                self.capabilities.provider.clone(),
                "proposal-replay-1",
                "replay-one.md",
            )],
            ReplyMode::TwoValidReverseOrder => vec![
                proposal(
                    request,
                    self.capabilities.provider.clone(),
                    "proposal-z",
                    "replay-z.md",
                ),
                proposal(
                    request,
                    self.capabilities.provider.clone(),
                    "proposal-a",
                    "replay-a.md",
                ),
            ],
            ReplyMode::OneInvalid => {
                let mut invalid = proposal(
                    request,
                    self.capabilities.provider.clone(),
                    "proposal-invalid",
                    "invalid.md",
                );
                invalid.projection_hash = ContentHash::from_bytes(b"stale projection").hex();
                vec![invalid]
            }
        };
        if matches!(self.reply, ReplyMode::CancelAndReturn) {
            cancellation.cancel();
        }
        Ok(proposals)
    }
}

fn capabilities(boundary: DataBoundary) -> ProviderCapabilities {
    ProviderCapabilities {
        provider: ProviderIdentity {
            provider: "sdk-fixture".into(),
            model: "deterministic-double".into(),
            version: Some("1".into()),
        },
        protocol_versions: vec![PROTOCOL_VERSION],
        operations: vec![ProviderOperation::KnowledgeAugmentation],
        max_input_bytes: 4 * 1024 * 1024,
        max_output_bytes: 4 * 1024 * 1024,
        structured_output: true,
        streaming: false,
        deterministic_controls: true,
        data_boundary: boundary,
    }
}

fn proposal(
    request: &AugmentationRequest,
    provider: ProviderIdentity,
    proposal_id: &str,
    path: &str,
) -> KnowledgeProposal {
    let document = request.documents.first().expect("selected document");
    KnowledgeProposal {
        schema_version: PROPOSAL_SCHEMA_VERSION,
        proposal_id: proposal_id.into(),
        plan_id: request.plan_id.clone(),
        projection_hash: request.projection_hash.clone(),
        provider,
        kind: ProposalKind::CreateGeneratedNote {
            title: format!("Generated {proposal_id}"),
            markdown_body: format!("# Generated {proposal_id}\n\nEvidence-bound replay output.\n"),
            suggested_path: Some(path.into()),
        },
        evidence: vec![EvidenceRefWire {
            snapshot_id: document.snapshot_id.clone(),
            document_id: document.document.id.clone(),
            block_id: None,
            byte_start: None,
            byte_end: None,
            content_hash: document.document.content_hash.clone(),
        }],
        uncertainty: Some(0.05),
        rationale: Some("deterministic SDK fixture".into()),
    }
}

fn fixture_context(remote_allowed: bool, source_id: &str) -> FixtureContext {
    let mut policy = CompilerPolicy::default();
    policy.augmentation.allow_remote_providers = remote_allowed;
    let compiler = VaultCompiler::builder()
        .policy(policy)
        .build()
        .expect("build replay compiler");
    let source = SourceSpec::directory(source_id, common::fixture("augmentation_replay"))
        .expect("augmentation fixture source");
    let inspection = compiler.inspect([source]).expect("inspect replay fixture");
    let plan = compiler.plan(&inspection).expect("plan replay fixture");
    let selected_id = plan
        .workspace
        .documents
        .values()
        .find(|document| document.source_file.logical_path == "Selected.md")
        .expect("selected fixture document")
        .document_id;
    FixtureContext {
        compiler,
        plan,
        selected_id,
    }
}

fn explicit_selection(context: &FixtureContext) -> DocumentSelection {
    DocumentSelection::Explicit(vec![context.selected_id])
}

fn live_recording(
    context: &FixtureContext,
    augmentor: &FixtureAugmentor,
    consent: RemoteProviderConsent,
) -> vaultc::Result<RecordedAugmentation> {
    context.compiler.augment(
        &context.plan,
        &explicit_selection(context),
        augmentor,
        &CancellationToken::default(),
        consent,
    )
}

fn jsonl_values(bytes: &[u8]) -> Vec<serde_json::Value> {
    assert_eq!(bytes.last(), Some(&b'\n'), "canonical JSONL final LF");
    bytes[..bytes.len() - 1]
        .split(|byte| *byte == b'\n')
        .map(|line| serde_json::from_slice(line).expect("fixture JSONL record"))
        .collect()
}

fn encode_jsonl(values: &[serde_json::Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend(to_canonical_json(value).expect("canonical fixture JSON record"));
        bytes.push(b'\n');
    }
    bytes
}

fn mutate_jsonl(bytes: &[u8], mutate: impl FnOnce(&mut Vec<serde_json::Value>)) -> Vec<u8> {
    let mut values = jsonl_values(bytes);
    mutate(&mut values);
    encode_jsonl(&values)
}

fn assert_replay_rejected(compiler: &VaultCompiler, plan: &DraftPlan, bytes: &[u8], case: &str) {
    let result = RecordedAugmentation::from_canonical_jsonl(bytes)
        .and_then(|recording| compiler.replay_augmentation(plan, &recording));
    assert!(result.is_err(), "replay accepted {case}");
}

fn approval_log(recording: &RecordedAugmentation) -> ApprovalLog {
    let validation = recording
        .validations()
        .first()
        .expect("one proposal validation");
    ApprovalLog {
        decisions: vec![ApprovalDecision {
            plan_id: validation.proposal.plan_id.clone(),
            proposal_id: validation.proposal.proposal_id.clone(),
            proposal_content_hash: validation.content_hash,
            approved: true,
            approver: "sdk-replay-qa".into(),
            policy_version: "test-v1".into(),
        }],
    }
}

fn assert_exact_transcript_shape(recording: &RecordedAugmentation) {
    let transcript = recording.transcript();
    assert_eq!(transcript.len(), 4);
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
    let expected_ids = [
        "capabilities-1",
        "capabilities-1",
        "augmentation-1",
        "augmentation-1",
    ];
    for (index, record) in transcript.iter().enumerate() {
        assert_eq!(record.sequence, index as u64);
        assert_eq!(record.message_type, expected_types[index]);
        assert_eq!(record.direction, expected_directions[index]);
        assert_eq!(record.request_id, expected_ids[index]);
    }
    assert_eq!(transcript[0].payload, serde_json::json!({}));
}

fn assert_transcript_hashes(recording: &RecordedAugmentation, sent_request: &AugmentationRequest) {
    for (index, record) in recording.transcript().iter().enumerate() {
        let expected_hash = if index == 2 {
            canonical_hash(TRANSCRIPT_HASH_DOMAIN, sent_request)
        } else {
            canonical_hash(TRANSCRIPT_HASH_DOMAIN, &record.payload)
        }
        .expect("transcript payload hash")
        .hex();
        assert_eq!(record.canonical_payload_hash, expected_hash);
    }
}

fn assert_canonical_recording_bytes(
    recording: &RecordedAugmentation,
    expected_records: usize,
) -> Vec<u8> {
    let bytes = recording
        .to_canonical_jsonl()
        .expect("canonical augmentation JSONL");
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert!(!bytes.contains(&b'\r'));
    assert_eq!(jsonl_values(&bytes).len(), expected_records);
    for line in bytes[..bytes.len() - 1].split(|byte| *byte == b'\n') {
        let value: serde_json::Value = serde_json::from_slice(line).expect("JSON line");
        assert_eq!(
            line,
            to_canonical_json(&value).expect("canonical JSON line")
        );
    }
    bytes
}

#[test]
fn augmentation_request_builder_is_deterministic_and_snapshot_bound() {
    let context = fixture_context(false, "sdk-replay-builder");
    let all = context
        .compiler
        .build_augmentation_request(&context.plan, &DocumentSelection::All)
        .expect("build all-document request");
    let repeated = context
        .compiler
        .build_augmentation_request(&context.plan, &DocumentSelection::All)
        .expect("repeat all-document request");
    assert_eq!(all, repeated);
    assert_eq!(all.documents.len(), context.plan.workspace.documents.len());

    for projection in &all.documents {
        let document_id = projection
            .document
            .id
            .parse::<DocumentId>()
            .expect("typed projected document ID");
        let sealed = &context.plan.workspace.documents[&document_id];
        assert_eq!(
            projection.snapshot_id,
            sealed.source_file.snapshot_id.to_string()
        );
        assert_eq!(projection.document.content_hash, sealed.body_hash.hex());
        assert_eq!(projection.logical_path, sealed.source_file.logical_path);
        assert_eq!(projection.title, sealed.title);
        assert_eq!(projection.selected_blocks.len(), sealed.blocks.len());
        for (projected, block) in projection.selected_blocks.iter().zip(&sealed.blocks) {
            assert_eq!(projected.block.id, block.block_id.to_string());
            assert_eq!(projected.block.content_hash, block.content_hash.hex());
            assert_eq!(projected.text, block.comparison_text);
        }
    }

    let mut reversed_ids: Vec<_> = context.plan.workspace.documents.keys().copied().collect();
    reversed_ids.reverse();
    let explicit = context
        .compiler
        .build_augmentation_request(
            &context.plan,
            &DocumentSelection::Explicit(reversed_ids.clone()),
        )
        .expect("build explicit request");
    assert_eq!(
        explicit.documents, all.documents,
        "caller order is irrelevant"
    );
    let request_text = String::from_utf8(to_canonical_json(&all).expect("canonical request"))
        .expect("request UTF-8");
    assert!(
        !request_text.contains(&common::fixture("augmentation_replay").display().to_string()),
        "provider projection must not disclose an absolute source locator"
    );

    assert!(
        context
            .compiler
            .build_augmentation_request(&context.plan, &DocumentSelection::Explicit(Vec::new()))
            .is_err(),
        "empty explicit disclosure must fail"
    );
    assert!(
        context
            .compiler
            .build_augmentation_request(
                &context.plan,
                &DocumentSelection::Explicit(vec![context.selected_id, context.selected_id])
            )
            .is_err(),
        "duplicate disclosure must fail"
    );
    let unknown = DocumentId::from_hash(ContentHash::from_bytes(b"unknown document"));
    assert!(
        context
            .compiler
            .build_augmentation_request(&context.plan, &DocumentSelection::Explicit(vec![unknown]))
            .is_err(),
        "unknown disclosure must fail"
    );
}

#[test]
fn all_selection_on_zero_document_plan_fails_before_provider_call() {
    let compiler = common::compiler();
    let source = SourceSpec::directory(
        "sdk-replay-no-documents",
        common::fixture("augmentation_replay/empty"),
    )
    .expect("asset-only source");
    let inspection = compiler
        .inspect([source])
        .expect("inspect asset-only fixture");
    let plan = compiler.plan(&inspection).expect("plan asset-only fixture");
    assert!(plan.workspace.documents.is_empty());
    let error = compiler
        .build_augmentation_request(&plan, &DocumentSelection::All)
        .expect_err("All requires at least one document");
    assert!(matches!(error, VaultcError::InvalidConfig(_)));

    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::Empty);
    let error = compiler
        .augment(
            &plan,
            &DocumentSelection::All,
            &augmentor,
            &CancellationToken::default(),
            RemoteProviderConsent::Denied,
        )
        .expect_err("empty disclosure must fail before provider negotiation");
    assert!(matches!(error, VaultcError::InvalidConfig(_)));
    assert_eq!(augmentor.capability_calls(), 0);
    assert_eq!(augmentor.proposal_calls(), 0);
}

#[test]
fn live_recording_has_exact_four_canonical_transcript_records() {
    let context = fixture_context(false, "sdk-replay-live");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::TwoValidReverseOrder);
    let recording = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("record live augmentation");
    assert_eq!(augmentor.capability_calls(), 1);
    assert_eq!(augmentor.proposal_calls(), 1);
    assert_eq!(recording.schema_version(), 1);
    assert_eq!(recording.plan_id(), context.plan.plan_id);
    assert_eq!(recording.projection_hash(), context.plan.projection_hash);
    assert_eq!(recording.provider(), &augmentor.capabilities.provider);

    assert_exact_transcript_shape(&recording);

    let sent_request = augmentor.request();
    assert_transcript_hashes(&recording, &sent_request);
    let transcript = recording.transcript();
    let stored_request: AugmentationRequest =
        serde_json::from_value(transcript[2].payload.clone()).expect("stored request payload");
    assert!(stored_request.documents.iter().all(|document| {
        document
            .selected_blocks
            .iter()
            .all(|block| block.text == "[redacted]")
    }));

    let response: AugmentationResponse =
        serde_json::from_value(transcript[3].payload.clone()).expect("recorded response");
    let response_ids: Vec<_> = response
        .proposals
        .iter()
        .map(|proposal| proposal.proposal_id.as_str())
        .collect();
    assert_eq!(response_ids, ["proposal-z", "proposal-a"]);
    let validation_ids: Vec<_> = recording
        .validations()
        .iter()
        .map(|validation| validation.proposal.proposal_id.as_str())
        .collect();
    assert_eq!(validation_ids, ["proposal-a", "proposal-z"]);

    let bytes = assert_canonical_recording_bytes(&recording, 7);
    let bytes_text = String::from_utf8(bytes.clone()).expect("UTF-8 recording");
    assert!(!bytes_text.contains(SELECTED_SENTINEL));
    assert!(!bytes_text.contains(UNSELECTED_SENTINEL));
    let sent_text = serde_json::to_string(&sent_request).expect("sent request JSON");
    assert!(sent_text.contains(SELECTED_SENTINEL));
    assert!(!sent_text.contains(UNSELECTED_SENTINEL));

    let authorization = context
        .compiler
        .authorize_augmentation_exchange(
            &context.plan,
            &sent_request,
            &augmentor.capabilities,
            RemoteProviderConsent::Denied,
        )
        .expect("authorize pre-negotiated exchange");
    let transport_recording = context
        .compiler
        .record_augmentation_exchange(&context.plan, authorization, &response)
        .expect("record pre-negotiated exchange");
    assert_eq!(
        transport_recording
            .to_canonical_jsonl()
            .expect("transport recording bytes"),
        bytes
    );
}

#[test]
fn offline_replay_is_provider_free_and_byte_identical() {
    let context = fixture_context(false, "sdk-replay-offline");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneValid);
    let recording = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("record live augmentation");
    let live_bytes = recording
        .to_canonical_jsonl()
        .expect("serialize live recording");
    let calls_after_live = (augmentor.capability_calls(), augmentor.proposal_calls());
    let replayed = context
        .compiler
        .replay_augmentation(&context.plan, &recording)
        .expect("offline replay");
    assert_eq!(
        (augmentor.capability_calls(), augmentor.proposal_calls()),
        calls_after_live,
        "offline replay must not invoke a provider"
    );
    assert_eq!(
        replayed
            .to_canonical_jsonl()
            .expect("serialize replayed recording"),
        live_bytes
    );

    let live_copy =
        RecordedAugmentation::from_canonical_jsonl(&live_bytes).expect("decode live approval copy");
    let replay_copy = RecordedAugmentation::from_canonical_jsonl(&live_bytes)
        .expect("decode replay approval copy");
    let approvals = approval_log(&live_copy);
    let live_approved = context
        .compiler
        .approve(
            context.plan.clone(),
            live_copy.into_validated(),
            approvals.clone(),
        )
        .expect("approve live recording");
    let replay_approved = context
        .compiler
        .approve(
            context.plan.clone(),
            replay_copy.into_validated(),
            approvals,
        )
        .expect("approve replay recording");
    assert_eq!(
        to_canonical_json(&live_approved).expect("canonical live approval"),
        to_canonical_json(&replay_approved).expect("canonical replay approval")
    );

    let temporary = tempfile::tempdir().expect("replay E2E parent");
    let live_output = temporary.path().join("live-output");
    let replay_output = temporary.path().join("replay-output");
    context
        .compiler
        .compile(&live_approved, &live_output)
        .expect("compile live approval");
    context
        .compiler
        .compile(&replay_approved, &replay_output)
        .expect("compile replay approval");
    assert_eq!(
        common::tree_bytes(&live_output),
        common::tree_bytes(&replay_output)
    );

    let live_pack = temporary.path().join("live.vaultpack");
    let replay_pack = temporary.path().join("replay.vaultpack");
    let zstd_level = context.plan.policy.output.zstd_level;
    create_pack(&live_output, &live_pack, zstd_level).expect("pack live output");
    create_pack(&replay_output, &replay_pack, zstd_level).expect("pack replay output");
    assert_eq!(
        std::fs::read(live_pack).expect("read live pack"),
        std::fs::read(replay_pack).expect("read replay pack")
    );
}

#[test]
fn approval_rejects_nonempty_validations_without_transcript() {
    let context = fixture_context(false, "sdk-replay-empty-transcript");
    let request = context
        .compiler
        .build_augmentation_request(&context.plan, &explicit_selection(&context))
        .expect("build validation request");
    let valid = proposal(
        &request,
        capabilities(DataBoundary::Local).provider,
        "proposal-no-transcript-valid",
        "no-transcript-valid.md",
    );
    let valid_without_transcript = context
        .compiler
        .validate_proposals(&context.plan, vec![valid])
        .expect("validate proposal without recording");
    let error = context
        .compiler
        .approve(
            context.plan.clone(),
            valid_without_transcript,
            ApprovalLog::default(),
        )
        .expect_err("valid proposal validation without transcript must fail");
    assert!(matches!(error, VaultcError::ApprovalStale(_)));

    let mut invalid = proposal(
        &request,
        capabilities(DataBoundary::Local).provider,
        "proposal-no-transcript-invalid",
        "no-transcript-invalid.md",
    );
    invalid.plan_id = "plan_stale".into();
    let invalid_without_transcript = context
        .compiler
        .validate_proposals(&context.plan, vec![invalid])
        .expect("record deterministic invalid validation");
    assert!(!invalid_without_transcript.validations[0].valid);
    assert!(
        context
            .compiler
            .approve(
                context.plan.clone(),
                invalid_without_transcript,
                ApprovalLog::default()
            )
            .is_err(),
        "invalid proposal validation without transcript must also fail"
    );

    context
        .compiler
        .approve(
            context.plan.clone(),
            vaultc::ValidatedProposals::default(),
            ApprovalLog::default(),
        )
        .expect("zero validations and zero transcript remain valid");

    let empty_provider = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::Empty);
    let recorded_zero = live_recording(&context, &empty_provider, RemoteProviderConsent::Denied)
        .expect("record zero-proposal exchange");
    assert!(recorded_zero.validations().is_empty());
    assert_eq!(recorded_zero.transcript().len(), 4);
    context
        .compiler
        .approve(
            context.plan.clone(),
            recorded_zero.into_validated(),
            ApprovalLog::default(),
        )
        .expect("zero validations with canonical exchange remain valid");
}

#[test]
fn approval_rejects_forged_recorded_validation_without_explicit_replay() {
    let context = fixture_context(false, "sdk-replay-forged-validation");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneValid);
    let canonical = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("baseline validation recording")
        .to_canonical_jsonl()
        .expect("baseline validation JSONL");

    let forged_hash = mutate_jsonl(&canonical, |values| {
        *values[5]
            .pointer_mut("/validation/content_hash")
            .expect("validation content hash") = serde_json::json!("00".repeat(32));
    });
    let forged_reasons = mutate_jsonl(&canonical, |values| {
        *values[5]
            .pointer_mut("/validation/reasons")
            .expect("validation reasons") = serde_json::json!(["forged reason"]);
    });
    let forged_validity = mutate_jsonl(&canonical, |values| {
        *values[5]
            .pointer_mut("/validation/valid")
            .expect("validation validity") = serde_json::json!(false);
    });

    for (name, bytes, with_decision) in [
        ("content hash", forged_hash, true),
        ("reasons", forged_reasons, true),
        ("validity", forged_validity, false),
    ] {
        let recording = RecordedAugmentation::from_canonical_jsonl(&bytes)
            .unwrap_or_else(|error| panic!("decode {name} forgery: {error}"));
        let decisions = if with_decision {
            approval_log(&recording)
        } else {
            ApprovalLog::default()
        };
        assert!(
            context
                .compiler
                .approve(context.plan.clone(), recording.into_validated(), decisions)
                .is_err(),
            "approval accepted forged validation {name}"
        );
    }
}

#[test]
fn remote_provider_requires_policy_and_runtime_consent() {
    for (policy_allows, consent, expected_success) in [
        (false, RemoteProviderConsent::Denied, false),
        (false, RemoteProviderConsent::Granted, false),
        (true, RemoteProviderConsent::Denied, false),
        (true, RemoteProviderConsent::Granted, true),
    ] {
        let context = fixture_context(policy_allows, &format!("sdk-remote-{policy_allows}"));
        let augmentor = FixtureAugmentor::new(
            DataBoundary::Remote {
                endpoint_label: "fixture-remote".into(),
            },
            ReplyMode::Empty,
        );
        let result = live_recording(&context, &augmentor, consent);
        assert_eq!(result.is_ok(), expected_success);
        assert_eq!(augmentor.capability_calls(), 1);
        assert_eq!(augmentor.proposal_calls(), usize::from(expected_success));
        if let Ok(recording) = result {
            let calls = (augmentor.capability_calls(), augmentor.proposal_calls());
            context
                .compiler
                .replay_augmentation(&context.plan, &recording)
                .expect("remote recording replays without durable runtime consent");
            assert_eq!(
                (augmentor.capability_calls(), augmentor.proposal_calls()),
                calls
            );
        }
    }

    let local_context = fixture_context(false, "sdk-local-no-consent");
    let local = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::Empty);
    live_recording(&local_context, &local, RemoteProviderConsent::Denied)
        .expect("local provider does not need remote consent");
    assert_eq!(local.proposal_calls(), 1);
}

#[test]
fn transport_preflight_denies_remote_disclosure_before_response_collection() {
    let disclosure_calls = AtomicUsize::new(0);
    for (policy_allows, consent) in [
        (false, RemoteProviderConsent::Granted),
        (true, RemoteProviderConsent::Denied),
    ] {
        let context = fixture_context(policy_allows, "sdk-transport-preflight-denied");
        let request = context
            .compiler
            .build_augmentation_request(&context.plan, &explicit_selection(&context))
            .expect("build preflight request");
        let capabilities = capabilities(DataBoundary::Remote {
            endpoint_label: "fixture-remote".into(),
        });
        assert!(
            context
                .compiler
                .authorize_augmentation_exchange(&context.plan, &request, &capabilities, consent,)
                .is_err()
        );
        assert_eq!(disclosure_calls.load(Ordering::Acquire), 0);
    }

    let context = fixture_context(true, "sdk-transport-preflight-granted");
    let request = context
        .compiler
        .build_augmentation_request(&context.plan, &explicit_selection(&context))
        .expect("build permitted preflight request");
    let capabilities = capabilities(DataBoundary::Remote {
        endpoint_label: "fixture-remote".into(),
    });
    let authorization = context
        .compiler
        .authorize_augmentation_exchange(
            &context.plan,
            &request,
            &capabilities,
            RemoteProviderConsent::Granted,
        )
        .expect("authorize remote disclosure");
    disclosure_calls.fetch_add(1, Ordering::AcqRel);
    assert_eq!(authorization.request(), &request);
    context
        .compiler
        .record_augmentation_exchange(
            &context.plan,
            authorization,
            &AugmentationResponse {
                proposals: Vec::new(),
            },
        )
        .expect("record authorized transport response");
    assert_eq!(disclosure_calls.load(Ordering::Acquire), 1);
}

#[test]
fn sdk_augmentation_cancellation_is_atomic() {
    let context = fixture_context(false, "sdk-replay-cancel");
    let source_before = common::tree_bytes(&common::fixture("augmentation_replay"));
    let plan_before = to_canonical_json(&context.plan).expect("canonical plan before cancellation");

    let pre_cancelled = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneValid);
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let error = context
        .compiler
        .augment(
            &context.plan,
            &explicit_selection(&context),
            &pre_cancelled,
            &cancellation,
            RemoteProviderConsent::Denied,
        )
        .expect_err("pre-cancelled augmentation must fail");
    assert!(matches!(error, VaultcError::Provider(_)));
    assert_eq!(pre_cancelled.capability_calls(), 0);
    assert_eq!(pre_cancelled.proposal_calls(), 0);

    let mid_cancelled = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::CancelAndReturn);
    let error = live_recording(&context, &mid_cancelled, RemoteProviderConsent::Denied)
        .expect_err("cancellation during provider call must not return a recording");
    assert!(matches!(error, VaultcError::Provider(_)));
    assert_eq!(mid_cancelled.capability_calls(), 1);
    assert_eq!(mid_cancelled.proposal_calls(), 1);

    assert_eq!(
        to_canonical_json(&context.plan).expect("canonical plan after cancellation"),
        plan_before
    );
    assert_eq!(
        common::tree_bytes(&common::fixture("augmentation_replay")),
        source_before
    );
}

#[test]
fn replay_rehydrates_exact_redacted_request() {
    let context = fixture_context(false, "sdk-replay-redaction");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneValid);
    let recording = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("record redacted request");
    let bytes = recording
        .to_canonical_jsonl()
        .expect("serialize redacted request");
    let request = augmentor.request();
    let request_text = serde_json::to_string(&request).expect("sent request JSON");
    let recording_text = String::from_utf8(bytes.clone()).expect("recording UTF-8");
    assert!(request_text.contains(SELECTED_SENTINEL));
    assert!(!request_text.contains(UNSELECTED_SENTINEL));
    assert!(!recording_text.contains(SELECTED_SENTINEL));
    assert!(!recording_text.contains(UNSELECTED_SENTINEL));
    context
        .compiler
        .replay_augmentation(&context.plan, &recording)
        .expect("rehydrate exact sealed request");

    let unredacted_storage = mutate_jsonl(&bytes, |values| {
        *values[3]
            .pointer_mut("/record/payload/documents/0/selected_blocks/0/text")
            .expect("stored block text") = serde_json::json!("not-redacted");
    });
    assert_replay_rejected(
        &context.compiler,
        &context.plan,
        &unredacted_storage,
        "stored unredacted block text",
    );

    let mut stale_sent_request = request;
    stale_sent_request.documents[0].snapshot_id = "snap_stale".into();
    let stale_hash = canonical_hash(TRANSCRIPT_HASH_DOMAIN, &stale_sent_request)
        .expect("stale request hash")
        .hex();
    let stale_snapshot = mutate_jsonl(&bytes, |values| {
        *values[3]
            .pointer_mut("/record/payload/documents/0/snapshot_id")
            .expect("stored snapshot ID") = serde_json::json!("snap_stale");
        *values[3]
            .pointer_mut("/record/canonical_payload_hash")
            .expect("request payload hash") = serde_json::json!(stale_hash);
    });
    assert_replay_rejected(
        &context.compiler,
        &context.plan,
        &stale_snapshot,
        "stale snapshot with internally consistent payload hash",
    );

    let other = fixture_context(false, "sdk-replay-cross-plan");
    assert!(
        other
            .compiler
            .replay_augmentation(&other.plan, &recording)
            .is_err(),
        "recording cannot cross sealed plan identity"
    );
}

#[test]
fn invalid_provider_proposal_replays_as_identical_rejection() {
    let context = fixture_context(false, "sdk-replay-invalid-proposal");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneInvalid);
    let recording = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("invalid proposal still produces an auditable recording");
    assert_eq!(recording.validations().len(), 1);
    assert!(!recording.validations()[0].valid);
    let bytes = recording
        .to_canonical_jsonl()
        .expect("serialize rejected proposal recording");
    let replayed = context
        .compiler
        .replay_augmentation(&context.plan, &recording)
        .expect("replay deterministic rejection");
    assert_eq!(
        replayed
            .to_canonical_jsonl()
            .expect("serialize replayed rejection"),
        bytes
    );

    let approved = context
        .compiler
        .approve(
            context.plan.clone(),
            RecordedAugmentation::from_canonical_jsonl(&bytes)
                .expect("decode rejected recording")
                .into_validated(),
            ApprovalLog::default(),
        )
        .expect("rejected proposal can be retained without materialization");
    assert!(approved.approved_proposals.is_empty());

    let decision = approval_log(&recording);
    assert!(
        context
            .compiler
            .approve(context.plan.clone(), recording.into_validated(), decision)
            .is_err(),
        "an invalid proposal cannot be approved"
    );
}

#[test]
fn recorded_augmentation_rejects_noncanonical_jsonl() {
    let context = fixture_context(false, "sdk-replay-noncanonical");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneValid);
    let canonical = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("baseline recording")
        .to_canonical_jsonl()
        .expect("baseline canonical JSONL");

    let mut no_final_lf = canonical.clone();
    no_final_lf.pop();
    let crlf = canonical
        .iter()
        .flat_map(|byte| {
            if *byte == b'\n' {
                vec![b'\r', b'\n']
            } else {
                vec![*byte]
            }
        })
        .collect::<Vec<_>>();
    let first_lf = canonical
        .iter()
        .position(|byte| *byte == b'\n')
        .expect("header LF");
    let mut blank_line = canonical.clone();
    blank_line.insert(first_lf + 1, b'\n');
    let mut leading_space = canonical.clone();
    leading_space.insert(0, b' ');
    let unknown_field = mutate_jsonl(&canonical, |values| {
        values[0]
            .as_object_mut()
            .expect("header object")
            .insert("unknown".into(), serde_json::json!(true));
    });
    let canonical_text = String::from_utf8(canonical.clone()).expect("canonical UTF-8");
    let duplicate_field = canonical_text
        .replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        )
        .into_bytes();

    for (name, bytes) in [
        ("empty", Vec::new()),
        ("unterminated", no_final_lf),
        ("CRLF", crlf),
        ("blank line", blank_line),
        ("leading whitespace", leading_space),
        ("unknown field", unknown_field),
        ("duplicate field", duplicate_field),
    ] {
        let error = RecordedAugmentation::from_canonical_jsonl(&bytes).expect_err(name);
        assert!(matches!(error, VaultcError::Provider(_)), "{name}: {error}");
    }
}

type ReplayAttack = (&'static str, Vec<u8>);

fn transcript_identity_attacks(canonical: &[u8]) -> [ReplayAttack; 4] {
    let sequence_gap = mutate_jsonl(canonical, |values| {
        *values[2]
            .pointer_mut("/record/sequence")
            .expect("transcript sequence") = serde_json::json!(7);
    });
    let stale_hash = mutate_jsonl(canonical, |values| {
        *values[4]
            .pointer_mut("/record/canonical_payload_hash")
            .expect("response payload hash") = serde_json::json!("00".repeat(32));
    });
    let stale_header = mutate_jsonl(canonical, |values| {
        *values[0].pointer_mut("/plan_id").expect("header plan ID") =
            serde_json::json!("plan_stale");
    });
    let header_provider_mismatch = mutate_jsonl(canonical, |values| {
        *values[0]
            .pointer_mut("/provider/provider")
            .expect("header provider") = serde_json::json!("different-provider");
    });

    [
        ("sequence gap", sequence_gap),
        ("stale transcript hash", stale_hash),
        ("stale header plan", stale_header),
        ("header provider mismatch", header_provider_mismatch),
    ]
}

fn provider_validation_attacks(canonical: &[u8]) -> [ReplayAttack; 3] {
    let capability_provider_mismatch = mutate_jsonl(canonical, |values| {
        let capability_payload = values[2]
            .pointer_mut("/record/payload")
            .expect("capability payload");
        *capability_payload
            .pointer_mut("/provider/provider")
            .expect("capability provider") = serde_json::json!("different-provider");
        let hash = canonical_hash(TRANSCRIPT_HASH_DOMAIN, capability_payload)
            .expect("changed capability hash")
            .hex();
        *values[2]
            .pointer_mut("/record/canonical_payload_hash")
            .expect("capability payload hash") = serde_json::json!(hash);
    });
    let response_validation_mismatch = mutate_jsonl(canonical, |values| {
        let response_payload = values[4]
            .pointer_mut("/record/payload")
            .expect("response payload");
        *response_payload
            .pointer_mut("/proposals")
            .expect("response proposals") = serde_json::json!([]);
        let hash = canonical_hash(TRANSCRIPT_HASH_DOMAIN, response_payload)
            .expect("changed response hash")
            .hex();
        *values[4]
            .pointer_mut("/record/canonical_payload_hash")
            .expect("response payload hash") = serde_json::json!(hash);
    });
    let validation_mismatch = mutate_jsonl(canonical, |values| {
        let valid = values[5]
            .pointer_mut("/validation/valid")
            .expect("validation valid field");
        *valid = serde_json::Value::Bool(!valid.as_bool().expect("boolean validation"));
    });

    [
        ("capability provider mismatch", capability_provider_mismatch),
        ("response/validation mismatch", response_validation_mismatch),
        ("deterministic validation mismatch", validation_mismatch),
    ]
}

fn unknown_payload_attacks(canonical: &[u8]) -> [ReplayAttack; 3] {
    let unknown_capability_field = mutate_jsonl(canonical, |values| {
        let payload = values[2]
            .pointer_mut("/record/payload")
            .expect("capability payload");
        payload
            .as_object_mut()
            .expect("capability object")
            .insert("unknown_capability_field".into(), serde_json::json!(true));
        let hash = canonical_hash(TRANSCRIPT_HASH_DOMAIN, payload)
            .expect("unknown capability payload hash")
            .hex();
        *values[2]
            .pointer_mut("/record/canonical_payload_hash")
            .expect("capability payload hash") = serde_json::json!(hash);
    });
    let unknown_request_field = mutate_jsonl(canonical, |values| {
        values[3]
            .pointer_mut("/record/payload")
            .expect("request payload")
            .as_object_mut()
            .expect("request object")
            .insert("unknown_request_field".into(), serde_json::json!(true));
    });
    let unknown_response_field = mutate_jsonl(canonical, |values| {
        let payload = values[4]
            .pointer_mut("/record/payload")
            .expect("response payload");
        payload
            .as_object_mut()
            .expect("response object")
            .insert("unknown_response_field".into(), serde_json::json!(true));
        let hash = canonical_hash(TRANSCRIPT_HASH_DOMAIN, payload)
            .expect("unknown response payload hash")
            .hex();
        *values[4]
            .pointer_mut("/record/canonical_payload_hash")
            .expect("response payload hash") = serde_json::json!(hash);
    });

    [
        ("unknown capability payload field", unknown_capability_field),
        ("unknown request payload field", unknown_request_field),
        ("unknown response payload field", unknown_response_field),
    ]
}

fn spoofed_header_attacks(canonical: &[u8]) -> [ReplayAttack; 3] {
    let provider = mutate_jsonl(canonical, |values| {
        *values[0]
            .pointer_mut("/provider/provider")
            .expect("header provider") = serde_json::json!("spoofed-provider");
    });
    let plan = mutate_jsonl(canonical, |values| {
        *values[0].pointer_mut("/plan_id").expect("header plan ID") =
            serde_json::json!(format!("plan_{}", "00".repeat(32)));
    });
    let projection = mutate_jsonl(canonical, |values| {
        *values[0]
            .pointer_mut("/projection_hash")
            .expect("header projection hash") = serde_json::json!("00".repeat(32));
    });
    [
        ("spoofed header provider", provider),
        ("spoofed header plan", plan),
        ("spoofed header projection", projection),
    ]
}

#[test]
fn decoder_rejects_nested_unknown_fields_and_spoofed_header_bindings() {
    let context = fixture_context(false, "sdk-replay-decoder-boundary");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneValid);
    let canonical = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("baseline decoder recording")
        .to_canonical_jsonl()
        .expect("baseline decoder JSONL");
    for (name, bytes) in unknown_payload_attacks(&canonical)
        .into_iter()
        .chain(spoofed_header_attacks(&canonical))
    {
        assert!(
            RecordedAugmentation::from_canonical_jsonl(&bytes).is_err(),
            "decoder accepted {name}"
        );
    }
}

#[test]
fn replay_rejects_malformed_stale_provider_and_validation_mismatches() {
    let context = fixture_context(false, "sdk-replay-adversarial");
    let augmentor = FixtureAugmentor::new(DataBoundary::Local, ReplyMode::OneValid);
    let canonical = live_recording(&context, &augmentor, RemoteProviderConsent::Denied)
        .expect("baseline adversarial recording")
        .to_canonical_jsonl()
        .expect("baseline adversarial JSONL");
    let accepted: Vec<_> = transcript_identity_attacks(&canonical)
        .into_iter()
        .chain(provider_validation_attacks(&canonical))
        .chain(unknown_payload_attacks(&canonical))
        .filter_map(|(name, bytes)| {
            let result = RecordedAugmentation::from_canonical_jsonl(&bytes)
                .and_then(|item| context.compiler.replay_augmentation(&context.plan, &item));
            result.is_ok().then_some(name)
        })
        .collect();
    assert!(accepted.is_empty(), "replay accepted attacks: {accepted:?}");
}
