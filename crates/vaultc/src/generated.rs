use serde::Serialize;

use crate::error::{Result, VaultcError};
use crate::identity::{ContentHash, EvidenceId, OperationId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedGeneratedNote {
    pub bytes: Vec<u8>,
    pub body_hash: ContentHash,
    pub expected_output_hash: ContentHash,
}

pub(crate) fn evidence_ids(
    evidence: &[vaultc_protocol::EvidenceRefWire],
) -> Result<Vec<EvidenceId>> {
    evidence.iter().map(EvidenceId::from_evidence).collect()
}

pub(crate) fn render_generated_note(
    proposal_id: &str,
    markdown_body: &str,
    evidence_ids: &[EvidenceId],
) -> Result<RenderedGeneratedNote> {
    if evidence_ids.is_empty() {
        return Err(VaultcError::ProposalInvalid(format!(
            "generated proposal `{proposal_id}` has no evidence"
        )));
    }

    let mut body = markdown_body.trim_end().as_bytes().to_vec();
    body.push(b'\n');
    let body_hash = ContentHash::from_bytes(&body);

    let proposal_id = serde_json::to_string(proposal_id)?;
    let mut bytes = format!(
        "---\nvaultc_generated: true\nvaultc_pack_id: null\nvaultc_proposal_id: {proposal_id}\nvaultc_sources:\n"
    )
    .into_bytes();
    for evidence_id in evidence_ids {
        let quoted = serde_json::to_string(evidence_id)?;
        bytes.extend_from_slice(format!("  - {quoted}\n").as_bytes());
    }
    bytes.extend_from_slice(b"---\n\n");
    bytes.extend_from_slice(&body);
    let expected_output_hash = ContentHash::from_bytes(&bytes);

    Ok(RenderedGeneratedNote {
        bytes,
        body_hash,
        expected_output_hash,
    })
}

#[derive(Serialize)]
struct GeneratedOperationIdentity<'a> {
    plan_id: &'a str,
    proposal_id: &'a str,
    proposal_content_hash: ContentHash,
    destination: &'a str,
    body_hash: ContentHash,
    expected_output_hash: ContentHash,
    evidence_ids: &'a [EvidenceId],
}

pub(crate) fn generated_operation_id(
    plan_id: &str,
    proposal_id: &str,
    proposal_content_hash: ContentHash,
    destination: &str,
    body_hash: ContentHash,
    expected_output_hash: ContentHash,
    evidence_ids: &[EvidenceId],
) -> Result<OperationId> {
    let identity = GeneratedOperationIdentity {
        plan_id,
        proposal_id,
        proposal_content_hash,
        destination,
        body_hash,
        expected_output_hash,
        evidence_ids,
    };
    Ok(OperationId::from_hash(crate::canonical::canonical_hash(
        "vaultc:generated-operation:v1\0",
        &identity,
    )?))
}
