use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vaultc_protocol::KnowledgeProposal;

use crate::error::{Result, VaultcError};
use crate::identity::{ContentHash, EvidenceId, OperationId};
use crate::plan::{ConflictResolution, DraftPlan};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictDecision {
    pub plan_id: String,
    pub conflict_id: String,
    pub conflict_content_hash: ContentHash,
    pub resolution: ConflictResolution,
    pub resolver: String,
    pub policy_version: String,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConflictDecisionLog {
    pub decisions: Vec<ConflictDecision>,
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
    pub conflict_decisions: Vec<ConflictDecision>,
    pub conflict_decisions_complete: bool,
    pub transcript: Vec<vaultc_protocol::TranscriptRecord>,
}

pub fn approve_plan(
    plan: DraftPlan,
    validated: ValidatedProposals,
    approvals: ApprovalLog,
) -> Result<ApprovedPlan> {
    approve_plan_with_conflicts(plan, validated, approvals, ConflictDecisionLog::default())
}

pub fn approve_plan_with_conflicts(
    plan: DraftPlan,
    validated: ValidatedProposals,
    mut approvals: ApprovalLog,
    mut conflict_log: ConflictDecisionLog,
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
    Ok(ApprovedPlan {
        plan,
        proposal_validations: validated.validations,
        approved_proposals,
        conflict_decisions: conflict_log.decisions,
        conflict_decisions_complete: true,
        transcript: validated.transcript,
    })
}

fn validate_required_conflict_coverage(
    plan: &DraftPlan,
    decisions: &[ConflictDecision],
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
        return Err(VaultcError::ApprovalStale(format!(
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
            return Err(VaultcError::ApprovalStale(format!(
                "proposal `{}` has more than one approval decision",
                pair[0].proposal_id
            )));
        }
    }
    for decision in &approvals.decisions {
        if decision.plan_id != plan.plan_id.to_string() {
            return Err(VaultcError::ApprovalStale(format!(
                "proposal `{}` approval belongs to a different plan",
                decision.proposal_id
            )));
        }
        if invalid_metadata(&decision.approver) || invalid_metadata(&decision.policy_version) {
            return Err(VaultcError::ApprovalStale(format!(
                "proposal `{}` approval metadata is empty or contains control characters",
                decision.proposal_id
            )));
        }
        let Some(validation) = validated
            .validations
            .iter()
            .find(|validation| validation.proposal.proposal_id == decision.proposal_id)
        else {
            return Err(VaultcError::ApprovalStale(format!(
                "proposal `{}` approval does not reference a validated proposal",
                decision.proposal_id
            )));
        };
        if !validation.valid {
            return Err(VaultcError::ApprovalStale(format!(
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
            return Err(VaultcError::ApprovalStale(format!(
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
    let vaultc_protocol::ProposalKind::CreateGeneratedNote { markdown_body, .. } = &proposal.kind
    else {
        return Ok(ProposalMaterialization::NonMaterializing);
    };
    let destination = crate::provenance::generated_output_path(proposal).ok_or_else(|| {
        VaultcError::ProposalInvalid(format!(
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
            return Err(VaultcError::ProposalInvalid(format!(
                "generated output `{destination}` collides with sealed output `{existing}`"
            )));
        }
    }
    Ok(())
}

pub fn validate_conflict_decisions(plan: &DraftPlan, decisions: &[ConflictDecision]) -> Result<()> {
    plan.validate_integrity()?;
    let mut seen = std::collections::BTreeSet::new();
    for decision in decisions {
        if !seen.insert(decision.conflict_id.as_str()) {
            return Err(VaultcError::ApprovalStale(format!(
                "conflict `{}` has more than one decision",
                decision.conflict_id
            )));
        }
        if decision.plan_id != plan.plan_id.to_string() {
            return Err(VaultcError::ApprovalStale(format!(
                "conflict `{}` decision belongs to a different plan",
                decision.conflict_id
            )));
        }
        // V1 has no typed target/rewrite payload for a genuine user
        // resolution. An explicit waiver may preserve the source link, but it
        // must not be mislabeled as resolved.
        if decision.resolution != ConflictResolution::WaivedByPolicy {
            return Err(VaultcError::ApprovalStale(format!(
                "conflict `{}` must use waived_by_policy until a typed resolution action exists",
                decision.conflict_id
            )));
        }
        if invalid_metadata(&decision.resolver)
            || invalid_metadata(&decision.policy_version)
            || decision
                .rationale
                .as_deref()
                .is_some_and(|value| value.len() > 16 * 1024 || value.chars().any(char::is_control))
        {
            return Err(VaultcError::ApprovalStale(format!(
                "conflict `{}` decision metadata is invalid",
                decision.conflict_id
            )));
        }
        let Some(conflict) = plan
            .conflicts
            .iter()
            .find(|conflict| conflict.conflict_id == decision.conflict_id)
        else {
            return Err(VaultcError::ApprovalStale(format!(
                "decision references unknown conflict `{}`",
                decision.conflict_id
            )));
        };
        if conflict.resolution != ConflictResolution::Unresolved {
            return Err(VaultcError::ApprovalStale(format!(
                "conflict `{}` was already resolved by the sealed plan",
                decision.conflict_id
            )));
        }
        if decision.conflict_content_hash != conflict.content_hash {
            return Err(VaultcError::ApprovalStale(format!(
                "conflict `{}` decision has a stale content hash",
                decision.conflict_id
            )));
        }
    }
    Ok(())
}

fn invalid_metadata(value: &str) -> bool {
    value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control)
}

fn validate_transcript(
    records: &[vaultc_protocol::TranscriptRecord],
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
        return Err(VaultcError::PlanStale(
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
        return Err(VaultcError::ApprovalStale(
            "recorded proposal validations are stale".into(),
        ));
    }
    validated.transcript.clone_from(&approved.transcript);
    let approvals = ApprovalLog {
        decisions: approved
            .approved_proposals
            .iter()
            .map(|proposal| proposal.approval.clone())
            .collect(),
    };
    let rebuilt = approve_plan_with_conflicts(
        approved.plan.clone(),
        validated,
        approvals,
        ConflictDecisionLog {
            decisions: approved.conflict_decisions.clone(),
        },
    )?;
    if rebuilt.approved_proposals != approved.approved_proposals
        || rebuilt.proposal_validations != approved.proposal_validations
        || rebuilt.conflict_decisions != approved.conflict_decisions
        || rebuilt.conflict_decisions_complete != approved.conflict_decisions_complete
    {
        return Err(VaultcError::ApprovalStale(
            "approved plan content no longer matches its sealed decisions".into(),
        ));
    }
    Ok(())
}
