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
    if records.is_empty() {
        return Ok(());
    }
    let expected_types = [
        vaultc_protocol::MessageType::CapabilitiesRequest,
        vaultc_protocol::MessageType::CapabilitiesResponse,
        vaultc_protocol::MessageType::AugmentationRequest,
        vaultc_protocol::MessageType::AugmentationResponse,
    ];
    let expected_directions = [
        vaultc_protocol::TranscriptDirection::Request,
        vaultc_protocol::TranscriptDirection::Response,
        vaultc_protocol::TranscriptDirection::Request,
        vaultc_protocol::TranscriptDirection::Response,
    ];
    if records.len() != expected_types.len()
        || records
            .iter()
            .map(|record| record.message_type)
            .ne(expected_types)
        || records
            .iter()
            .map(|record| record.direction)
            .ne(expected_directions)
    {
        return Err(VaultcError::ApprovalStale(
            "provider transcript shape is invalid".into(),
        ));
    }
    for (index, record) in records.iter().enumerate() {
        if record.sequence != index as u64 || invalid_metadata(&record.request_id) {
            return Err(VaultcError::ApprovalStale(
                "provider transcript sequence or request identity is invalid".into(),
            ));
        }
        let payload = if record.message_type == vaultc_protocol::MessageType::AugmentationRequest {
            hydrate_redacted_request(&record.payload, plan)?
        } else {
            record.payload.clone()
        };
        let expected =
            crate::canonical::canonical_hash("vaultc:provider-transcript-payload:v1\0", &payload)?;
        if record.canonical_payload_hash != expected.hex() {
            return Err(VaultcError::ApprovalStale(format!(
                "provider transcript record {} has a stale payload hash",
                record.sequence
            )));
        }
    }
    validate_transcript_payloads(records, plan, validations)?;
    Ok(())
}

fn validate_transcript_payloads(
    records: &[vaultc_protocol::TranscriptRecord],
    plan: &DraftPlan,
    validations: &[crate::provider::ProposalValidation],
) -> Result<()> {
    if records[0].request_id != "capabilities-1"
        || records[1].request_id != "capabilities-1"
        || records[2].request_id != "augmentation-1"
        || records[3].request_id != "augmentation-1"
    {
        return Err(VaultcError::ApprovalStale(
            "provider transcript request IDs are invalid".into(),
        ));
    }
    let capabilities: vaultc_protocol::ProviderCapabilities =
        serde_json::from_value(records[1].payload.clone()).map_err(|error| {
            VaultcError::ApprovalStale(format!(
                "provider transcript capabilities are malformed: {error}"
            ))
        })?;
    crate::provider::validate_capabilities(&capabilities, &plan.policy)?;
    if !capabilities.structured_output {
        return Err(VaultcError::ApprovalStale(
            "provider transcript did not negotiate structured output".into(),
        ));
    }
    let hydrated = hydrate_redacted_request(&records[2].payload, plan)?;
    let request: vaultc_protocol::AugmentationRequest = serde_json::from_value(hydrated.clone())?;
    let response: vaultc_protocol::AugmentationResponse =
        serde_json::from_value(records[3].payload.clone()).map_err(|error| {
            VaultcError::ApprovalStale(format!(
                "provider transcript response is malformed: {error}"
            ))
        })?;
    if crate::canonical::to_canonical_json(&hydrated)?.len() as u64 > capabilities.max_input_bytes
        || crate::canonical::to_canonical_json(&response)?.len() as u64
            > capabilities.max_output_bytes
    {
        return Err(VaultcError::ApprovalStale(
            "provider transcript exceeds negotiated byte limits".into(),
        ));
    }
    let mut response_proposals = response.proposals;
    response_proposals.sort_by(|left, right| left.proposal_id.cmp(&right.proposal_id));
    let recorded: Vec<_> = validations
        .iter()
        .map(|validation| validation.proposal.clone())
        .collect();
    if response_proposals != recorded
        || recorded
            .iter()
            .any(|proposal| proposal.provider != capabilities.provider)
        || recorded.iter().any(|proposal| {
            !request
                .allowed_proposal_kinds
                .contains(&proposal.kind.name())
        })
    {
        return Err(VaultcError::ApprovalStale(
            "provider transcript proposals are not bound to negotiation and validation".into(),
        ));
    }
    Ok(())
}

fn hydrate_redacted_request(
    payload: &serde_json::Value,
    plan: &DraftPlan,
) -> Result<serde_json::Value> {
    let mut request: vaultc_protocol::AugmentationRequest = serde_json::from_value(payload.clone())
        .map_err(|error| {
            VaultcError::ApprovalStale(format!(
                "augmentation transcript request schema is invalid: {error}"
            ))
        })?;
    if request.plan_id != plan.plan_id.to_string()
        || request.projection_hash != plan.projection_hash.hex()
        || request.limits.max_proposals > plan.policy.augmentation.max_proposals
        || request.limits.max_generated_bytes > plan.policy.augmentation.max_generated_bytes
    {
        return Err(VaultcError::ApprovalStale(
            "augmentation transcript request is stale for this plan".into(),
        ));
    }
    let mut seen_documents = std::collections::BTreeSet::new();
    for projection in &mut request.documents {
        let document_id = projection
            .document
            .id
            .parse::<crate::identity::DocumentId>()
            .map_err(|_| {
                VaultcError::ApprovalStale(
                    "augmentation transcript contains a malformed document ID".into(),
                )
            })?;
        if !seen_documents.insert(document_id) {
            return Err(VaultcError::ApprovalStale(
                "augmentation transcript contains duplicate document projections".into(),
            ));
        }
        let document = plan.workspace.documents.get(&document_id).ok_or_else(|| {
            VaultcError::ApprovalStale(
                "augmentation transcript references an unknown document".into(),
            )
        })?;
        if projection.snapshot_id != document.source_file.snapshot_id.to_string()
            || projection.document.kind != vaultc_protocol::ObjectKind::Document
            || projection.document.content_hash != document.body_hash.hex()
            || projection.logical_path != document.source_file.logical_path
            || projection.title != document.title
        {
            return Err(VaultcError::ApprovalStale(
                "augmentation transcript document projection is stale".into(),
            ));
        }
        let mut seen_blocks = std::collections::BTreeSet::new();
        for projected in &mut projection.selected_blocks {
            let block_id = projected
                .block
                .id
                .parse::<crate::identity::BlockId>()
                .map_err(|_| {
                    VaultcError::ApprovalStale(
                        "augmentation transcript contains a malformed block ID".into(),
                    )
                })?;
            if !seen_blocks.insert(block_id) {
                return Err(VaultcError::ApprovalStale(
                    "augmentation transcript contains duplicate block projections".into(),
                ));
            }
            let block = document
                .blocks
                .iter()
                .find(|block| block.block_id == block_id)
                .ok_or_else(|| {
                    VaultcError::ApprovalStale(
                        "augmentation transcript references an unknown block".into(),
                    )
                })?;
            if projected.block.kind != vaultc_protocol::ObjectKind::Block
                || projected.block.content_hash != block.content_hash.hex()
                || projected.text != "[redacted]"
            {
                return Err(VaultcError::ApprovalStale(
                    "augmentation transcript block projection is stale or not redacted".into(),
                ));
            }
            projected.text.clone_from(&block.comparison_text);
        }
    }
    serde_json::to_value(request).map_err(Into::into)
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
