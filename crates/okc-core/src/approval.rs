use std::collections::BTreeMap;

use okc_protocol::KnowledgeProposal;
use serde::{Deserialize, Serialize};

use crate::error::{OkcError, Result};
use crate::identity::{ContentHash, DocumentId, EvidenceId, OperationId};
use crate::ir::CanvasReferenceTarget;
use crate::materialization::{MaterializationPlan, derive_materialization_plan};
use crate::plan::{ConflictResolution, ConflictSubject, DraftPlan};
use crate::provider::ValidatedProposals;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub plan_id: String,
    pub proposal_id: String,
    pub proposal_content_hash: ContentHash,
    pub approved: bool,
    pub approver: String,
    pub policy_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApprovalLog {
    pub decisions: Vec<ApprovalDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CuratorId(pub String);

impl CuratorId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if invalid_metadata(&value) {
            return Err(OkcError::ApprovalStale(
                "curator ID is empty or contains control characters".into(),
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConflictAction {
    WaivePreserveOriginal,
    SelectMarkdownTarget { target_document_id: DocumentId },
    SelectCanvasTarget { target: CanvasReferenceTarget },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionOverlay {
    pub plan_id: String,
    pub conflict_id: String,
    pub conflict_content_hash: ContentHash,
    pub action: ConflictAction,
    pub decided_by: CuratorId,
    pub policy_version: String,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DecisionOverlayLog {
    pub decisions: Vec<DecisionOverlay>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposalMaterialization {
    NonMaterializing,
    GeneratedNote {
        destination: String,
        body_hash: ContentHash,
        expected_output_hash: ContentHash,
        evidence_ids: Vec<EvidenceId>,
        operation_id: OperationId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedProposal {
    pub proposal: KnowledgeProposal,
    pub content_hash: ContentHash,
    pub approval: ApprovalDecision,
    pub materialization: ProposalMaterialization,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovedPlan {
    pub plan: DraftPlan,
    #[serde(default)]
    pub proposal_validations: Vec<crate::provider::ProposalValidation>,
    pub approved_proposals: Vec<ApprovedProposal>,
    #[serde(default)]
    pub conflict_decisions: Vec<DecisionOverlay>,
    pub conflict_decisions_complete: bool,
    pub materialization: MaterializationPlan,
    pub transcript: Vec<okc_protocol::TranscriptRecord>,
}

pub fn approve_plan(
    plan: DraftPlan,
    validated: ValidatedProposals,
    approvals: ApprovalLog,
) -> Result<ApprovedPlan> {
    approve_plan_with_conflicts(plan, validated, approvals, DecisionOverlayLog::default())
}

pub fn approve_plan_with_conflicts(
    plan: DraftPlan,
    validated: ValidatedProposals,
    mut approvals: ApprovalLog,
    mut conflict_log: DecisionOverlayLog,
) -> Result<ApprovedPlan> {
    plan.validate_integrity()?;
    validate_transcript(&validated.transcript, &plan, &validated.validations)?;
    conflict_log.decisions.sort_by(|left, right| {
        left.conflict_id
            .as_bytes()
            .cmp(right.conflict_id.as_bytes())
    });
    validate_conflict_decisions(&plan, &conflict_log.decisions)?;
    validate_required_conflict_coverage(&plan, &conflict_log.decisions)?;
    validate_proposal_approvals(&plan, &validated, &mut approvals)?;
    let approved_proposals = collect_approved_proposals(&plan, &validated, &approvals)?;
    let materialization =
        derive_materialization_plan(&plan, &conflict_log.decisions, &approved_proposals, None)?;
    Ok(ApprovedPlan {
        plan,
        proposal_validations: validated.validations,
        approved_proposals,
        conflict_decisions: conflict_log.decisions,
        conflict_decisions_complete: true,
        materialization,
        transcript: validated.transcript,
    })
}

fn validate_required_conflict_coverage(
    plan: &DraftPlan,
    decisions: &[DecisionOverlay],
) -> Result<()> {
    let decided_conflicts: std::collections::BTreeSet<_> = decisions
        .iter()
        .map(|decision| decision.conflict_id.as_str())
        .collect();
    let unresolved: Vec<_> = plan
        .unresolved_required_conflicts()
        .filter(|conflict| !decided_conflicts.contains(conflict.conflict_id.as_str()))
        .map(|conflict| conflict.conflict_id.clone())
        .collect();
    if !unresolved.is_empty() {
        return Err(OkcError::ApprovalStale(format!(
            "required conflicts remain unresolved: {}",
            unresolved.join(", ")
        )));
    }
    Ok(())
}

fn validate_proposal_approvals(
    plan: &DraftPlan,
    validated: &ValidatedProposals,
    approvals: &mut ApprovalLog,
) -> Result<()> {
    approvals.decisions.sort_by(|left, right| {
        left.proposal_id
            .as_bytes()
            .cmp(right.proposal_id.as_bytes())
    });
    for pair in approvals.decisions.windows(2) {
        if pair[0].proposal_id == pair[1].proposal_id {
            return Err(OkcError::ApprovalStale(format!(
                "proposal `{}` has more than one approval decision",
                pair[0].proposal_id
            )));
        }
    }
    for decision in &approvals.decisions {
        if decision.plan_id != plan.plan_id.to_string() {
            return Err(OkcError::ApprovalStale(format!(
                "proposal `{}` approval belongs to a different plan",
                decision.proposal_id
            )));
        }
        if invalid_metadata(&decision.approver) || invalid_metadata(&decision.policy_version) {
            return Err(OkcError::ApprovalStale(format!(
                "proposal `{}` approval metadata is empty or contains control characters",
                decision.proposal_id
            )));
        }
        let Some(validation) = validated
            .validations
            .iter()
            .find(|validation| validation.proposal.proposal_id == decision.proposal_id)
        else {
            return Err(OkcError::ApprovalStale(format!(
                "proposal `{}` approval does not reference a validated proposal",
                decision.proposal_id
            )));
        };
        if !validation.valid {
            return Err(OkcError::ApprovalStale(format!(
                "proposal `{}` approval references an invalid proposal",
                decision.proposal_id
            )));
        }
    }
    Ok(())
}

fn collect_approved_proposals(
    plan: &DraftPlan,
    validated: &ValidatedProposals,
    approvals: &ApprovalLog,
) -> Result<Vec<ApprovedProposal>> {
    let decision_by_id: BTreeMap<_, _> = approvals
        .decisions
        .iter()
        .map(|decision| (decision.proposal_id.as_str(), decision))
        .collect();
    let mut approved_proposals = Vec::new();
    for validation in validated.valid() {
        let Some(decision) = decision_by_id.get(validation.proposal.proposal_id.as_str()) else {
            continue;
        };
        if decision.proposal_content_hash != validation.content_hash {
            return Err(OkcError::ApprovalStale(format!(
                "proposal `{}` content hash changed",
                validation.proposal.proposal_id
            )));
        }
        if decision.approved {
            let materialization =
                proposal_materialization(plan, &validation.proposal, validation.content_hash)?;
            approved_proposals.push(ApprovedProposal {
                proposal: validation.proposal.clone(),
                content_hash: validation.content_hash,
                approval: (*decision).clone(),
                materialization,
            });
        }
    }
    approved_proposals.sort_by(|left, right| {
        left.proposal
            .proposal_id
            .as_bytes()
            .cmp(right.proposal.proposal_id.as_bytes())
    });
    validate_materialization_destinations(plan, &approved_proposals)?;
    Ok(approved_proposals)
}

pub(crate) fn proposal_materialization(
    plan: &DraftPlan,
    proposal: &KnowledgeProposal,
    proposal_content_hash: ContentHash,
) -> Result<ProposalMaterialization> {
    let okc_protocol::ProposalKind::CreateGeneratedNote { markdown_body, .. } = &proposal.kind
    else {
        return Ok(ProposalMaterialization::NonMaterializing);
    };
    let destination = crate::provenance::generated_output_path(proposal).ok_or_else(|| {
        OkcError::ProposalInvalid(format!(
            "generated proposal `{}` has no output path",
            proposal.proposal_id
        ))
    })?;
    crate::snapshot::validate_output_logical_path(&destination, &plan.policy)?;
    let evidence_ids = crate::generated::evidence_ids(&proposal.evidence)?;
    let rendered = crate::generated::render_generated_note(
        &proposal.proposal_id,
        markdown_body,
        &evidence_ids,
    )?;
    let operation_id = crate::generated::generated_operation_id(
        &plan.plan_id.to_string(),
        &proposal.proposal_id,
        proposal_content_hash,
        &destination,
        rendered.body_hash,
        rendered.expected_output_hash,
        &evidence_ids,
    )?;
    Ok(ProposalMaterialization::GeneratedNote {
        destination,
        body_hash: rendered.body_hash,
        expected_output_hash: rendered.expected_output_hash,
        evidence_ids,
        operation_id,
    })
}

fn validate_materialization_destinations(
    plan: &DraftPlan,
    proposals: &[ApprovedProposal],
) -> Result<()> {
    let mut destinations: BTreeMap<String, String> = plan
        .operations
        .iter()
        .map(|operation| {
            (
                crate::plan::portable_key(operation.destination()),
                operation.destination().to_owned(),
            )
        })
        .collect();
    for proposal in proposals {
        let ProposalMaterialization::GeneratedNote { destination, .. } = &proposal.materialization
        else {
            continue;
        };
        let key = crate::plan::portable_key(destination);
        if let Some(existing) = destinations.insert(key, destination.clone()) {
            return Err(OkcError::ProposalInvalid(format!(
                "generated output `{destination}` collides with sealed output `{existing}`"
            )));
        }
    }
    Ok(())
}

pub fn validate_conflict_decisions(plan: &DraftPlan, decisions: &[DecisionOverlay]) -> Result<()> {
    plan.validate_integrity()?;
    let mut seen = std::collections::BTreeSet::new();
    for decision in decisions {
        if !seen.insert(decision.conflict_id.as_str()) {
            return Err(OkcError::ApprovalStale(format!(
                "conflict `{}` has more than one decision",
                decision.conflict_id
            )));
        }
        if decision.plan_id != plan.plan_id.to_string() {
            return Err(OkcError::ApprovalStale(format!(
                "conflict `{}` decision belongs to a different plan",
                decision.conflict_id
            )));
        }
        if invalid_metadata(&decision.decided_by.0)
            || invalid_metadata(&decision.policy_version)
            || decision
                .rationale
                .as_deref()
                .is_some_and(|value| value.len() > 16 * 1024 || value.chars().any(char::is_control))
        {
            return Err(OkcError::ApprovalStale(format!(
                "conflict `{}` decision metadata is invalid",
                decision.conflict_id
            )));
        }
        let Some(conflict) = plan
            .conflicts
            .iter()
            .find(|conflict| conflict.conflict_id == decision.conflict_id)
        else {
            return Err(OkcError::ApprovalStale(format!(
                "decision references unknown conflict `{}`",
                decision.conflict_id
            )));
        };
        if conflict.resolution != ConflictResolution::Unresolved {
            return Err(OkcError::ApprovalStale(format!(
                "conflict `{}` was already resolved by the sealed plan",
                decision.conflict_id
            )));
        }
        if decision.conflict_content_hash != conflict.content_hash {
            return Err(OkcError::ApprovalStale(format!(
                "conflict `{}` decision has a stale content hash",
                decision.conflict_id
            )));
        }
        validate_conflict_action(plan, conflict, &decision.action)?;
    }
    Ok(())
}

fn validate_conflict_action(
    plan: &DraftPlan,
    conflict: &crate::plan::Conflict,
    action: &ConflictAction,
) -> Result<()> {
    if conflict.kind != crate::plan::ConflictKind::LinkAmbiguity {
        return match action {
            ConflictAction::WaivePreserveOriginal => Ok(()),
            ConflictAction::SelectMarkdownTarget { .. }
            | ConflictAction::SelectCanvasTarget { .. } => Err(OkcError::ApprovalStale(format!(
                "conflict `{}` does not support a target action",
                conflict.conflict_id
            ))),
        };
    }

    match (&conflict.subject, action) {
        (_, ConflictAction::WaivePreserveOriginal) => Ok(()),
        (
            Some(ConflictSubject::MarkdownLink {
                document_id,
                link_id,
                raw_target,
            }),
            ConflictAction::SelectMarkdownTarget { target_document_id },
        ) => {
            let link = plan
                .workspace
                .documents
                .get(document_id)
                .and_then(|document| {
                    document
                        .links
                        .iter()
                        .find(|link| link.link_id == *link_id && link.raw_target == *raw_target)
                })
                .ok_or_else(|| {
                    OkcError::ApprovalStale(format!(
                        "conflict `{}` Markdown subject is stale",
                        conflict.conflict_id
                    ))
                })?;
            let crate::ir::LinkResolution::Ambiguous { candidates } = &link.resolution else {
                return Err(OkcError::ApprovalStale(format!(
                    "conflict `{}` Markdown candidates are stale",
                    conflict.conflict_id
                )));
            };
            if !candidates.contains(target_document_id) {
                return Err(OkcError::ApprovalStale(format!(
                    "conflict `{}` selected an unsealed Markdown target",
                    conflict.conflict_id
                )));
            }
            Ok(())
        }
        (
            Some(ConflictSubject::CanvasReference {
                canvas_id,
                node_id,
                raw_path,
            }),
            ConflictAction::SelectCanvasTarget { target },
        ) => {
            let reference = plan
                .workspace
                .canvases
                .get(canvas_id)
                .and_then(|canvas| {
                    canvas.file_references.iter().find(|reference| {
                        reference.node_id == *node_id && reference.raw_path == *raw_path
                    })
                })
                .ok_or_else(|| {
                    OkcError::ApprovalStale(format!(
                        "conflict `{}` Canvas subject is stale",
                        conflict.conflict_id
                    ))
                })?;
            let crate::ir::CanvasReferenceResolution::Ambiguous { candidates } =
                &reference.resolution
            else {
                return Err(OkcError::ApprovalStale(format!(
                    "conflict `{}` Canvas candidates are stale",
                    conflict.conflict_id
                )));
            };
            if !candidates.contains(target) {
                return Err(OkcError::ApprovalStale(format!(
                    "conflict `{}` selected an unsealed Canvas target",
                    conflict.conflict_id
                )));
            }
            Ok(())
        }
        _ => Err(OkcError::ApprovalStale(format!(
            "conflict `{}` action type does not match its sealed subject",
            conflict.conflict_id
        ))),
    }
}

fn invalid_metadata(value: &str) -> bool {
    value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control)
}

fn validate_transcript(
    records: &[okc_protocol::TranscriptRecord],
    plan: &DraftPlan,
    validations: &[crate::provider::ProposalValidation],
) -> Result<()> {
    crate::augmentation::validate_transcript(plan, &plan.policy, records, validations)
}

pub fn validate_approved_plan(
    approved: &ApprovedPlan,
    policy: &crate::config::CompilerPolicy,
) -> Result<()> {
    approved.plan.validate_integrity()?;
    if approved.plan.policy.semantic_hash()? != policy.semantic_hash()? {
        return Err(OkcError::PlanStale(
            "approved plan policy does not match compiler policy".into(),
        ));
    }
    let proposals: Vec<_> = approved
        .proposal_validations
        .iter()
        .map(|validation| validation.proposal.clone())
        .collect();
    let mut validated = crate::provider::validate_proposals(&approved.plan, proposals, policy)?;
    if validated.validations != approved.proposal_validations {
        return Err(OkcError::ApprovalStale(
            "recorded proposal validations are stale".into(),
        ));
    }
    validated.transcript.clone_from(&approved.transcript);
    validate_transcript(
        &validated.transcript,
        &approved.plan,
        &validated.validations,
    )?;
    let approvals = ApprovalLog {
        decisions: approved
            .approved_proposals
            .iter()
            .map(|proposal| proposal.approval.clone())
            .collect(),
    };
    let mut conflict_decisions = approved.conflict_decisions.clone();
    conflict_decisions.sort_by(|left, right| {
        left.conflict_id
            .as_bytes()
            .cmp(right.conflict_id.as_bytes())
    });
    validate_conflict_decisions(&approved.plan, &conflict_decisions)?;
    validate_required_conflict_coverage(&approved.plan, &conflict_decisions)?;
    let mut approvals = approvals;
    validate_proposal_approvals(&approved.plan, &validated, &mut approvals)?;
    let approved_proposals = collect_approved_proposals(&approved.plan, &validated, &approvals)?;
    let materialization = derive_materialization_plan(
        &approved.plan,
        &conflict_decisions,
        &approved_proposals,
        Some(&approved.materialization),
    )?;
    let rebuilt = ApprovedPlan {
        plan: approved.plan.clone(),
        proposal_validations: validated.validations,
        approved_proposals,
        conflict_decisions,
        conflict_decisions_complete: true,
        materialization,
        transcript: validated.transcript,
    };
    if rebuilt.approved_proposals != approved.approved_proposals
        || rebuilt.proposal_validations != approved.proposal_validations
        || rebuilt.conflict_decisions != approved.conflict_decisions
        || rebuilt.conflict_decisions_complete != approved.conflict_decisions_complete
        || rebuilt.materialization != approved.materialization
    {
        return Err(OkcError::ApprovalStale(
            "approved plan content no longer matches its sealed decisions".into(),
        ));
    }
    Ok(())
}
