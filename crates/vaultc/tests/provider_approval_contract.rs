mod common;

use vaultc::identity::ContentHash;
use vaultc::{ApprovalDecision, ApprovalLog, DraftPlan, VaultcError};
use vaultc_protocol::{
    EvidenceRefWire, KnowledgeProposal, PROPOSAL_SCHEMA_VERSION, ProposalKind, ProviderIdentity,
};

fn valid_proposal(plan: &DraftPlan) -> KnowledgeProposal {
    let document = plan
        .workspace
        .documents
        .values()
        .next()
        .expect("fixture document");
    KnowledgeProposal {
        schema_version: PROPOSAL_SCHEMA_VERSION,
        proposal_id: "proposal-fixture-1".into(),
        plan_id: plan.plan_id.to_string(),
        projection_hash: plan.projection_hash.hex(),
        provider: ProviderIdentity {
            provider: "local-fixture".into(),
            model: "deterministic-test-double".into(),
            version: Some("1".into()),
        },
        kind: ProposalKind::CreateGeneratedNote {
            title: "Generated fixture".into(),
            markdown_body: "# Generated fixture\n\nEvidence-bound content.\n".into(),
            suggested_path: Some("generated-fixture.md".into()),
        },
        evidence: vec![EvidenceRefWire {
            snapshot_id: document.source_file.snapshot_id.to_string(),
            document_id: document.document_id.to_string(),
            block_id: None,
            byte_start: None,
            byte_end: None,
            content_hash: document.source_file.content_hash.hex(),
        }],
        uncertainty: Some(0.1),
        rationale: Some("integration-test proposal".into()),
    }
}

fn basic_plan() -> (vaultc::VaultCompiler, DraftPlan) {
    let compiler = common::compiler();
    let inspection = compiler
        .inspect([common::fixture_source("basic", "basic_vault")])
        .expect("inspect provider fixture");
    let plan = compiler.plan(&inspection).expect("plan provider fixture");
    (compiler, plan)
}

fn validate_evidence_case(
    compiler: &vaultc::VaultCompiler,
    plan: &DraftPlan,
    proposal_id: &str,
    evidence: EvidenceRefWire,
) -> vaultc::ProposalValidation {
    let mut proposal = valid_proposal(plan);
    proposal.proposal_id = proposal_id.into();
    proposal.evidence = vec![evidence];
    compiler
        .validate_proposals(plan, vec![proposal])
        .expect("validate evidence case")
        .validations
        .into_iter()
        .next()
        .expect("one proposal validation")
}

#[test]
fn evidence_span_hash_must_match_exact_block_or_file_level_policy() {
    let (compiler, plan) = basic_plan();
    let document = plan
        .workspace
        .documents
        .values()
        .next()
        .expect("fixture document");
    let block = document.blocks.first().expect("fixture document block");
    assert!(
        block.span.byte_start < block.span.byte_end,
        "fixture block must permit a distinct mismatched span"
    );

    let file_level = EvidenceRefWire {
        snapshot_id: document.source_file.snapshot_id.to_string(),
        document_id: document.document_id.to_string(),
        block_id: None,
        byte_start: None,
        byte_end: None,
        content_hash: document.source_file.content_hash.hex(),
    };
    assert!(
        validate_evidence_case(&compiler, &plan, "evidence-file-level", file_level.clone(),).valid,
        "exact file-level hash with no span is valid"
    );

    let arbitrary_file_span = EvidenceRefWire {
        byte_start: Some(0),
        byte_end: Some(1),
        ..file_level
    };
    let validation = validate_evidence_case(
        &compiler,
        &plan,
        "evidence-arbitrary-file-span",
        arbitrary_file_span,
    );
    assert!(
        !validation.valid,
        "file-level evidence cannot claim arbitrary bytes without an exact block identity"
    );

    let exact_block = EvidenceRefWire {
        snapshot_id: document.source_file.snapshot_id.to_string(),
        document_id: document.document_id.to_string(),
        block_id: Some(block.block_id.to_string()),
        byte_start: Some(block.span.byte_start),
        byte_end: Some(block.span.byte_end),
        content_hash: block.content_hash.hex(),
    };
    assert!(
        validate_evidence_case(
            &compiler,
            &plan,
            "evidence-exact-block-span",
            exact_block.clone(),
        )
        .valid,
        "exact block hash and exact block span are valid"
    );

    let block_without_span = EvidenceRefWire {
        byte_start: None,
        byte_end: None,
        ..exact_block.clone()
    };
    assert!(
        validate_evidence_case(
            &compiler,
            &plan,
            "evidence-exact-block-no-span",
            block_without_span,
        )
        .valid,
        "exact block hash may omit its redundant span"
    );

    let stale_block_hash = EvidenceRefWire {
        content_hash: ContentHash::from_bytes(b"stale block evidence").hex(),
        ..exact_block.clone()
    };
    let validation = validate_evidence_case(
        &compiler,
        &plan,
        "evidence-stale-block-hash",
        stale_block_hash,
    );
    assert!(!validation.valid, "stale block content hash must fail");

    let mismatched_block_span = EvidenceRefWire {
        byte_start: Some(block.span.byte_start + 1),
        ..exact_block
    };
    let validation = validate_evidence_case(
        &compiler,
        &plan,
        "evidence-mismatched-block-span",
        mismatched_block_span,
    );
    assert!(!validation.valid, "mismatched block span must fail");
}

#[test]
fn valid_proposal_requires_explicit_matching_approval() {
    let (compiler, plan) = basic_plan();
    let proposal = valid_proposal(&plan);

    let unapproved_validation = compiler
        .validate_proposals(&plan, vec![proposal.clone()])
        .expect("validate unapproved proposal");
    assert_eq!(unapproved_validation.valid().count(), 1);
    let unapproved = compiler
        .approve(plan.clone(), unapproved_validation, ApprovalLog::default())
        .expect("unapproved proposal remains excluded");
    assert!(unapproved.approved_proposals.is_empty());

    let temporary = tempfile::tempdir().expect("temporary provider output parent");
    let without_approval = temporary.path().join("without-approval");
    compiler
        .compile(&unapproved, &without_approval)
        .expect("compile without proposal");
    assert!(
        !without_approval
            .join("knowledge/_generated/generated-fixture.md")
            .exists()
    );

    let validated = compiler
        .validate_proposals(&plan, vec![proposal.clone()])
        .expect("validate approved proposal");
    let content_hash = validated
        .valid()
        .next()
        .expect("valid proposal record")
        .content_hash;
    let approved = compiler
        .approve(
            plan.clone(),
            validated,
            ApprovalLog {
                decisions: vec![ApprovalDecision {
                    plan_id: plan.plan_id.to_string(),
                    proposal_id: proposal.proposal_id.clone(),
                    proposal_content_hash: content_hash,
                    approved: true,
                    approver: "qa-fixture".into(),
                    policy_version: "test-v1".into(),
                }],
            },
        )
        .expect("approve matching proposal");
    assert_eq!(approved.approved_proposals.len(), 1);

    let with_approval = temporary.path().join("with-approval");
    compiler
        .compile(&approved, &with_approval)
        .expect("compile approved proposal");
    let generated_path = "knowledge/_generated/generated-fixture.md";
    let generated = std::fs::read_to_string(with_approval.join(generated_path))
        .expect("read approved generated note");
    let frontmatter = generated
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n"))
        .map(|(yaml, _)| yaml)
        .expect("generated note has YAML frontmatter");
    let frontmatter: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(frontmatter).expect("generated frontmatter is valid YAML");
    assert_eq!(frontmatter["vaultc_generated"], true);
    assert_eq!(frontmatter["vaultc_proposal_id"], "proposal-fixture-1");
    let explanation = compiler
        .explain_provenance(&with_approval, generated_path)
        .expect("explain generated note");
    let encoded = serde_json::to_value(explanation).expect("encode explanation");
    assert_eq!(encoded["records"][0]["proposal_id"], "proposal-fixture-1");
    assert_eq!(
        encoded["records"][0]["evidence"].as_array().map(Vec::len),
        Some(1)
    );
}

#[test]
fn stale_or_forged_proposals_fail_validation_or_approval() {
    let (compiler, plan) = basic_plan();
    let mut forged = valid_proposal(&plan);
    forged.projection_hash = "00".repeat(32);
    if let ProposalKind::CreateGeneratedNote { suggested_path, .. } = &mut forged.kind {
        *suggested_path = Some("../escape.md".into());
    }
    let validation = compiler
        .validate_proposals(&plan, vec![forged])
        .expect("validation returns deterministic rejection records");
    assert_eq!(validation.valid().count(), 0);
    assert!(!validation.validations[0].valid);
    assert!(
        validation.validations[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("projection hash"))
    );
    assert!(
        validation.validations[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("unsafe path"))
    );

    let proposal = valid_proposal(&plan);
    let validated = compiler
        .validate_proposals(&plan, vec![proposal.clone()])
        .expect("validate proposal for stale approval test");
    let content_hash = validated
        .valid()
        .next()
        .expect("valid proposal")
        .content_hash;
    let error = compiler
        .approve(
            plan.clone(),
            validated,
            ApprovalLog {
                decisions: vec![ApprovalDecision {
                    plan_id: "plan_stale".into(),
                    proposal_id: proposal.proposal_id,
                    proposal_content_hash: content_hash,
                    approved: true,
                    approver: "forged".into(),
                    policy_version: "test-v1".into(),
                }],
            },
        )
        .expect_err("approval bound to another plan must be stale");
    assert!(matches!(error, VaultcError::ApprovalStale(_)));

    let proposal = valid_proposal(&plan);
    let validated = compiler
        .validate_proposals(&plan, vec![proposal.clone()])
        .expect("validate proposal for stale content test");
    let error = compiler
        .approve(
            plan.clone(),
            validated,
            ApprovalLog {
                decisions: vec![ApprovalDecision {
                    plan_id: plan.plan_id.to_string(),
                    proposal_id: proposal.proposal_id,
                    proposal_content_hash: ContentHash::from_bytes(b"stale"),
                    approved: true,
                    approver: "forged".into(),
                    policy_version: "test-v1".into(),
                }],
            },
        )
        .expect_err("approval bound to changed proposal must be stale");
    assert!(matches!(error, VaultcError::ApprovalStale(_)));
}

#[test]
fn approved_proposal_content_cannot_change_before_materialization() {
    let (compiler, plan) = basic_plan();
    let proposal = valid_proposal(&plan);
    let validated = compiler
        .validate_proposals(&plan, vec![proposal.clone()])
        .expect("validate proposal");
    let content_hash = validated
        .valid()
        .next()
        .expect("valid proposal")
        .content_hash;
    let mut approved = compiler
        .approve(
            plan.clone(),
            validated,
            ApprovalLog {
                decisions: vec![ApprovalDecision {
                    plan_id: plan.plan_id.to_string(),
                    proposal_id: proposal.proposal_id,
                    proposal_content_hash: content_hash,
                    approved: true,
                    approver: "qa-fixture".into(),
                    policy_version: "test-v1".into(),
                }],
            },
        )
        .expect("approve proposal");
    let ProposalKind::CreateGeneratedNote { markdown_body, .. } =
        &mut approved.approved_proposals[0].proposal.kind
    else {
        panic!("fixture proposal must materialize a generated note");
    };
    markdown_body.push_str("\nUnapproved mutation.\n");

    let temporary = tempfile::tempdir().expect("temporary tampered-proposal output");
    let destination = temporary.path().join("must-not-publish");
    let error = compiler
        .compile(&approved, &destination)
        .expect_err("changed approved proposal must be stale");
    assert!(matches!(
        error,
        VaultcError::ApprovalStale(_) | VaultcError::ProposalInvalid(_)
    ));
    assert!(!destination.exists());
}

#[test]
fn generated_frontmatter_quotes_untrusted_proposal_identity_as_data() {
    let (compiler, plan) = basic_plan();
    let mut proposal = valid_proposal(&plan);
    proposal.proposal_id = "quoted\"\nvaultc_generated: false".into();
    let validated = compiler
        .validate_proposals(&plan, vec![proposal.clone()])
        .expect("validate proposal with hostile identifier");
    let content_hash = validated
        .valid()
        .next()
        .expect("hostile identifier remains representable data")
        .content_hash;
    let approved = compiler
        .approve(
            plan.clone(),
            validated,
            ApprovalLog {
                decisions: vec![ApprovalDecision {
                    plan_id: plan.plan_id.to_string(),
                    proposal_id: proposal.proposal_id.clone(),
                    proposal_content_hash: content_hash,
                    approved: true,
                    approver: "qa-fixture".into(),
                    policy_version: "test-v1".into(),
                }],
            },
        )
        .expect("approve proposal with hostile identifier");
    let temporary = tempfile::tempdir().expect("temporary YAML-injection output");
    let destination = temporary.path().join("compiled");
    compiler
        .compile(&approved, &destination)
        .expect("compile safely quoted generated note");
    let generated =
        std::fs::read_to_string(destination.join("knowledge/_generated/generated-fixture.md"))
            .expect("read generated note");
    let frontmatter = generated
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n"))
        .map(|(yaml, _)| yaml)
        .expect("generated note has YAML frontmatter");
    let frontmatter: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(frontmatter).expect("generated frontmatter remains valid YAML");

    assert_eq!(frontmatter["vaultc_generated"], true);
    assert_eq!(
        frontmatter["vaultc_proposal_id"],
        proposal.proposal_id.as_str()
    );
}
