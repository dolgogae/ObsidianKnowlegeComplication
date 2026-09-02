//! Immutable derivation of the effective V2 output operation set.

use serde::{Deserialize, Serialize};

use crate::approval::{ApprovedProposal, ConflictAction, DecisionOverlay};
use crate::canonical::canonical_hash;
use crate::error::{OkcError, Result};
use crate::identity::{ContentHash, MaterializationId, PlanId};
use crate::plan::{ConflictSubject, DraftPlan, OutputOperation};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializationPlan {
    pub base_plan_id: PlanId,
    pub action_set_hash: ContentHash,
    pub proposal_set_hash: ContentHash,
    pub effective_operations: Vec<OutputOperation>,
    pub materialization_id: MaterializationId,
}

#[derive(Serialize)]
struct MaterializationIdentity<'a> {
    base_plan_id: PlanId,
    action_set_hash: ContentHash,
    proposal_set_hash: ContentHash,
    effective_operations: &'a [OutputOperation],
}

pub(crate) fn derive_materialization_plan(
    plan: &DraftPlan,
    decisions: &[DecisionOverlay],
    proposals: &[ApprovedProposal],
    committed: Option<&MaterializationPlan>,
) -> Result<MaterializationPlan> {
    let action_set_hash = canonical_hash("okc:action-set:v2\0", decisions)?;
    let proposal_commitments: Vec<_> = proposals
        .iter()
        .map(|proposal| {
            (
                &proposal.proposal.proposal_id,
                proposal.content_hash,
                &proposal.materialization,
            )
        })
        .collect();
    let proposal_set_hash = canonical_hash("okc:proposal-set:v2\0", &proposal_commitments)?;
    let mut effective_operations = plan.operations.clone();

    for decision in decisions {
        let conflict = plan
            .conflicts
            .iter()
            .find(|conflict| conflict.conflict_id == decision.conflict_id)
            .ok_or_else(|| {
                OkcError::ApprovalStale(format!(
                    "materialization references unknown conflict `{}`",
                    decision.conflict_id
                ))
            })?;
        match (&decision.action, &conflict.subject) {
            (ConflictAction::WaivePreserveOriginal, _) => {}
            (
                ConflictAction::SelectMarkdownTarget { target_document_id },
                Some(ConflictSubject::MarkdownLink {
                    document_id,
                    link_id,
                    ..
                }),
            ) => crate::plan::apply_markdown_target_selection(
                plan,
                &mut effective_operations,
                *document_id,
                *link_id,
                *target_document_id,
                committed.map(|materialization| materialization.effective_operations.as_slice()),
            )?,
            (
                ConflictAction::SelectCanvasTarget { target },
                Some(ConflictSubject::CanvasReference {
                    canvas_id, node_id, ..
                }),
            ) => crate::plan::apply_canvas_target_selection(
                plan,
                &mut effective_operations,
                *canvas_id,
                node_id,
                *target,
            )?,
            _ => {
                return Err(OkcError::ApprovalStale(format!(
                    "conflict `{}` action does not match its sealed subject",
                    decision.conflict_id
                )));
            }
        }
    }

    effective_operations.sort_by(|left, right| {
        left.destination()
            .as_bytes()
            .cmp(right.destination().as_bytes())
            .then_with(|| left.operation_id().cmp(&right.operation_id()))
    });
    let identity = MaterializationIdentity {
        base_plan_id: plan.plan_id,
        action_set_hash,
        proposal_set_hash,
        effective_operations: &effective_operations,
    };
    let materialization_id =
        MaterializationId::from_hash(canonical_hash("okc:materialization:v2\0", &identity)?);
    Ok(MaterializationPlan {
        base_plan_id: plan.plan_id,
        action_set_hash,
        proposal_set_hash,
        effective_operations,
        materialization_id,
    })
}
