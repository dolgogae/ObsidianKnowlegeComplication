mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::str::FromStr as _;

use okc_core::approval::{ConflictAction, CuratorId, DecisionOverlay, DecisionOverlayLog};
use okc_core::compile::ArtifactManifest;
use okc_core::identity::{ContentHash, EvidenceId, RecordId};
use okc_core::plan::{ConflictKind, ConflictSubject};
use okc_core::provenance::{
    AttributionKind, AttributionState, BuildInputKind, BuildInputSourceRecord, EdgePosition,
    EdgeRecord, EdgeRelation, OperationRecord, OutputRole, OutputStorage, ProvenanceQuery,
    ProvenanceRecord, ProvenanceRecordKind, ProvenanceSubject, SourceRecord,
};
use okc_core::{
    ApprovalDecision, ApprovalLog, ApprovedPlan, DraftPlan, OkcCompiler, OkcError,
    ValidatedProposals,
};
use okc_protocol::{
    EvidenceRefWire, KnowledgeProposal, PROPOSAL_SCHEMA_VERSION, ProposalKind, ProviderIdentity,
};

const GENERATED_PATH: &str = "knowledge/_generated/typed-provenance.md";

fn evidence_for(document: &okc_core::ir::Document) -> EvidenceRefWire {
    EvidenceRefWire {
        snapshot_id: document.source_file.snapshot_id.to_string(),
        document_id: document.document_id.to_string(),
        block_id: None,
        byte_start: None,
        byte_end: None,
        content_hash: document.source_file.content_hash.hex(),
    }
}

fn generated_proposal(plan: &DraftPlan) -> KnowledgeProposal {
    let evidence_paths = ["Declared.md", "NotDeclared.md", "Opaque.md"];
    let evidence = evidence_paths
        .iter()
        .map(|path| {
            let document = plan
                .workspace
                .documents
                .values()
                .find(|document| document.source_file.logical_path == *path)
                .unwrap_or_else(|| panic!("typed fixture document `{path}`"));
            evidence_for(document)
        })
        .collect();
    KnowledgeProposal {
        schema_version: PROPOSAL_SCHEMA_VERSION,
        proposal_id: "proposal-typed-provenance".into(),
        plan_id: plan.plan_id.to_string(),
        projection_hash: plan.projection_hash.hex(),
        provider: ProviderIdentity {
            provider: "local-fixture".into(),
            model: "deterministic-test-double".into(),
            version: Some("1".into()),
        },
        kind: ProposalKind::CreateGeneratedNote {
            title: "Typed provenance".into(),
            markdown_body: "# Typed provenance\n\nGenerated from all attribution states.\n".into(),
            suggested_path: Some("typed-provenance.md".into()),
        },
        evidence,
        uncertainty: Some(0.1),
        rationale: Some("typed provenance acceptance".into()),
    }
}

fn approved_generated_fixture() -> (OkcCompiler, ApprovedPlan, KnowledgeProposal) {
    approved_generated_fixture_for("typed-provenance")
}

fn approved_generated_fixture_for(
    source_id: &str,
) -> (OkcCompiler, ApprovedPlan, KnowledgeProposal) {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source(source_id, "typed_provenance")])
        .expect("inspect typed provenance fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan typed provenance fixture");
    let proposal = generated_proposal(&plan);
    let validated = common::record_proposals(&compiler, &plan, vec![proposal.clone()]);
    let proposal_content_hash = validated
        .valid()
        .next()
        .expect("typed proposal is valid")
        .content_hash;
    let approved = compiler
        .approve(
            plan.clone(),
            validated,
            ApprovalLog {
                decisions: vec![ApprovalDecision {
                    plan_id: plan.plan_id.to_string(),
                    proposal_id: proposal.proposal_id.clone(),
                    proposal_content_hash,
                    approved: true,
                    approver: "qa-typed-provenance".into(),
                    policy_version: "test-v2".into(),
                }],
            },
        )
        .expect("approve typed provenance proposal");
    (compiler, approved, proposal)
}

fn read_stored_records(artifact: &std::path::Path) -> (Vec<u8>, Vec<ProvenanceRecord>) {
    let bytes = fs::read(artifact.join(".okc/provenance.jsonl"))
        .expect("read stored typed provenance graph");
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.contains(&b'\r'));
    let records = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let record: ProvenanceRecord =
                serde_json::from_slice(line).expect("decode typed provenance record");
            assert_eq!(
                okc_core::canonical::to_canonical_json(&record)
                    .expect("canonical typed provenance record"),
                line
            );
            record.validate_identity().expect("valid typed RecordId");
            record
        })
        .collect();
    (bytes, records)
}

fn sort_records(records: &mut [ProvenanceRecord]) {
    records.sort_by(|left, right| {
        left.type_order().cmp(&right.type_order()).then_with(|| {
            left.record_id
                .hash()
                .as_bytes()
                .cmp(right.record_id.hash().as_bytes())
        })
    });
}

fn encode_records(records: &[ProvenanceRecord]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend(
            okc_core::canonical::to_canonical_json(record).expect("encode typed graph record"),
        );
        bytes.push(b'\n');
    }
    bytes
}

fn refresh_checksum(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed graph file");
    let hash = hex::encode(Sha256::digest(&bytes));
    let checksum_path = root.join(".okc/checksums.txt");
    let checksums = fs::read_to_string(&checksum_path).expect("read typed graph checksums");
    let mut replaced = false;
    let mut rewritten = String::new();
    for line in checksums.lines() {
        let (_, path) = line.split_once("  ").expect("valid checksum line");
        if path == logical_path {
            writeln!(&mut rewritten, "{hash}  {logical_path}")
                .expect("write refreshed graph checksum");
            replaced = true;
        } else {
            writeln!(&mut rewritten, "{line}").expect("write unchanged checksum");
        }
    }
    assert!(replaced, "`{logical_path}` must be checksummed");
    fs::write(checksum_path, rewritten).expect("write refreshed checksums");
}

fn reseal_graph_bytes(root: &Path, bytes: &[u8]) {
    use sha2::{Digest as _, Sha256};

    let provenance_path = root.join(".okc/provenance.jsonl");
    fs::write(&provenance_path, bytes).expect("write deliberately forged typed graph");
    let manifest_path = root.join(".okc/manifest.json");
    let mut manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read typed graph manifest"))
            .expect("decode typed graph manifest");
    let file = manifest
        .files
        .iter_mut()
        .find(|file| file.path == ".okc/provenance.jsonl")
        .expect("manifest inventories typed graph");
    file.byte_len = bytes.len() as u64;
    file.raw_sha256 = hex::encode(Sha256::digest(bytes));
    manifest.provenance_graph_hash = okc_core::provenance::stored_graph_hash(bytes);
    manifest.artifact_id = okc_core::canonical::canonical_hash(
        "okc:artifact:v2\0",
        &(
            &manifest.plan_id,
            &manifest.files,
            &manifest.approved_proposal_hashes,
        ),
    )
    .expect("recalculate deliberately forged artifact identity");
    fs::write(
        &manifest_path,
        okc_core::canonical::to_canonical_json_pretty(&manifest)
            .expect("encode deliberately forged manifest"),
    )
    .expect("write deliberately forged manifest");
    refresh_checksum(root, ".okc/provenance.jsonl");
    refresh_checksum(root, ".okc/manifest.json");
}

fn reseal_records(root: &Path, records: &mut [ProvenanceRecord]) {
    sort_records(records);
    reseal_graph_bytes(root, &encode_records(records));
}

fn assert_graph_rejected(compiler: &OkcCompiler, artifact: &Path, attack: &str) {
    let Err(error) = compiler.verify(artifact) else {
        panic!("accepted fully resealed provenance attack `{attack}`");
    };
    assert!(
        matches!(
            error,
            OkcError::VerificationFailed(_)
                | OkcError::MalformedInput { .. }
                | OkcError::ResourceLimit(_)
        ),
        "`{attack}` must fail at a provenance verification boundary, got {error}"
    );
}

fn replace_node_and_references(
    records: &mut [ProvenanceRecord],
    index: usize,
    kind: ProvenanceRecordKind,
) {
    let old_id = records[index].record_id;
    let replacement = ProvenanceRecord::new(kind).expect("re-identify forged typed record");
    let new_id = replacement.record_id;
    records[index] = replacement;
    for record in records {
        let ProvenanceRecordKind::Edge(edge) = &record.kind else {
            continue;
        };
        if edge.from != old_id && edge.to != old_id {
            continue;
        }
        let mut edge = edge.clone();
        if edge.from == old_id {
            edge.from = new_id;
        }
        if edge.to == old_id {
            edge.to = new_id;
        }
        *record = ProvenanceRecord::new(ProvenanceRecordKind::Edge(edge))
            .expect("re-identify forged typed edge");
    }
}

fn outgoing(records: &[ProvenanceRecord], from: RecordId) -> Vec<&EdgeRecord> {
    records
        .iter()
        .filter_map(|record| match &record.kind {
            ProvenanceRecordKind::Edge(edge) if edge.from == from => Some(edge),
            _ => None,
        })
        .collect()
}

fn records_by_id(records: &[ProvenanceRecord]) -> BTreeMap<RecordId, &ProvenanceRecordKind> {
    records
        .iter()
        .map(|record| (record.record_id, &record.kind))
        .collect()
}

fn output_path(record: &ProvenanceRecord) -> Option<&str> {
    let ProvenanceRecordKind::Output(output) = &record.kind else {
        return None;
    };
    match &output.subject {
        ProvenanceSubject::ArtifactPath { path } => Some(path),
        ProvenanceSubject::Package => None,
    }
}

fn assert_strict_record_order(records: &[ProvenanceRecord]) {
    for pair in records.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        assert!(
            left.type_order() < right.type_order()
                || (left.type_order() == right.type_order()
                    && left.record_id.hash().as_bytes() < right.record_id.hash().as_bytes()),
            "typed records must be strictly sorted by type and raw RecordId: {} then {}",
            left.record_id,
            right.record_id
        );
    }
}

fn assert_every_output_has_exact_producer(records: &[ProvenanceRecord]) {
    for record in records {
        let ProvenanceRecordKind::Output(output) = &record.kind else {
            continue;
        };
        let producers: Vec<_> = outgoing(records, record.record_id)
            .into_iter()
            .filter(|edge| edge.relation == EdgeRelation::DerivedFrom)
            .collect();
        assert_eq!(
            producers.len(),
            1,
            "output {:?} must have exactly one producer",
            output.subject
        );
        assert_eq!(producers[0].to, output.producing_operation);
    }
}

fn vault_file_attribution<'a>(
    records: &'a [ProvenanceRecord],
    source_path: &str,
) -> &'a AttributionState {
    let states: Vec<_> = records
        .iter()
        .filter_map(|record| match &record.kind {
            ProvenanceRecordKind::Source(SourceRecord::VaultFile(source))
                if source.source_path == source_path =>
            {
                Some(&source.attribution)
            }
            _ => None,
        })
        .collect();
    assert_eq!(states.len(), 1, "one VaultFile source for `{source_path}`");
    states[0]
}

fn evidence_attribution<'a>(
    records: &'a [ProvenanceRecord],
    source_path: &str,
) -> &'a AttributionState {
    let states: Vec<_> = records
        .iter()
        .filter_map(|record| match &record.kind {
            ProvenanceRecordKind::Source(SourceRecord::Evidence(source))
                if source.source_path == source_path =>
            {
                Some(&source.attribution)
            }
            _ => None,
        })
        .collect();
    assert_eq!(states.len(), 1, "one Evidence source for `{source_path}`");
    states[0]
}

fn assert_declared_attribution(state: &AttributionState) {
    let AttributionState::Declared { declarations } = state else {
        panic!("expected declared attribution, got {state:?}");
    };
    assert_eq!(
        declarations
            .iter()
            .map(|declaration| (declaration.kind, declaration.source_key.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (AttributionKind::Author, "Author"),
            (AttributionKind::Author, "authors"),
            (AttributionKind::License, "Licenses"),
            (AttributionKind::License, "license"),
        ],
        "recognized mixed-case attribution keys retain their original spelling and canonical order"
    );
    assert_eq!(declarations[0].value, serde_json::json!("Ada Lovelace"));
    assert_eq!(declarations[1].value, serde_json::json!(["Grace Hopper"]));
    assert_eq!(
        declarations[2].value,
        serde_json::json!(["CC-BY-4.0", {"name":"House Terms", "url":"https://example.invalid/license"}])
    );
    assert_eq!(
        declarations[3].value,
        serde_json::json!({"family":"permissive", "identifier":"MIT"})
    );
}

#[test]
fn typed_record_and_edge_ids_match_literal_vectors() {
    let source_kind =
        ProvenanceRecordKind::Source(SourceRecord::BuildInput(BuildInputSourceRecord {
            input_kind: BuildInputKind::Toolchain,
            identity: "fixture-toolchain-v2".into(),
            content_hash: ContentHash::parse_hex(&"11".repeat(32)).expect("literal content hash"),
        }));
    let source_identity = serde_json::json!({
        "schema_version": 2,
        "kind": &source_kind,
    });
    assert_eq!(
        String::from_utf8(
            okc_core::canonical::to_canonical_json(&source_identity)
                .expect("canonical source identity payload"),
        )
        .expect("identity JSON is UTF-8"),
        concat!(
            "{\"kind\":{\"type\":\"source\",\"value\":{\"type\":\"build_input\",",
            "\"value\":{\"content_hash\":\"1111111111111111111111111111111111111111111111111111111111111111\",",
            "\"identity\":\"fixture-toolchain-v2\",\"input_kind\":\"toolchain\"}}},",
            "\"schema_version\":2}"
        )
    );
    let source = ProvenanceRecord::new(source_kind).expect("construct source record");
    assert_eq!(
        source.record_id.to_string(),
        "record_e3027c33323aa14efe97f6ec1f81fac73ba7631278d7e32531d8a737720b3124"
    );
    source.validate_identity().expect("literal source identity");
    assert_eq!(source.type_order(), 0);

    let edge_kind = ProvenanceRecordKind::Edge(EdgeRecord {
        relation: EdgeRelation::DerivedFrom,
        from: RecordId::from_str(&format!("record_{}", "22".repeat(32)))
            .expect("literal dependent record ID"),
        to: RecordId::from_str(&format!("record_{}", "33".repeat(32)))
            .expect("literal prerequisite record ID"),
        position: EdgePosition::Unordered,
    });
    let edge = ProvenanceRecord::new(edge_kind).expect("construct edge record");
    assert_eq!(
        edge.record_id.to_string(),
        "record_ef70194ad0fb33e2143daecf8efbdfa85b82bc7e3a4da5543cc2b6f3fb1b955a"
    );
    edge.validate_identity().expect("literal edge identity");
    assert_eq!(
        edge.type_order(),
        6,
        "edges use RecordId and the last type order"
    );
}

#[test]
fn public_typed_provenance_serde_rejects_unknown_enum_fields() {
    let record = ProvenanceRecord::new(ProvenanceRecordKind::Source(SourceRecord::BuildInput(
        BuildInputSourceRecord {
            input_kind: BuildInputKind::Toolchain,
            identity: "strict-public-serde".into(),
            content_hash: ContentHash::from_bytes(b"strict public serde"),
        },
    )))
    .expect("construct strict public serde fixture");
    for (pointer, field) in [
        ("/kind", "unknown_kind_field"),
        ("/kind/value", "unknown_source_field"),
    ] {
        let mut value = serde_json::to_value(&record).expect("encode typed record value");
        value
            .pointer_mut(pointer)
            .and_then(serde_json::Value::as_object_mut)
            .expect("typed enum object")
            .insert(field.into(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<ProvenanceRecord>(value).is_err(),
            "public serde must reject unknown field inserted at `{pointer}`"
        );
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one acceptance checks the stored graph's complete content, generated, attribution, and administrative closure"
)]
fn stored_graph_closes_copy_rewrite_generated_admin_and_attribution() {
    let (compiler, approved, proposal) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary typed provenance parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile typed provenance fixture");
    compiler
        .verify(&artifact)
        .expect("verify typed provenance fixture");

    let (jsonl, records) = read_stored_records(&artifact);
    okc_core::provenance::validate_stored_graph(&records).expect("validate stored typed graph");
    assert_strict_record_order(&records);
    assert_every_output_has_exact_producer(&records);

    let manifest: okc_core::compile::ArtifactManifest = serde_json::from_slice(
        &fs::read(artifact.join(".okc/manifest.json")).expect("read typed manifest"),
    )
    .expect("decode typed manifest");
    assert_eq!(manifest.provenance_schema_version, 2);
    assert_eq!(
        manifest.provenance_graph_hash,
        okc_core::provenance::stored_graph_hash(&jsonl)
    );
    let expected_stored_outputs: BTreeSet<_> = manifest
        .files
        .iter()
        .filter(|file| file.path != ".okc/provenance.jsonl")
        .map(|file| file.path.as_str())
        .collect();
    let actual_stored_outputs: BTreeSet<_> = records.iter().filter_map(output_path).collect();
    assert_eq!(actual_stored_outputs, expected_stored_outputs);
    assert!(records.iter().all(|record| match &record.kind {
        ProvenanceRecordKind::Output(output) => output.storage == OutputStorage::Stored,
        _ => true,
    }));

    assert_declared_attribution(vault_file_attribution(&records, "Declared.md"));
    assert_eq!(
        vault_file_attribution(&records, "NotDeclared.md"),
        &AttributionState::NotDeclared
    );
    assert!(matches!(
        vault_file_attribution(&records, "Opaque.md"),
        AttributionState::Opaque { .. }
    ));
    for source_path in ["Board.canvas", "View.base", "assets/evidence.bin"] {
        assert_eq!(
            vault_file_attribution(&records, source_path),
            &AttributionState::NotApplicable
        );
    }
    assert_declared_attribution(evidence_attribution(&records, "Declared.md"));
    assert_eq!(
        evidence_attribution(&records, "NotDeclared.md"),
        &AttributionState::NotDeclared
    );
    assert!(matches!(
        evidence_attribution(&records, "Opaque.md"),
        AttributionState::Opaque { .. }
    ));

    let ids = records_by_id(&records);
    let proposal_record = records
        .iter()
        .find(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Proposal(value)
                    if value.proposal_id == proposal.proposal_id
            )
        })
        .expect("typed proposal record");
    let expected_evidence_ids = proposal
        .evidence
        .iter()
        .map(EvidenceId::from_evidence)
        .collect::<Result<Vec<_>, _>>()
        .expect("derive fixture EvidenceIds");
    let ProvenanceRecordKind::Proposal(proposal_value) = &proposal_record.kind else {
        unreachable!();
    };
    assert_eq!(proposal_value.evidence_ids, expected_evidence_ids);
    let mut support_edges: Vec<_> = outgoing(&records, proposal_record.record_id)
        .into_iter()
        .filter(|edge| edge.relation == EdgeRelation::SupportedBy)
        .collect();
    support_edges.sort_by_key(|edge| match edge.position {
        EdgePosition::Ordered { index } => index,
        EdgePosition::Unordered => panic!("proposal evidence must be ordered"),
    });
    assert_eq!(support_edges.len(), expected_evidence_ids.len());
    for (index, edge) in support_edges.iter().enumerate() {
        assert_eq!(
            edge.position,
            EdgePosition::Ordered {
                index: index as u64
            }
        );
        let ProvenanceRecordKind::Source(SourceRecord::Evidence(evidence)) = ids[&edge.to] else {
            panic!("SupportedBy must target typed evidence");
        };
        assert_eq!(evidence.evidence_id, expected_evidence_ids[index]);
    }

    let generated = records
        .iter()
        .find(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Operation(OperationRecord::Generate { destination, .. })
                    if destination == GENERATED_PATH
            )
        })
        .expect("generated operation record");
    let generated_edges = outgoing(&records, generated.record_id);
    assert_eq!(
        generated_edges
            .iter()
            .filter(|edge| edge.relation == EdgeRelation::DerivedFrom)
            .map(|edge| edge.to)
            .collect::<Vec<_>>(),
        vec![proposal_record.record_id]
    );
    let approvals: Vec<_> = generated_edges
        .iter()
        .filter(|edge| edge.relation == EdgeRelation::ApprovedBy)
        .collect();
    assert_eq!(approvals.len(), 1);
    assert!(matches!(
        ids[&approvals[0].to],
        ProvenanceRecordKind::Approval(_)
    ));

    for (path, role) in [
        (".okc/plan.json", OutputRole::AuditPlan),
        (".okc/conflicts.json", OutputRole::AuditConflicts),
        (".okc/diagnostics.json", OutputRole::AuditDiagnostics),
        (".okc/ai-transcript.jsonl", OutputRole::AuditTranscript),
    ] {
        let output = records
            .iter()
            .find(|record| output_path(record) == Some(path))
            .expect("stored administrative output");
        let ProvenanceRecordKind::Output(output_value) = &output.kind else {
            unreachable!();
        };
        assert_eq!(output_value.role, role);
        assert_eq!(output_value.storage, OutputStorage::Stored);
        assert!(matches!(
            ids[&output_value.producing_operation],
            ProvenanceRecordKind::Operation(OperationRecord::SerializeAudit {
                role: operation_role,
                ..
            }) if *operation_role == role
        ));
    }
    assert!(records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Operation(OperationRecord::Copy { .. })
    )));
    assert!(records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Operation(OperationRecord::RewriteMarkdown { .. })
    )));
    assert!(records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Operation(OperationRecord::RewriteCanvas { .. })
    )));
}

#[test]
fn dedup_graph_reaches_every_exact_note_and_asset_member() {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([
            common::fixture_source("typed-alpha", "typed_dedup_alpha"),
            common::fixture_source("typed-beta", "typed_dedup_beta"),
        ])
        .expect("inspect typed dedup fixtures");
    let plan = compiler
        .plan(&inspection)
        .expect("plan typed dedup fixtures");
    let approved = compiler
        .approve_without_augmentation(plan)
        .expect("approve typed dedup fixtures");
    let temporary = tempfile::tempdir().expect("temporary typed dedup parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile typed dedup fixtures");
    compiler
        .verify(&artifact)
        .expect("verify typed dedup graph");
    let (_, records) = read_stored_records(&artifact);
    assert_every_output_has_exact_producer(&records);

    let deduplications: Vec<_> = records
        .iter()
        .filter(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Operation(OperationRecord::Deduplicate {
                    member_count: 2,
                    ..
                })
            )
        })
        .collect();
    assert_eq!(
        deduplications.len(),
        2,
        "one exact note group and one exact attachment group"
    );
    let ids = records_by_id(&records);
    let mut reached_source_paths = BTreeSet::new();
    for operation in deduplications {
        let member_edges: Vec<_> = outgoing(&records, operation.record_id)
            .into_iter()
            .filter(|edge| edge.relation == EdgeRelation::Deduplicates)
            .collect();
        assert_eq!(member_edges.len(), 2);
        let source_ids: BTreeSet<_> = member_edges
            .iter()
            .map(|edge| {
                let ProvenanceRecordKind::Source(SourceRecord::VaultFile(source)) = ids[&edge.to]
                else {
                    panic!("Deduplicates must target VaultFile sources");
                };
                reached_source_paths.insert(source.source_path.clone());
                match source.source_path.as_str() {
                    "Duplicate.md" | "Elsewhere.md" => {
                        assert!(matches!(
                            source.attribution,
                            AttributionState::Declared { .. }
                        ));
                    }
                    "assets/shared.bin" | "media/renamed.bin" => {
                        assert_eq!(source.attribution, AttributionState::NotApplicable);
                    }
                    path => panic!("unexpected exact member `{path}`"),
                }
                source.source_id.to_string()
            })
            .collect();
        assert_eq!(
            source_ids,
            BTreeSet::from(["typed-alpha".to_owned(), "typed-beta".to_owned()]),
            "dedup closure retains both source Vault identities"
        );
    }
    assert_eq!(
        reached_source_paths,
        BTreeSet::from([
            "Duplicate.md".to_owned(),
            "Elsewhere.md".to_owned(),
            "assets/shared.bin".to_owned(),
            "media/renamed.bin".to_owned(),
        ])
    );
}

#[test]
fn stored_graph_schema_order_ids_and_legacy_forms_fail_closed_after_reseal() {
    let (compiler, approved, _) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary strict graph parent");
    let baseline = temporary.path().join("baseline");
    compiler
        .compile(&approved, &baseline)
        .expect("compile strict graph baseline");
    let original =
        fs::read(baseline.join(".okc/provenance.jsonl")).expect("read strict graph baseline");
    let first_lf = original
        .iter()
        .position(|byte| *byte == b'\n')
        .expect("stored graph has canonical lines");
    let first_line = &original[..first_lf];
    let remaining = &original[first_lf + 1..];
    let first_value: serde_json::Value =
        serde_json::from_slice(first_line).expect("decode first typed record as JSON");

    let mutate_first = |mutate: fn(&mut serde_json::Map<String, serde_json::Value>)| {
        let mut value = first_value.clone();
        mutate(value.as_object_mut().expect("typed record envelope object"));
        let mut bytes = okc_core::canonical::to_canonical_json(&value)
            .expect("canonicalize deliberately malformed envelope");
        bytes.push(b'\n');
        bytes.extend_from_slice(remaining);
        bytes
    };
    let unknown = mutate_first(|object| {
        object.insert("unexpected".into(), serde_json::json!(true));
    });
    let missing_schema = mutate_first(|object| {
        object.remove("schema_version");
    });
    let stale_id = mutate_first(|object| {
        object.insert(
            "record_id".into(),
            serde_json::json!(format!("record_{}", "00".repeat(32))),
        );
    });
    let mut missing_final_lf = original.clone();
    assert_eq!(missing_final_lf.pop(), Some(b'\n'));
    let mut crlf = original.clone();
    crlf.splice(first_lf..=first_lf, *b"\r\n");
    let mut blank_line = Vec::new();
    blank_line.extend_from_slice(first_line);
    blank_line.extend_from_slice(b"\n\n");
    blank_line.extend_from_slice(remaining);
    let mut noncanonical =
        serde_json::to_vec_pretty(&first_value).expect("pretty-print noncanonical typed record");
    noncanonical.push(b'\n');
    noncanonical.extend_from_slice(remaining);
    let mut duplicate_key = format!(
        "{{\"schema_version\":2,{}",
        &String::from_utf8_lossy(first_line)[1..]
    )
    .into_bytes();
    duplicate_key.push(b'\n');
    duplicate_key.extend_from_slice(remaining);
    let mut lines: Vec<Vec<u8>> = original
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(<[u8]>::to_vec)
        .collect();
    lines.swap(0, 1);
    let mut reversed = Vec::new();
    for line in &lines {
        reversed.extend_from_slice(line);
        reversed.push(b'\n');
    }
    let mut duplicate_record = original.clone();
    duplicate_record.extend_from_slice(first_line);
    duplicate_record.push(b'\n');
    let legacy = format!(
        "{{\"type\":\"output\",\"output_path\":\"knowledge/legacy.md\",\"output_hash\":\"{}\"}}\n",
        "00".repeat(32)
    )
    .into_bytes();

    for (name, bytes) in [
        ("unknown-envelope-field", unknown),
        ("missing-schema-version", missing_schema),
        ("stale-record-id", stale_id),
        ("missing-final-lf", missing_final_lf),
        ("crlf", crlf),
        ("blank-line", blank_line),
        ("noncanonical-json", noncanonical),
        ("duplicate-object-key", duplicate_key),
        ("out-of-order-records", reversed),
        ("duplicate-record-id", duplicate_record),
        ("legacy-flat-ledger", legacy),
    ] {
        let artifact = temporary.path().join(name);
        common::copy_tree(&baseline, &artifact);
        reseal_graph_bytes(&artifact, &bytes);
        assert_graph_rejected(&compiler, &artifact, name);
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one table-driven adversarial test covers structurally valid and invalid fully resealed graph attacks"
)]
fn verifier_rejects_fully_resealed_typed_graph_semantic_attacks() {
    let (compiler, approved, _) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary semantic graph parent");
    let baseline = temporary.path().join("baseline");
    compiler
        .compile(&approved, &baseline)
        .expect("compile semantic graph baseline");
    let (_, original) = read_stored_records(&baseline);

    let artifact = temporary.path().join("attribution-erasure");
    common::copy_tree(&baseline, &artifact);
    let mut attribution_erasure = original.clone();
    let source_index = attribution_erasure
        .iter()
        .position(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Source(SourceRecord::VaultFile(source))
                    if source.source_path == "Declared.md"
            )
        })
        .expect("declared VaultFile source");
    let ProvenanceRecordKind::Source(SourceRecord::VaultFile(mut source)) =
        attribution_erasure[source_index].kind.clone()
    else {
        unreachable!();
    };
    source.attribution = AttributionState::NotDeclared;
    replace_node_and_references(
        &mut attribution_erasure,
        source_index,
        ProvenanceRecordKind::Source(SourceRecord::VaultFile(source)),
    );
    sort_records(&mut attribution_erasure);
    okc_core::provenance::validate_stored_graph(&attribution_erasure)
        .expect("attribution erasure remains structurally valid");
    reseal_records(&artifact, &mut attribution_erasure);
    assert_graph_rejected(&compiler, &artifact, "attribution-erasure");

    let artifact = temporary.path().join("swapped-copy-sources");
    common::copy_tree(&baseline, &artifact);
    let mut swapped = original.clone();
    let copy_ids: BTreeSet<_> = swapped
        .iter()
        .filter_map(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Operation(OperationRecord::Copy { .. })
            )
            .then_some(record.record_id)
        })
        .collect();
    let mut source_edge_indices: Vec<_> = swapped
        .iter()
        .enumerate()
        .filter_map(|(index, record)| match &record.kind {
            ProvenanceRecordKind::Edge(edge)
                if copy_ids.contains(&edge.from) && edge.relation == EdgeRelation::DerivedFrom =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect();
    source_edge_indices.sort_unstable();
    let [first_index, second_index, ..] = source_edge_indices.as_slice() else {
        panic!("fixture needs at least two Copy source bindings");
    };
    let ProvenanceRecordKind::Edge(mut first) = swapped[*first_index].kind.clone() else {
        unreachable!();
    };
    let ProvenanceRecordKind::Edge(mut second) = swapped[*second_index].kind.clone() else {
        unreachable!();
    };
    assert_ne!(first.to, second.to);
    std::mem::swap(&mut first.to, &mut second.to);
    swapped[*first_index] = ProvenanceRecord::new(ProvenanceRecordKind::Edge(first))
        .expect("re-identify first swapped binding");
    swapped[*second_index] = ProvenanceRecord::new(ProvenanceRecordKind::Edge(second))
        .expect("re-identify second swapped binding");
    sort_records(&mut swapped);
    okc_core::provenance::validate_stored_graph(&swapped)
        .expect("source substitution remains structurally valid");
    reseal_records(&artifact, &mut swapped);
    assert_graph_rejected(&compiler, &artifact, "swapped-copy-sources");

    for attack in ["missing-producer", "dangling-endpoint", "derivation-cycle"] {
        let artifact = temporary.path().join(attack);
        common::copy_tree(&baseline, &artifact);
        let mut forged = original.clone();
        match attack {
            "missing-producer" => {
                let (output_id, producer_id) = forged
                    .iter()
                    .find_map(|record| match &record.kind {
                        ProvenanceRecordKind::Output(output) => {
                            Some((record.record_id, output.producing_operation))
                        }
                        _ => None,
                    })
                    .expect("stored output producer");
                forged.retain(|record| {
                    !matches!(
                        &record.kind,
                        ProvenanceRecordKind::Edge(edge)
                            if edge.relation == EdgeRelation::DerivedFrom
                                && edge.from == output_id
                                && edge.to == producer_id
                    )
                });
            }
            "dangling-endpoint" => {
                let output_id = forged
                    .iter()
                    .find(|record| matches!(&record.kind, ProvenanceRecordKind::Output(_)))
                    .expect("stored output")
                    .record_id;
                forged.push(
                    ProvenanceRecord::new(ProvenanceRecordKind::Edge(EdgeRecord {
                        relation: EdgeRelation::DerivedFrom,
                        from: output_id,
                        to: RecordId::from_str(&format!("record_{}", "ff".repeat(32)))
                            .expect("literal dangling RecordId"),
                        position: EdgePosition::Unordered,
                    }))
                    .expect("construct dangling edge"),
                );
            }
            "derivation-cycle" => {
                let (output_id, operation_id) = forged
                    .iter()
                    .find_map(|record| match &record.kind {
                        ProvenanceRecordKind::Output(output)
                            if output.role == OutputRole::AuditPlan =>
                        {
                            Some((record.record_id, output.producing_operation))
                        }
                        _ => None,
                    })
                    .expect("audit output and SerializeAudit producer");
                forged.push(
                    ProvenanceRecord::new(ProvenanceRecordKind::Edge(EdgeRecord {
                        relation: EdgeRelation::DerivedFrom,
                        from: operation_id,
                        to: output_id,
                        position: EdgePosition::Unordered,
                    }))
                    .expect("construct derivation cycle"),
                );
            }
            _ => unreachable!(),
        }
        reseal_records(&artifact, &mut forged);
        assert_graph_rejected(&compiler, &artifact, attack);
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one acceptance follows the same explanation through directory, pack, cursor, and virtual-envelope boundaries"
)]
fn virtual_envelope_pagination_cursor_and_directory_pack_queries_are_identical() {
    let (compiler, approved, _) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary provenance query parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile provenance query fixture");
    let pack = temporary.path().join("compiled.okcpack");
    okc_core::pack::create_pack(&artifact, &pack, compiler.policy().output.zstd_level)
        .expect("create provenance query OKCPack");
    let (_, stored) = read_stored_records(&artifact);
    let stored_ids: BTreeSet<_> = stored.iter().map(|record| record.record_id).collect();

    for (path, role) in [
        (".okc/provenance.jsonl", OutputRole::Provenance),
        (".okc/manifest.json", OutputRole::Manifest),
        (".okc/checksums.txt", OutputRole::Checksums),
    ] {
        let directory = compiler
            .explain_provenance(&artifact, path)
            .expect("explain virtual directory envelope");
        let packed = compiler
            .explain_provenance(&pack, path)
            .expect("explain virtual packed envelope");
        assert_eq!(directory, packed, "directory/pack parity for `{path}`");
        assert_eq!(
            okc_core::canonical::to_canonical_json(&directory)
                .expect("canonical directory explanation"),
            okc_core::canonical::to_canonical_json(&packed).expect("canonical packed explanation")
        );
        let roots: Vec<_> = directory
            .records
            .iter()
            .filter_map(|record| match &record.kind {
                ProvenanceRecordKind::Output(output)
                    if output.subject
                        == (ProvenanceSubject::ArtifactPath {
                            path: path.to_owned(),
                        }) =>
                {
                    Some((record, output))
                }
                _ => None,
            })
            .collect();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].1.role, role);
        assert_eq!(roots[0].1.storage, OutputStorage::VirtualAuditEnvelope);
        assert!(
            !stored_ids.contains(&roots[0].0.record_id),
            "virtual envelope output must not self-occur in stored JSONL"
        );
    }
    let stored_plan = compiler
        .explain_provenance(&artifact, ".okc/plan.json")
        .expect("explain stored audit plan");
    let plan_root = stored_plan
        .records
        .iter()
        .find(|record| {
            matches!(
                &record.kind,
                ProvenanceRecordKind::Output(output)
                    if output.subject == (ProvenanceSubject::ArtifactPath {
                        path: ".okc/plan.json".into(),
                    })
            )
        })
        .expect("stored audit plan output");
    let ProvenanceRecordKind::Output(plan_output) = &plan_root.kind else {
        unreachable!();
    };
    assert_eq!(plan_output.storage, OutputStorage::Stored);
    assert!(stored_ids.contains(&plan_root.record_id));

    let full_directory = compiler
        .explain_provenance(&artifact, GENERATED_PATH)
        .expect("complete generated directory explanation");
    let full_pack = compiler
        .explain_provenance(&pack, GENERATED_PATH)
        .expect("complete generated pack explanation");
    assert_eq!(full_directory, full_pack);
    let mut cursor = None;
    let mut paged = Vec::new();
    loop {
        let mut query = ProvenanceQuery::artifact_path(GENERATED_PATH)
            .with_limit(1)
            .expect("one-record page");
        if let Some(value) = &cursor {
            query = query.with_cursor(value);
        }
        let directory_page = compiler
            .explain_provenance_page(&artifact, &query)
            .expect("next directory provenance page");
        let packed_page = compiler
            .explain_provenance_page(&pack, &query)
            .expect("next packed provenance page");
        assert_eq!(directory_page, packed_page);
        assert_eq!(directory_page.graph_hash, full_directory.graph_hash);
        assert_eq!(directory_page.records.len(), 1);
        paged.extend(directory_page.records.clone());
        if directory_page.complete {
            assert!(directory_page.next_cursor.is_none());
            break;
        }
        cursor = directory_page.next_cursor;
    }
    assert_eq!(paged, full_directory.records);

    let first = compiler
        .explain_provenance_page(
            &artifact,
            &ProvenanceQuery::artifact_path(GENERATED_PATH)
                .with_limit(1)
                .expect("first cursor page"),
        )
        .expect("first cursor page");
    let valid_cursor = first.next_cursor.expect("multi-record explanation cursor");
    let mut tampered_cursor = valid_cursor.clone();
    let replacement = if tampered_cursor.ends_with('0') {
        '1'
    } else {
        '0'
    };
    tampered_cursor.pop();
    tampered_cursor.push(replacement);
    let (other_compiler, other_approved, _) =
        approved_generated_fixture_for("typed-provenance-other-artifact");
    let other_artifact = temporary.path().join("other-compiled");
    other_compiler
        .compile(&other_approved, &other_artifact)
        .expect("compile distinct cursor-binding artifact");
    for query in [
        ProvenanceQuery::artifact_path(GENERATED_PATH).with_cursor("malformed"),
        ProvenanceQuery::artifact_path(GENERATED_PATH).with_cursor(tampered_cursor),
        ProvenanceQuery::artifact_path(".okc/plan.json").with_cursor(valid_cursor.clone()),
    ] {
        assert!(matches!(
            compiler.explain_provenance_page(&artifact, &query),
            Err(OkcError::VerificationFailed(_))
        ));
    }
    assert!(matches!(
        other_compiler.explain_provenance_page(
            &other_artifact,
            &ProvenanceQuery::artifact_path(GENERATED_PATH).with_cursor(valid_cursor),
        ),
        Err(OkcError::VerificationFailed(_))
    ));
    assert!(matches!(
        ProvenanceQuery::artifact_path(GENERATED_PATH).with_limit(0),
        Err(OkcError::InvalidConfig(_))
    ));
    assert!(matches!(
        ProvenanceQuery::artifact_path(GENERATED_PATH).with_max_bytes(0),
        Err(OkcError::InvalidConfig(_))
    ));
    let tiny = ProvenanceQuery::artifact_path(GENERATED_PATH)
        .with_max_bytes(1)
        .expect("one-byte query is within the public numeric bound");
    assert!(matches!(
        compiler.explain_provenance_page(&artifact, &tiny),
        Err(OkcError::ResourceLimit(_))
    ));

    assert!(matches!(
        compiler.explain_provenance_page(&artifact, &ProvenanceQuery::package()),
        Err(OkcError::VerificationFailed(_))
    ));
    let package = compiler
        .explain_provenance_page(&pack, &ProvenanceQuery::package())
        .expect("explain explicit package subject");
    assert!(package.complete);
    assert_eq!(package.subject, ProvenanceSubject::Package);
    assert!(package.records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Output(output)
            if output.subject == ProvenanceSubject::Package
                && output.role == OutputRole::Package
                && output.storage == OutputStorage::VirtualPackage
    )));
    assert!(package.records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Operation(OperationRecord::Package { .. })
    )));
    assert!(package.records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Edge(edge) if edge.relation == EdgeRelation::PackagedAs
    )));
    assert!(package.records.iter().any(|record| matches!(
        &record.kind,
        ProvenanceRecordKind::Source(SourceRecord::BuildInput(source))
            if source.input_kind == BuildInputKind::InnerArtifact
    )));
    let mut package_cursor = None;
    let mut paged_package = Vec::new();
    loop {
        let mut query = ProvenanceQuery::package()
            .with_limit(1)
            .expect("one-record package page");
        if let Some(value) = &package_cursor {
            query = query.with_cursor(value);
        }
        let page = compiler
            .explain_provenance_page(&pack, &query)
            .expect("next package page");
        paged_package.extend(page.records);
        if page.complete {
            assert!(page.next_cursor.is_none());
            break;
        }
        package_cursor = page.next_cursor;
    }
    assert_eq!(paged_package, package.records);
}

#[test]
fn page_max_bytes_bounds_the_complete_serialized_response() {
    let (compiler, approved, _) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary page byte-bound parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile page byte-bound fixture");
    let one_record = compiler
        .explain_provenance_page(
            &artifact,
            &ProvenanceQuery::artifact_path(GENERATED_PATH)
                .with_limit(1)
                .expect("one-record reference page"),
        )
        .expect("construct one-record reference page");
    assert!(!one_record.complete);
    let exact_bytes = one_record
        .canonical_json_bytes()
        .expect("serialize complete provenance response");
    assert_eq!(
        exact_bytes,
        okc_core::canonical::to_canonical_json(&one_record)
            .expect("independently serialize complete provenance response")
    );
    let exact_bound = one_record
        .canonical_json_len()
        .expect("measure complete provenance response");
    let bounded_query = ProvenanceQuery::artifact_path(GENERATED_PATH)
        .with_max_bytes(exact_bound)
        .expect("reference response is within the public maximum");
    let bounded = compiler
        .explain_provenance_page(&artifact, &bounded_query)
        .expect("page fits its exact serialized response bound");
    assert!(
        bounded
            .canonical_json_len()
            .expect("serialize bounded provenance response")
            <= exact_bound,
        "max_bytes must cover the whole returned page envelope, records, and cursor"
    );

    let too_small = ProvenanceQuery::artifact_path(GENERATED_PATH)
        .with_max_bytes(exact_bound - 1)
        .expect("one-byte-smaller bound is numerically valid");
    assert!(matches!(
        compiler.explain_provenance_page(&artifact, &too_small),
        Err(OkcError::ResourceLimit(_))
    ));
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one acceptance compares identical node-scoped decision binding for Markdown and Canvas"
)]
fn markdown_and_canvas_waivers_bind_distinct_typed_subjects() {
    for (source_id, fixture, expected_subject) in [
        (
            "typed-markdown-ambiguity",
            "typed_markdown_ambiguity",
            "markdown",
        ),
        ("typed-canvas-ambiguity", "canvas_ambiguity", "canvas"),
    ] {
        let compiler = common::compiler();
        let inspection = compiler
            .inspect([common::fixture_source(source_id, fixture)])
            .expect("inspect typed ambiguity fixture");
        let plan = compiler
            .plan(&inspection)
            .expect("plan typed ambiguity fixture");
        let conflicts: Vec<_> = plan
            .conflicts
            .iter()
            .filter(|conflict| conflict.kind == ConflictKind::LinkAmbiguity && conflict.required)
            .collect();
        assert_eq!(
            conflicts.len(),
            2,
            "fixture has two node-scoped ambiguities"
        );
        let mut subject_ids = std::collections::BTreeSet::new();
        for conflict in &conflicts {
            match (&conflict.subject, expected_subject) {
                (
                    Some(ConflictSubject::MarkdownLink {
                        document_id,
                        link_id,
                        raw_target,
                    }),
                    "markdown",
                ) => {
                    assert_eq!(raw_target, "Topic");
                    subject_ids.insert(format!("{document_id}/{link_id}"));
                }
                (
                    Some(ConflictSubject::CanvasReference {
                        canvas_id,
                        node_id,
                        raw_path,
                    }),
                    "canvas",
                ) => {
                    assert_eq!(raw_path, "Topic.md");
                    subject_ids.insert(format!("{canvas_id}/{node_id}"));
                }
                (subject, _) => panic!("unexpected typed conflict subject: {subject:?}"),
            }
        }
        assert_eq!(
            subject_ids.len(),
            2,
            "each ambiguity has a distinct subject"
        );

        let expected_subjects: Vec<_> = conflicts
            .iter()
            .map(|conflict| conflict.subject.clone().expect("typed conflict subject"))
            .collect();
        let decisions = conflicts
            .iter()
            .map(|conflict| DecisionOverlay {
                plan_id: plan.plan_id.to_string(),
                conflict_id: conflict.conflict_id.clone(),
                conflict_content_hash: conflict.content_hash,
                action: ConflictAction::WaivePreserveOriginal,
                decided_by: CuratorId::new("qa-typed-provenance").expect("valid curator ID"),
                policy_version: "test-v2".into(),
                rationale: Some("retain the ambiguous source representation".into()),
            })
            .collect();
        let approved = compiler
            .approve_with_conflicts(
                plan,
                ValidatedProposals::default(),
                ApprovalLog::default(),
                DecisionOverlayLog { decisions },
            )
            .expect("approve both exact typed waivers");
        assert_eq!(approved.conflict_decisions.len(), 2);

        let temporary = tempfile::tempdir().expect("temporary typed decision parent");
        let artifact = temporary.path().join("compiled");
        compiler
            .compile(&approved, &artifact)
            .expect("compile typed waiver artifact");
        compiler
            .verify(&artifact)
            .expect("verify typed waiver artifact");
        let (_, records) = read_stored_records(&artifact);
        let decision_records: Vec<_> = records
            .iter()
            .filter_map(|record| match &record.kind {
                ProvenanceRecordKind::Decision(decision) => Some((record.record_id, decision)),
                _ => None,
            })
            .collect();
        assert_eq!(decision_records.len(), 2);
        for subject in &expected_subjects {
            assert_eq!(
                decision_records
                    .iter()
                    .filter(|(_, decision)| &decision.subject == subject)
                    .count(),
                1,
                "each node-scoped waiver has exactly one typed Decision record"
            );
        }
        let decision_ids: BTreeSet<_> = decision_records.iter().map(|(id, _)| *id).collect();
        let bound_operations: Vec<_> = records
            .iter()
            .filter_map(|record| match &record.kind {
                ProvenanceRecordKind::Operation(_) => {
                    let bound: BTreeSet<_> = outgoing(&records, record.record_id)
                        .into_iter()
                        .filter(|edge| edge.relation == EdgeRelation::DecidedBy)
                        .map(|edge| edge.to)
                        .collect();
                    (!bound.is_empty()).then_some(bound)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            bound_operations,
            vec![decision_ids],
            "one content operation binds both exact node decisions without cross-subject collapse"
        );
    }
}
