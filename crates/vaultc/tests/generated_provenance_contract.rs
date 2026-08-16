mod common;

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde::Serialize;
use vaultc::approval::ProposalMaterialization;
use vaultc::compile::ArtifactManifest;
use vaultc::identity::{BlockId, ContentHash, DocumentId, EvidenceId, OperationId, SnapshotId};
use vaultc::provenance::{ProvenanceRecord, ProvenanceSource};
use vaultc::{ApprovalDecision, ApprovalLog, ApprovedPlan, DraftPlan, VaultCompiler, VaultcError};
use vaultc_protocol::{
    EvidenceRefWire, KnowledgeProposal, PROPOSAL_SCHEMA_VERSION, ProposalKind, ProviderIdentity,
};

const GENERATED_PATH: &str = "knowledge/_generated/generated-provenance.md";
const PROPOSAL_ID: &str = "proposal-generated-provenance-closure";

fn evidence_for(document: &vaultc::ir::Document) -> EvidenceRefWire {
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
    let mut documents: Vec<_> = plan.workspace.documents.values().collect();
    documents.sort_by(|left, right| {
        right
            .source_file
            .logical_path
            .cmp(&left.source_file.logical_path)
    });
    assert!(
        documents.len() >= 2,
        "fixture provides two evidence documents"
    );
    let mut evidence: Vec<_> = documents.into_iter().take(2).map(evidence_for).collect();
    evidence.sort_by_key(independent_evidence_id);
    evidence.reverse();
    assert!(
        independent_evidence_id(&evidence[0]) > independent_evidence_id(&evidence[1]),
        "fixture deliberately distinguishes proposal order from sorted EvidenceId order"
    );
    KnowledgeProposal {
        schema_version: PROPOSAL_SCHEMA_VERSION,
        proposal_id: PROPOSAL_ID.into(),
        plan_id: plan.plan_id.to_string(),
        projection_hash: plan.projection_hash.hex(),
        provider: ProviderIdentity {
            provider: "local-fixture".into(),
            model: "deterministic-test-double".into(),
            version: Some("1".into()),
        },
        kind: ProposalKind::CreateGeneratedNote {
            title: "Generated provenance closure".into(),
            markdown_body:
                "# Generated provenance closure\n\nEvidence-bound synthesis from two documents.\n"
                    .into(),
            suggested_path: Some("generated-provenance.md".into()),
        },
        evidence,
        uncertainty: Some(0.1),
        rationale: Some("generated provenance acceptance fixture".into()),
    }
}

fn approved_generated_fixture() -> (VaultCompiler, ApprovedPlan, KnowledgeProposal) {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source(
            "generated-provenance",
            "basic_vault",
        )])
        .expect("inspect generated provenance fixture");
    let plan = compiler
        .plan(&inspection)
        .expect("plan generated provenance fixture");
    let proposal = generated_proposal(&plan);
    let validated = compiler
        .validate_proposals(&plan, vec![proposal.clone()])
        .expect("validate generated provenance proposal");
    let proposal_content_hash = validated
        .valid()
        .next()
        .expect("generated provenance proposal is valid")
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
                    approver: "qa-generated-provenance".into(),
                    policy_version: "test-v1".into(),
                }],
            },
        )
        .expect("approve exact generated provenance proposal");
    (compiler, approved, proposal)
}

fn generated_frontmatter(note: &str) -> (&str, serde_yaml_ng::Value) {
    let yaml = note
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(yaml, _)| yaml)
        .expect("generated note has canonical frontmatter delimiters");
    let parsed = serde_yaml_ng::from_str(yaml).expect("generated frontmatter is valid YAML");
    (yaml, parsed)
}

fn independent_evidence_id(evidence: &EvidenceRefWire) -> String {
    use sha2::{Digest as _, Sha256};

    let snapshot_id = evidence
        .snapshot_id
        .parse::<SnapshotId>()
        .expect("fixture snapshot ID");
    let document_id = evidence
        .document_id
        .parse::<DocumentId>()
        .expect("fixture document ID");
    let block_id = evidence
        .block_id
        .as_deref()
        .map(str::parse::<BlockId>)
        .transpose()
        .expect("fixture block ID");
    let content_hash =
        ContentHash::parse_hex(&evidence.content_hash).expect("fixture evidence content hash");
    let mode = match (block_id, evidence.byte_start, evidence.byte_end) {
        (None, None, None) => 0_u8,
        (Some(_), None, None) => 1_u8,
        (Some(_), Some(_), Some(_)) => 2_u8,
        _ => panic!("fixture evidence must use a valid canonical mode"),
    };
    let mut hasher = Sha256::new();
    hasher.update(b"vaultc:evidence:v1\0");
    hasher.update(snapshot_id.hash().as_bytes());
    hasher.update(document_id.hash().as_bytes());
    hasher.update([mode]);
    if let Some(block_id) = block_id {
        hasher.update(block_id.hash().as_bytes());
    }
    if let (Some(start), Some(end)) = (evidence.byte_start, evidence.byte_end) {
        hasher.update(start.to_be_bytes());
        hasher.update(end.to_be_bytes());
    }
    hasher.update(content_hash.as_bytes());
    format!("evidence_{}", hex::encode(hasher.finalize()))
}

fn assert_evidence_id_golden_vectors() {
    let file_level = EvidenceRefWire {
        snapshot_id: format!("snap_{}", "11".repeat(32)),
        document_id: format!("doc_{}", "22".repeat(32)),
        block_id: None,
        byte_start: None,
        byte_end: None,
        content_hash: "33".repeat(32),
    };
    let expected = "evidence_ca746ceb4dbbe4c963a82668cc4aa11852a41d84dcea24e8aa36407b52993bbe";
    assert_eq!(independent_evidence_id(&file_level), expected);
    assert_eq!(
        EvidenceId::from_evidence(&file_level)
            .expect("production file-level EvidenceId")
            .to_string(),
        expected,
        "production identity must match the literal fixed-width golden vector"
    );

    let block_without_span = EvidenceRefWire {
        block_id: Some(format!("block_{}", "44".repeat(32))),
        ..file_level.clone()
    };
    let block_with_span = EvidenceRefWire {
        byte_start: Some(7),
        byte_end: Some(23),
        ..block_without_span.clone()
    };
    let without_span = independent_evidence_id(&block_without_span);
    let with_span = independent_evidence_id(&block_with_span);
    assert_ne!(
        without_span, with_span,
        "mode 1 block evidence and mode 2 exact-span evidence must have distinct identities"
    );
    for (evidence, independently_derived) in [
        (block_without_span, without_span),
        (block_with_span, with_span),
    ] {
        assert_eq!(
            EvidenceId::from_evidence(&evidence)
                .expect("production block EvidenceId")
                .to_string(),
            independently_derived,
            "production block identity must match the independent tagged-byte formula"
        );
    }
}

#[derive(Serialize)]
struct ExpectedGeneratedOperationIdentity<'a> {
    plan_id: &'a str,
    proposal_id: &'a str,
    proposal_content_hash: ContentHash,
    destination: &'a str,
    body_hash: ContentHash,
    expected_output_hash: ContentHash,
    evidence_ids: &'a [EvidenceId],
}

fn assert_generated_materialization(
    approved: &ApprovedPlan,
    proposal: &KnowledgeProposal,
    generated_bytes: &[u8],
) {
    let approved_proposal = approved
        .approved_proposals
        .iter()
        .find(|approved| approved.proposal.proposal_id == proposal.proposal_id)
        .expect("approved generated proposal");
    let ProposalMaterialization::GeneratedNote {
        destination,
        body_hash,
        expected_output_hash,
        evidence_ids,
        operation_id,
    } = &approved_proposal.materialization
    else {
        panic!("generated proposal has a generated-note materialization");
    };
    let expected_evidence_ids: Vec<_> = proposal
        .evidence
        .iter()
        .map(|evidence| EvidenceId::from_evidence(evidence).expect("approved EvidenceId"))
        .collect();
    assert_eq!(destination, GENERATED_PATH);
    assert_eq!(evidence_ids, &expected_evidence_ids);
    assert_eq!(
        evidence_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        proposal
            .evidence
            .iter()
            .map(independent_evidence_id)
            .collect::<Vec<_>>(),
        "materialization must preserve the exact independently derived EvidenceId order"
    );

    let ProposalKind::CreateGeneratedNote { markdown_body, .. } = &proposal.kind else {
        panic!("fixture proposal creates generated Markdown");
    };
    let mut canonical_body = markdown_body.trim_end().as_bytes().to_vec();
    canonical_body.push(b'\n');
    assert_eq!(*body_hash, ContentHash::from_bytes(&canonical_body));
    assert_eq!(
        *expected_output_hash,
        ContentHash::from_bytes(generated_bytes),
        "materialization must commit the complete generated note bytes"
    );
    let expected_operation_id = OperationId::from_hash(
        vaultc::canonical::canonical_hash(
            "vaultc:generated-operation:v1\0",
            &ExpectedGeneratedOperationIdentity {
                plan_id: &approved.plan.plan_id.to_string(),
                proposal_id: &proposal.proposal_id,
                proposal_content_hash: approved_proposal.content_hash,
                destination,
                body_hash: *body_hash,
                expected_output_hash: *expected_output_hash,
                evidence_ids,
            },
        )
        .expect("independently derive generated operation commitment"),
    );
    assert_eq!(*operation_id, expected_operation_id);
}

fn expected_generated_provenance(
    approved: &ApprovedPlan,
    proposal: &KnowledgeProposal,
    generated_bytes: &[u8],
) -> ProvenanceRecord {
    let mut source_snapshot_ids: Vec<_> = proposal
        .evidence
        .iter()
        .map(|evidence| evidence.snapshot_id.clone())
        .collect();
    let mut source_document_ids: Vec<_> = proposal
        .evidence
        .iter()
        .map(|evidence| evidence.document_id.clone())
        .collect();
    source_snapshot_ids.sort();
    source_snapshot_ids.dedup();
    source_document_ids.sort();
    source_document_ids.dedup();

    let sources: Vec<_> = proposal
        .evidence
        .iter()
        .map(|evidence| {
            let document_id = evidence
                .document_id
                .parse()
                .expect("fixture evidence document ID");
            let document = approved
                .plan
                .workspace
                .documents
                .get(&document_id)
                .expect("fixture evidence document belongs to plan");
            ProvenanceSource {
                source_id: document.source_file.source_id.to_string(),
                snapshot_id: document.source_file.snapshot_id.to_string(),
                source_file_id: document.source_file.file_id.to_string(),
                source_path: document.source_file.logical_path.clone(),
                source_content_hash: document.source_file.content_hash,
                source_document_id: Some(document.document_id.to_string()),
                evidence_content_hash: Some(
                    ContentHash::parse_hex(&evidence.content_hash)
                        .expect("fixture evidence content hash"),
                ),
                byte_start: evidence.byte_start,
                byte_end: evidence.byte_end,
            }
        })
        .collect();
    let approved_proposal = approved
        .approved_proposals
        .iter()
        .find(|approved| approved.proposal.proposal_id == proposal.proposal_id)
        .expect("approved generated proposal");
    let ProposalMaterialization::GeneratedNote {
        operation_id,
        evidence_ids,
        ..
    } = &approved_proposal.materialization
    else {
        panic!("generated proposal has a generated-note materialization");
    };
    ProvenanceRecord::Output {
        output_path: GENERATED_PATH.into(),
        output_hash: ContentHash::from_bytes(generated_bytes),
        operation_id: operation_id.to_string(),
        source_snapshot_ids,
        source_document_ids,
        sources,
        proposal_id: Some(proposal.proposal_id.clone()),
        evidence: proposal.evidence.clone(),
        evidence_ids: evidence_ids.clone(),
    }
}

fn refresh_checksum(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed artifact file");
    let hash = hex::encode(Sha256::digest(bytes));
    let checksums_path = root.join(".vaultc/checksums.txt");
    let checksums = fs::read_to_string(&checksums_path).expect("read artifact checksums");
    let mut replaced = false;
    let mut output = String::new();
    for line in checksums.lines() {
        let (_, path) = line.split_once("  ").expect("valid checksum fixture line");
        if path == logical_path {
            writeln!(&mut output, "{hash}  {logical_path}")
                .expect("write changed checksum fixture line");
            replaced = true;
        } else {
            writeln!(&mut output, "{line}").expect("write unchanged checksum fixture line");
        }
    }
    assert!(replaced, "changed artifact file is covered by checksums");
    fs::write(checksums_path, output).expect("refresh fixture checksum");
}

fn reseal_artifact_after_change(root: &Path, logical_path: &str) {
    use sha2::{Digest as _, Sha256};

    let manifest_path = root.join(".vaultc/manifest.json");
    let mut manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read artifact manifest"))
            .expect("decode artifact manifest");
    let bytes = fs::read(root.join(logical_path)).expect("read deliberately changed artifact file");
    let file = manifest
        .files
        .iter_mut()
        .find(|file| file.path == logical_path)
        .expect("changed artifact file is covered by the manifest");
    file.byte_len = bytes.len() as u64;
    file.sha256 = hex::encode(Sha256::digest(bytes));
    manifest.artifact_id = vaultc::canonical::canonical_hash(
        "vaultc:artifact:v1\0",
        &(
            &manifest.plan_id,
            &manifest.files,
            &manifest.approved_proposal_hashes,
        ),
    )
    .expect("recalculate deliberately resealed artifact identity");
    fs::write(
        &manifest_path,
        vaultc::canonical::to_canonical_json_pretty(&manifest)
            .expect("encode deliberately resealed manifest"),
    )
    .expect("write deliberately resealed manifest");
    refresh_checksum(root, logical_path);
    refresh_checksum(root, ".vaultc/manifest.json");
}

fn encode_provenance(records: &[ProvenanceRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(
            vaultc::canonical::to_canonical_json(record).expect("encode provenance record"),
        );
        encoded.push(b'\n');
    }
    encoded
}

fn rewrite_generated_output_hash(root: &Path, output_bytes: &[u8]) {
    let provenance_path = root.join(".vaultc/provenance.jsonl");
    let mut records: Vec<ProvenanceRecord> = fs::read_to_string(&provenance_path)
        .expect("read generated provenance ledger")
        .lines()
        .map(|line| serde_json::from_str(line).expect("decode provenance record"))
        .collect();
    let record = records
        .iter_mut()
        .find(|record| {
            matches!(
                record,
                ProvenanceRecord::Output { output_path, .. } if output_path == GENERATED_PATH
            )
        })
        .expect("generated output has provenance");
    let ProvenanceRecord::Output { output_hash, .. } = record;
    *output_hash = ContentHash::from_bytes(output_bytes);
    fs::write(&provenance_path, encode_provenance(&records))
        .expect("rewrite generated provenance output hash");
}

fn fully_reseal_generated_output(root: &Path, output_bytes: &[u8]) {
    fs::write(root.join(GENERATED_PATH), output_bytes).expect("write forged generated output");
    rewrite_generated_output_hash(root, output_bytes);
    reseal_artifact_after_change(root, GENERATED_PATH);
    reseal_artifact_after_change(root, ".vaultc/provenance.jsonl");
}

fn replace_first_source_id(note: &str) -> String {
    let forged = format!("evidence_{}", "0".repeat(64));
    let quoted = serde_json::to_string(&forged).expect("quote forged evidence ID");
    let mut replaced = false;
    let mut output = String::new();
    for line in note.split_inclusive('\n') {
        if !replaced && line.starts_with("  - ") {
            writeln!(&mut output, "  - {quoted}").expect("write forged source identity");
            replaced = true;
        } else {
            output.push_str(line);
        }
    }
    assert!(replaced, "generated note contains a source identity");
    output
}

#[test]
fn generated_frontmatter_uses_exact_canonical_evidence_ids_and_provenance_closure() {
    assert_evidence_id_golden_vectors();
    let (compiler, approved, proposal) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary generated provenance parent");
    let artifact = temporary.path().join("compiled");
    compiler
        .compile(&approved, &artifact)
        .expect("compile generated provenance artifact");
    assert!(
        compiler
            .verify(&artifact)
            .expect("verify generated artifact")
            .valid
    );

    let generated = fs::read(artifact.join(GENERATED_PATH)).expect("read generated note");
    assert_generated_materialization(&approved, &proposal, &generated);
    let generated_text = std::str::from_utf8(&generated).expect("generated note is UTF-8");
    let (yaml, frontmatter) = generated_frontmatter(generated_text);
    let lines: Vec<_> = yaml.lines().collect();
    assert_eq!(lines[0], "vaultc_generated: true");
    assert_eq!(lines[1], "vaultc_pack_id: null");
    assert!(lines[2].starts_with("vaultc_proposal_id: "));
    assert_eq!(lines[3], "vaultc_sources:");
    assert!(!yaml.contains("vaultc_confidence:"));

    let source_ids: Vec<_> = frontmatter["vaultc_sources"]
        .as_sequence()
        .expect("vaultc_sources is a YAML sequence")
        .iter()
        .map(|source| {
            source
                .as_str()
                .expect("vaultc source identity is a string")
                .to_owned()
        })
        .collect();
    assert_eq!(source_ids.len(), proposal.evidence.len());
    let expected_source_ids: Vec<_> = proposal
        .evidence
        .iter()
        .map(independent_evidence_id)
        .collect();
    assert_eq!(
        source_ids, expected_source_ids,
        "generated EvidenceId values and their order must exactly match approved evidence"
    );
    assert!(
        source_ids
            .iter()
            .all(|source_id| source_id.parse::<EvidenceId>().is_ok())
    );

    let explanation = compiler
        .explain_provenance(&artifact, GENERATED_PATH)
        .expect("explain generated provenance");
    assert_eq!(
        explanation.records,
        vec![expected_generated_provenance(
            &approved, &proposal, &generated,
        )],
        "generated explanation must be the exact approved evidence/source closure"
    );
}

#[test]
fn verifier_rejects_resealed_generated_body_or_source_ids() {
    let (compiler, approved, _) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary generated tamper parent");
    let baseline = temporary.path().join("baseline");
    compiler
        .compile(&approved, &baseline)
        .expect("compile generated tamper baseline");
    let original =
        fs::read_to_string(baseline.join(GENERATED_PATH)).expect("read generated tamper baseline");

    let attacks = [
        (
            "generated body",
            original.replace(
                "Evidence-bound synthesis from two documents.",
                "Forged synthesis with different semantics.",
            ),
        ),
        (
            "proposal ID",
            original.replace(PROPOSAL_ID, "proposal-generated-forged"),
        ),
        ("evidence/source ID", replace_first_source_id(&original)),
    ];
    let mut accepted = Vec::new();
    for (name, forged) in attacks {
        assert_ne!(forged, original, "{name} attack changes generated bytes");
        let artifact = temporary.path().join(name.replace(['/', ' '], "-"));
        common::copy_tree(&baseline, &artifact);
        fully_reseal_generated_output(&artifact, forged.as_bytes());
        match compiler.verify(&artifact) {
            Ok(report) if report.valid => accepted.push(name),
            Ok(_) => panic!("{name} verification returned a non-valid success report"),
            Err(VaultcError::VerificationFailed(_)) => {}
            Err(error) => panic!("{name} must fail as a verification error: {error}"),
        }
    }
    assert!(
        accepted.is_empty(),
        "verifier accepted fully resealed generated derivation attacks: {accepted:?}"
    );
}

#[test]
fn generated_provenance_rejects_proposal_evidence_and_source_identity_tampering() {
    let (compiler, approved, _) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary closure tamper parent");
    let baseline = temporary.path().join("baseline");
    compiler
        .compile(&approved, &baseline)
        .expect("compile closure tamper baseline");

    let mut accepted = Vec::new();
    for attack in ["proposal", "evidence", "evidence-id", "source"] {
        let artifact = temporary.path().join(attack);
        common::copy_tree(&baseline, &artifact);
        let provenance_path = artifact.join(".vaultc/provenance.jsonl");
        let mut records: Vec<ProvenanceRecord> = fs::read_to_string(&provenance_path)
            .expect("read generated provenance")
            .lines()
            .map(|line| serde_json::from_str(line).expect("decode generated provenance"))
            .collect();
        let record = records
            .iter_mut()
            .find(|record| {
                matches!(
                    record,
                    ProvenanceRecord::Output { output_path, .. } if output_path == GENERATED_PATH
                )
            })
            .expect("generated provenance record");
        let ProvenanceRecord::Output {
            proposal_id,
            evidence,
            evidence_ids,
            sources,
            ..
        } = record;
        match attack {
            "proposal" => *proposal_id = Some("proposal-generated-forged".into()),
            "evidence" => {
                evidence
                    .pop()
                    .expect("fixture has multiple evidence records");
            }
            "evidence-id" => {
                evidence_ids[0] = format!("evidence_{}", "0".repeat(64))
                    .parse()
                    .expect("syntactically valid forged EvidenceId");
            }
            "source" => {
                let replacement = sources[1].source_document_id.clone();
                sources[0].source_document_id = replacement;
            }
            _ => unreachable!(),
        }
        fs::write(&provenance_path, encode_provenance(&records))
            .expect("write forged generated provenance");
        reseal_artifact_after_change(&artifact, ".vaultc/provenance.jsonl");
        match compiler.verify(&artifact) {
            Ok(report) if report.valid => accepted.push(attack),
            Ok(_) => panic!("{attack} verification returned a non-valid success report"),
            Err(VaultcError::VerificationFailed(_)) => {}
            Err(error) => panic!("{attack} must fail as a verification error: {error}"),
        }
    }
    assert!(
        accepted.is_empty(),
        "verifier accepted generated provenance identity attacks: {accepted:?}"
    );
}

#[test]
fn generated_artifact_pack_and_explanation_are_deterministic() {
    let (compiler, approved, _) = approved_generated_fixture();
    let temporary = tempfile::tempdir().expect("temporary generated determinism parent");
    let first_artifact = temporary.path().join("first-artifact");
    let second_artifact = temporary.path().join("second-artifact");
    compiler
        .compile(&approved, &first_artifact)
        .expect("compile first generated artifact");
    compiler
        .compile(&approved, &second_artifact)
        .expect("compile second generated artifact");
    assert_eq!(
        common::tree_bytes(&first_artifact),
        common::tree_bytes(&second_artifact),
        "generated compiled artifact bytes must be deterministic"
    );

    let first_pack = temporary.path().join("first.vaultpack");
    let second_pack = temporary.path().join("second.vaultpack");
    vaultc::pack::create_pack(
        &first_artifact,
        &first_pack,
        compiler.policy().output.zstd_level,
    )
    .expect("pack first generated artifact");
    vaultc::pack::create_pack(
        &second_artifact,
        &second_pack,
        compiler.policy().output.zstd_level,
    )
    .expect("pack second generated artifact");
    assert_eq!(
        fs::read(&first_pack).expect("read first generated pack"),
        fs::read(&second_pack).expect("read second generated pack"),
        "generated VaultPack bytes must be deterministic"
    );
    assert!(
        compiler
            .verify(&first_pack)
            .expect("verify generated pack")
            .valid
    );
    assert_eq!(
        compiler
            .explain_provenance(&first_artifact, GENERATED_PATH)
            .expect("explain generated directory artifact"),
        compiler
            .explain_provenance(&first_pack, GENERATED_PATH)
            .expect("explain generated VaultPack"),
        "directory and VaultPack explanations must be byte-semantically identical"
    );
}
