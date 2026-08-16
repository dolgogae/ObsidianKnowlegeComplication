use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::compile::ArtifactManifest;
use crate::config::CompilerPolicy;
use crate::error::{Result, VaultcError};
use crate::identity::ContentHash;
use crate::snapshot::validate_output_logical_path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub artifact_path: PathBuf,
    pub valid: bool,
    pub checked_files: u64,
    pub artifact_id: String,
    pub plan_id: String,
}

pub fn verify_artifact(path: &Path) -> Result<VerificationReport> {
    if path.is_dir() {
        verify_directory(path)
    } else if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("vaultpack"))
    {
        #[cfg(feature = "archives")]
        {
            let temporary = tempfile::tempdir().map_err(|error| VaultcError::io(path, error))?;
            crate::pack::extract_pack_safely(path, temporary.path())?;
            let mut report = verify_directory(temporary.path())?;
            report.artifact_path = path.to_path_buf();
            Ok(report)
        }
        #[cfg(not(feature = "archives"))]
        {
            Err(VaultcError::UnsupportedSource(path.to_path_buf()))
        }
    } else {
        Err(VaultcError::VerificationFailed(
            "artifact must be a Compiled Vault directory or .vaultpack".into(),
        ))
    }
}

// Verification is intentionally linear: later trust checks only consume data
// whose bytes and identities were established by the preceding checks.
#[allow(clippy::too_many_lines)]
pub(crate) fn verify_directory(root: &Path) -> Result<VerificationReport> {
    validate_artifact_tree(root)?;
    let manifest_path = root.join(".vaultc/manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|error| VaultcError::io(&manifest_path, error))?;
    let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        VaultcError::VerificationFailed(format!("manifest is malformed: {error}"))
    })?;
    if crate::canonical::to_canonical_json_pretty(&manifest)? != manifest_bytes {
        return Err(VaultcError::VerificationFailed(
            "manifest is not in canonical serialized form".into(),
        ));
    }
    if manifest.schema_version != 1 {
        return Err(VaultcError::VerificationFailed(format!(
            "unsupported manifest schema {}",
            manifest.schema_version
        )));
    }
    if manifest.compiler_version != env!("CARGO_PKG_VERSION") {
        return Err(VaultcError::VerificationFailed(format!(
            "artifact compiler version `{}` is not supported by verifier `{}`",
            manifest.compiler_version,
            env!("CARGO_PKG_VERSION")
        )));
    }
    let checksum_path = root.join(".vaultc/checksums.txt");
    let checksum_text = fs::read_to_string(&checksum_path)
        .map_err(|error| VaultcError::io(&checksum_path, error))?;
    let mut expected = BTreeMap::new();
    for (line_number, line) in checksum_text.lines().enumerate() {
        let Some((hash, path)) = line.split_once("  ") else {
            return Err(VaultcError::VerificationFailed(format!(
                "invalid checksum line {}",
                line_number + 1
            )));
        };
        if ContentHash::parse_hex(hash).is_err() {
            return Err(VaultcError::VerificationFailed(format!(
                "invalid checksum hash on line {}",
                line_number + 1
            )));
        }
        validate_output_logical_path(path, &CompilerPolicy::default()).map_err(|error| {
            VaultcError::VerificationFailed(format!("unsafe checksum path: {error}"))
        })?;
        if expected.insert(path.to_owned(), hash.to_owned()).is_some() {
            return Err(VaultcError::VerificationFailed(format!(
                "duplicate checksum path `{path}`"
            )));
        }
    }
    let mut canonical_checksums = String::new();
    for (path, hash) in &expected {
        writeln!(&mut canonical_checksums, "{hash}  {path}")
            .map_err(|error| VaultcError::Internal(error.to_string()))?;
    }
    if canonical_checksums != checksum_text {
        return Err(VaultcError::VerificationFailed(
            "checksum file is not sorted canonical text".into(),
        ));
    }
    let mut actual_paths = BTreeSet::new();
    for (logical, hash) in &expected {
        let file = root.join(logical);
        let metadata =
            fs::symlink_metadata(&file).map_err(|error| VaultcError::io(&file, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(VaultcError::VerificationFailed(format!(
                "artifact member `{logical}` is not a regular file"
            )));
        }
        let bytes = fs::read(&file).map_err(|error| VaultcError::io(&file, error))?;
        let actual = crate::compile::raw_sha256_hex(&bytes);
        if &actual != hash {
            return Err(VaultcError::VerificationFailed(format!(
                "checksum mismatch for `{logical}`"
            )));
        }
        actual_paths.insert(logical.clone());
    }
    let inventory = crate::compile::inventory(root, &[".vaultc/checksums.txt"])?;
    let inventory_paths: BTreeSet<_> = inventory.iter().map(|file| file.path.clone()).collect();
    if inventory_paths != actual_paths {
        return Err(VaultcError::VerificationFailed(
            "artifact contains missing or unchecksummed files".into(),
        ));
    }
    for required in [
        ".vaultc/manifest.json",
        ".vaultc/plan.json",
        ".vaultc/provenance.jsonl",
        ".vaultc/conflicts.json",
        ".vaultc/diagnostics.json",
        ".vaultc/ai-transcript.jsonl",
    ] {
        if !actual_paths.contains(required) {
            return Err(VaultcError::VerificationFailed(format!(
                "artifact is missing required audit file `{required}`"
            )));
        }
    }

    let identity_inventory =
        crate::compile::inventory(root, &[".vaultc/manifest.json", ".vaultc/checksums.txt"])?;
    if manifest.files != identity_inventory {
        return Err(VaultcError::VerificationFailed(
            "manifest inventory does not match artifact bytes".into(),
        ));
    }
    let expected_artifact_id = crate::compile::artifact_identity(
        &manifest.plan_id,
        &manifest.files,
        &manifest.approved_proposal_hashes,
    )?;
    if manifest.artifact_id != expected_artifact_id {
        return Err(VaultcError::VerificationFailed(
            "artifact ID does not match the manifest identity payload".into(),
        ));
    }

    let approved_path = root.join(".vaultc/plan.json");
    let approved_bytes =
        fs::read(&approved_path).map_err(|error| VaultcError::io(&approved_path, error))?;
    let approved: crate::approval::ApprovedPlan =
        serde_json::from_slice(&approved_bytes).map_err(|error| {
            VaultcError::VerificationFailed(format!("sealed approved plan is malformed: {error}"))
        })?;
    if crate::canonical::to_canonical_json_pretty(&approved)? != approved_bytes {
        return Err(VaultcError::VerificationFailed(
            "sealed approved plan is not canonical JSON".into(),
        ));
    }
    crate::approval::validate_approved_plan(&approved, &approved.plan.policy)
        .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
    validate_operation_output_hashes(root, &approved, &actual_paths)?;
    validate_markdown_outputs(root, &approved)?;
    validate_canvas_outputs(root, &approved, &actual_paths)?;
    validate_generated_outputs(root, &approved, &actual_paths)?;
    let snapshot_ids: Vec<_> = approved
        .plan
        .snapshots
        .iter()
        .map(|snapshot| snapshot.snapshot_id.to_string())
        .collect();
    let proposal_hashes: Vec<_> = approved
        .approved_proposals
        .iter()
        .map(|proposal| proposal.content_hash)
        .collect();
    if manifest.plan_id != approved.plan.plan_id.to_string()
        || manifest.policy_hash != approved.plan.policy.semantic_hash()?
        || manifest.projection_hash != approved.plan.projection_hash
        || manifest.source_snapshot_ids != snapshot_ids
        || manifest.approved_proposal_hashes != proposal_hashes
    {
        return Err(VaultcError::VerificationFailed(
            "manifest linkage to the sealed approved plan is invalid".into(),
        ));
    }
    validate_audit_files(root, &approved)?;

    let provenance_path = root.join(".vaultc/provenance.jsonl");
    let provenance = fs::read_to_string(&provenance_path)
        .map_err(|error| VaultcError::io(&provenance_path, error))?;
    let output_files: BTreeMap<_, _> = manifest
        .files
        .iter()
        .filter(|file| !file.path.starts_with(".vaultc/"))
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut provenance_outputs = BTreeSet::new();
    let mut actual_provenance = Vec::new();
    for (line_number, line) in provenance
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let record: crate::provenance::ProvenanceRecord =
            serde_json::from_str(line).map_err(|error| {
                VaultcError::VerificationFailed(format!(
                    "malformed provenance line {}: {error}",
                    line_number + 1
                ))
            })?;
        validate_provenance_record(root, &approved, &output_files, &record)?;
        let crate::provenance::ProvenanceRecord::Output { output_path, .. } = &record;
        if !provenance_outputs.insert(output_path.clone()) {
            return Err(VaultcError::VerificationFailed(format!(
                "duplicate provenance record for `{output_path}`"
            )));
        }
        actual_provenance.push(record);
    }
    for file in &manifest.files {
        if !file.path.starts_with(".vaultc/") && !provenance_outputs.contains(&file.path) {
            return Err(VaultcError::VerificationFailed(format!(
                "provenance missing for `{}`",
                file.path
            )));
        }
    }
    let output_hashes = manifest
        .files
        .iter()
        .filter(|file| !file.path.starts_with(".vaultc/"))
        .map(|file| {
            let path = root.join(&file.path);
            let bytes = fs::read(&path).map_err(|error| VaultcError::io(&path, error))?;
            Ok((file.path.clone(), ContentHash::from_bytes(&bytes)))
        })
        .collect::<Result<Vec<_>>>()?;
    let expected_provenance = crate::provenance::records_for_output(&approved, &output_hashes)
        .map_err(|error| {
            VaultcError::VerificationFailed(format!(
                "sealed provenance reconstruction failed: {error}"
            ))
        })?;
    if actual_provenance != expected_provenance
        || provenance.as_bytes() != crate::provenance::encode_jsonl(&expected_provenance)?
    {
        return Err(VaultcError::VerificationFailed(
            "provenance records do not exactly match the sealed derivation graph".into(),
        ));
    }
    Ok(VerificationReport {
        artifact_path: root.to_path_buf(),
        valid: true,
        checked_files: expected.len() as u64,
        artifact_id: manifest.artifact_id.hex(),
        plan_id: manifest.plan_id,
    })
}

fn validate_audit_files(root: &Path, approved: &crate::approval::ApprovedPlan) -> Result<()> {
    let mut expected_transcript = Vec::new();
    for record in &approved.transcript {
        expected_transcript.extend(crate::canonical::to_canonical_json(record)?);
        expected_transcript.push(b'\n');
    }
    let transcript_path = root.join(".vaultc/ai-transcript.jsonl");
    let transcript =
        fs::read(&transcript_path).map_err(|error| VaultcError::io(&transcript_path, error))?;
    if transcript != expected_transcript {
        return Err(VaultcError::VerificationFailed(
            "standalone provider transcript does not match the approved plan".into(),
        ));
    }

    let conflicts_path = root.join(".vaultc/conflicts.json");
    let expected_conflicts = crate::canonical::to_canonical_json_pretty(&serde_json::json!({
        "conflicts": &approved.plan.conflicts,
        "decisions": &approved.conflict_decisions,
    }))?;
    if fs::read(&conflicts_path).map_err(|error| VaultcError::io(&conflicts_path, error))?
        != expected_conflicts
    {
        return Err(VaultcError::VerificationFailed(
            "standalone conflict audit does not match the approved plan".into(),
        ));
    }

    let diagnostics_path = root.join(".vaultc/diagnostics.json");
    let expected_diagnostics =
        crate::canonical::to_canonical_json_pretty(&approved.plan.diagnostics)?;
    if fs::read(&diagnostics_path).map_err(|error| VaultcError::io(&diagnostics_path, error))?
        != expected_diagnostics
    {
        return Err(VaultcError::VerificationFailed(
            "standalone diagnostics audit does not match the sealed plan".into(),
        ));
    }
    Ok(())
}

fn validate_operation_output_hashes(
    root: &Path,
    approved: &crate::approval::ApprovedPlan,
    artifact_paths: &BTreeSet<String>,
) -> Result<()> {
    for operation in &approved.plan.operations {
        let (destination, expected_output_hash) = match operation {
            crate::plan::OutputOperation::Copy {
                destination,
                expected_hash,
                ..
            } => (destination, expected_hash),
            crate::plan::OutputOperation::RewriteMarkdown {
                destination,
                expected_output_hash,
                ..
            }
            | crate::plan::OutputOperation::RewriteCanvas {
                destination,
                expected_output_hash,
                ..
            } => (destination, expected_output_hash),
        };
        if !artifact_paths.contains(destination) {
            return Err(VaultcError::VerificationFailed(format!(
                "sealed output operation `{destination}` is missing from the checksummed artifact"
            )));
        }
        let path = root.join(destination);
        let actual = fs::read(&path).map_err(|error| VaultcError::io(&path, error))?;
        if ContentHash::from_bytes(&actual) != *expected_output_hash {
            return Err(VaultcError::VerificationFailed(format!(
                "output hash does not match the sealed operation for `{destination}`"
            )));
        }
    }
    Ok(())
}

fn validate_generated_outputs(
    root: &Path,
    approved: &crate::approval::ApprovedPlan,
    artifact_paths: &BTreeSet<String>,
) -> Result<()> {
    for proposal in &approved.approved_proposals {
        let rebuilt = crate::approval::proposal_materialization(
            &approved.plan,
            &proposal.proposal,
            proposal.content_hash,
        )
        .map_err(|error| {
            VaultcError::VerificationFailed(format!(
                "generated proposal materialization cannot be reconstructed: {error}"
            ))
        })?;
        if rebuilt != proposal.materialization {
            return Err(VaultcError::VerificationFailed(format!(
                "generated proposal `{}` materialization is stale",
                proposal.proposal.proposal_id
            )));
        }
        match (&proposal.proposal.kind, &proposal.materialization) {
            (
                vaultc_protocol::ProposalKind::CreateGeneratedNote { markdown_body, .. },
                crate::approval::ProposalMaterialization::GeneratedNote {
                    destination,
                    body_hash,
                    expected_output_hash,
                    evidence_ids,
                    ..
                },
            ) => {
                if !artifact_paths.contains(destination) {
                    return Err(VaultcError::VerificationFailed(format!(
                        "generated output `{destination}` is missing from the checksummed artifact"
                    )));
                }
                let rendered = crate::generated::render_generated_note(
                    &proposal.proposal.proposal_id,
                    markdown_body,
                    evidence_ids,
                )
                .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
                let output_path = root.join(destination);
                let actual =
                    fs::read(&output_path).map_err(|error| VaultcError::io(&output_path, error))?;
                if rendered.body_hash != *body_hash
                    || rendered.expected_output_hash != *expected_output_hash
                    || actual != rendered.bytes
                    || ContentHash::from_bytes(&actual) != *expected_output_hash
                {
                    return Err(VaultcError::VerificationFailed(format!(
                        "generated output `{destination}` does not match its sealed materialization"
                    )));
                }
            }
            (
                vaultc_protocol::ProposalKind::ExplainConflict { .. },
                crate::approval::ProposalMaterialization::NonMaterializing,
            ) => {}
            _ => {
                return Err(VaultcError::VerificationFailed(format!(
                    "proposal `{}` kind does not match its materialization",
                    proposal.proposal.proposal_id
                )));
            }
        }
    }
    Ok(())
}

fn validate_markdown_outputs(root: &Path, approved: &crate::approval::ApprovedPlan) -> Result<()> {
    for operation in &approved.plan.operations {
        let crate::plan::OutputOperation::RewriteMarkdown {
            source_id,
            snapshot_id,
            source_path,
            destination,
            expected_hash,
            replacements,
            ..
        } = operation
        else {
            continue;
        };
        let document = approved
            .plan
            .workspace
            .documents
            .values()
            .find(|document| {
                &document.source_file.source_id == source_id
                    && document.source_file.snapshot_id == *snapshot_id
                    && document.source_file.logical_path == *source_path
            })
            .ok_or_else(|| {
                VaultcError::VerificationFailed(format!(
                    "Markdown operation source `{source_id}/{source_path}` is absent from the sealed workspace"
                ))
            })?;
        if document.source_file.content_hash != *expected_hash {
            return Err(VaultcError::VerificationFailed(format!(
                "Markdown operation source hash is stale for `{destination}`"
            )));
        }
        let planned = crate::plan::planned_markdown_replacements(
            document,
            &approved.plan.output_paths,
            &approved.plan.asset_output_paths,
        )
        .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
        if &planned != replacements {
            return Err(VaultcError::VerificationFailed(format!(
                "Markdown rewrite recipe does not match sealed link resolution for `{destination}`"
            )));
        }
        let path = root.join(destination);
        let actual = fs::read(&path).map_err(|error| VaultcError::io(&path, error))?;
        reconstruct_markdown_source(document, &actual, replacements)?;
        validate_rewritten_markdown_links(
            document,
            destination,
            &actual,
            replacements,
            &approved.plan.policy,
        )?;
    }
    Ok(())
}

fn reconstruct_markdown_source(
    document: &crate::ir::Document,
    output: &[u8],
    replacements: &[crate::plan::RewriteReplacement],
) -> Result<()> {
    let source_len = usize::try_from(document.source_file.byte_len).map_err(|_| {
        VaultcError::VerificationFailed("sealed Markdown source length overflows usize".into())
    })?;
    let mut reconstructed = Vec::with_capacity(source_len);
    let mut source_cursor = 0_usize;
    let mut output_cursor = 0_usize;
    for replacement in replacements {
        let start = usize::try_from(replacement.span.byte_start).map_err(|_| {
            VaultcError::VerificationFailed("Markdown replacement start overflows usize".into())
        })?;
        let end = usize::try_from(replacement.span.byte_end).map_err(|_| {
            VaultcError::VerificationFailed("Markdown replacement end overflows usize".into())
        })?;
        if start < source_cursor || start >= end || end > source_len {
            return Err(VaultcError::VerificationFailed(
                "Markdown replacement spans overlap or are out of bounds".into(),
            ));
        }
        let link = document
            .links
            .iter()
            .find(|link| link.span == replacement.span)
            .ok_or_else(|| {
                VaultcError::VerificationFailed(
                    "Markdown replacement is not bound to a sealed link".into(),
                )
            })?;
        if end - start != link.raw_target.len() {
            return Err(VaultcError::VerificationFailed(format!(
                "Markdown link {} raw target length does not match its source span",
                link.link_id
            )));
        }
        let unchanged_len = start - source_cursor;
        let unchanged_end = output_cursor.checked_add(unchanged_len).ok_or_else(|| {
            VaultcError::VerificationFailed("Markdown output offset overflow".into())
        })?;
        let replacement_end = unchanged_end
            .checked_add(replacement.replacement.len())
            .ok_or_else(|| {
                VaultcError::VerificationFailed("Markdown output offset overflow".into())
            })?;
        if output.get(unchanged_end..replacement_end) != Some(replacement.replacement.as_bytes()) {
            return Err(VaultcError::VerificationFailed(format!(
                "Markdown output does not contain its sealed replacement for link {}",
                link.link_id
            )));
        }
        let unchanged = output.get(output_cursor..unchanged_end).ok_or_else(|| {
            VaultcError::VerificationFailed("Markdown output is truncated".into())
        })?;
        reconstructed.extend_from_slice(unchanged);
        reconstructed.extend_from_slice(link.raw_target.as_bytes());
        source_cursor = end;
        output_cursor = replacement_end;
    }
    let remaining = source_len.checked_sub(source_cursor).ok_or_else(|| {
        VaultcError::VerificationFailed("Markdown source offset underflow".into())
    })?;
    if output.len().checked_sub(output_cursor) != Some(remaining) {
        return Err(VaultcError::VerificationFailed(
            "Markdown output length is inconsistent with its sealed replacements".into(),
        ));
    }
    reconstructed.extend_from_slice(&output[output_cursor..]);
    if reconstructed.len() != source_len
        || ContentHash::from_bytes(&reconstructed) != document.source_file.content_hash
        || std::str::from_utf8(&reconstructed).is_err()
    {
        return Err(VaultcError::VerificationFailed(format!(
            "Markdown output cannot reconstruct sealed source `{}/{}`",
            document.source_file.source_id, document.source_file.logical_path
        )));
    }
    Ok(())
}

fn validate_rewritten_markdown_links(
    document: &crate::ir::Document,
    destination: &str,
    output: &[u8],
    replacements: &[crate::plan::RewriteReplacement],
    policy: &CompilerPolicy,
) -> Result<()> {
    let mut output_file = document.source_file.clone();
    destination.clone_into(&mut output_file.logical_path);
    output_file.byte_len = u64::try_from(output.len()).map_err(|_| {
        VaultcError::VerificationFailed("rewritten Markdown length overflows u64".into())
    })?;
    output_file.content_hash = ContentHash::from_bytes(output);
    output_file.file_id = crate::identity::SourceFileId::from_file(
        destination,
        crate::ir::FileKind::Markdown.media_family(),
        output_file.content_hash,
    );
    let mut diagnostics = Vec::new();
    let reparsed = crate::parse::parse_markdown(output_file, output, policy, &mut diagnostics)
        .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
    if reparsed.links.len() != document.links.len() {
        return Err(VaultcError::VerificationFailed(format!(
            "rewritten Markdown `{destination}` changed the number of parsed links"
        )));
    }
    for (original, reparsed) in document.links.iter().zip(&reparsed.links) {
        let expected_target = replacements
            .iter()
            .find(|replacement| replacement.span == original.span)
            .map_or(original.raw_target.as_str(), |replacement| {
                replacement.replacement.as_str()
            });
        if reparsed.raw_target != expected_target
            || reparsed.syntax != original.syntax
            || reparsed.heading != original.heading
            || reparsed.block_id != original.block_id
            || reparsed.display != original.display
            || reparsed.embed != original.embed
        {
            return Err(VaultcError::VerificationFailed(format!(
                "rewritten Markdown `{destination}` changed unintended link semantics"
            )));
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "copy and rewrite checks share one exhaustive Canvas semantic closure invariant"
)]
fn validate_canvas_outputs(
    root: &Path,
    approved: &crate::approval::ApprovedPlan,
    artifact_paths: &BTreeSet<String>,
) -> Result<()> {
    for operation in &approved.plan.operations {
        let (source_id, snapshot_id, source_path, destination, expected_source_hash, rewrites) =
            match operation {
                crate::plan::OutputOperation::Copy {
                    source_id,
                    snapshot_id,
                    source_path,
                    destination,
                    expected_hash,
                    kind: crate::ir::FileKind::Canvas,
                    ..
                } => (
                    source_id,
                    snapshot_id,
                    source_path,
                    destination,
                    expected_hash,
                    None,
                ),
                crate::plan::OutputOperation::RewriteCanvas {
                    source_id,
                    snapshot_id,
                    source_path,
                    destination,
                    expected_hash,
                    rewrites,
                    ..
                } => (
                    source_id,
                    snapshot_id,
                    source_path,
                    destination,
                    expected_hash,
                    Some(rewrites.as_slice()),
                ),
                crate::plan::OutputOperation::Copy { .. }
                | crate::plan::OutputOperation::RewriteMarkdown { .. } => continue,
            };
        let canvas = approved
            .plan
            .workspace
            .canvases
            .values()
            .find(|canvas| {
                &canvas.source_file.source_id == source_id
                    && canvas.source_file.snapshot_id == *snapshot_id
                    && canvas.source_file.logical_path == *source_path
            })
            .ok_or_else(|| {
                VaultcError::VerificationFailed(format!(
                    "Canvas operation source `{source_id}/{source_path}` is absent from the sealed workspace"
                ))
            })?;
        if canvas.source_file.content_hash != *expected_source_hash {
            return Err(VaultcError::VerificationFailed(format!(
                "Canvas operation source hash is stale for `{destination}`"
            )));
        }
        for reference in &canvas.file_references {
            if let crate::ir::CanvasReferenceResolution::Resolved { target } = &reference.resolution
            {
                let target_path = match target {
                    crate::ir::CanvasReferenceTarget::Document(document_id) => {
                        approved.plan.output_paths.get(document_id)
                    }
                    crate::ir::CanvasReferenceTarget::Asset(asset_id) => {
                        approved.plan.asset_output_paths.get(asset_id)
                    }
                    crate::ir::CanvasReferenceTarget::Canvas(canvas_id) => {
                        approved.plan.canvas_output_paths.get(canvas_id)
                    }
                    crate::ir::CanvasReferenceTarget::Base(base_artifact_id) => {
                        approved.plan.base_output_paths.get(base_artifact_id)
                    }
                }
                .ok_or_else(|| {
                    VaultcError::VerificationFailed(format!(
                        "Canvas reference target for `{destination}` has no sealed output path"
                    ))
                })?;
                if !artifact_paths.contains(target_path) {
                    return Err(VaultcError::VerificationFailed(format!(
                        "Canvas reference target `{target_path}` is missing from the checksummed artifact"
                    )));
                }
            }
        }
        let path = root.join(destination);
        let actual = fs::read(&path).map_err(|error| VaultcError::io(&path, error))?;
        if let Some(rewrites) = rewrites {
            let expected = crate::plan::render_rewritten_canvas(canvas, rewrites)
                .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
            if actual != expected {
                return Err(VaultcError::VerificationFailed(format!(
                    "Canvas output bytes do not match the sealed rewrite for `{destination}`"
                )));
            }
        } else {
            let (actual_value, actual_references) =
                crate::parse::parse_canvas_json(source_path, &actual)
                    .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
            if actual_value != canvas.value
                || actual_references.len() != canvas.file_references.len()
                || actual_references
                    .iter()
                    .zip(&canvas.file_references)
                    .any(|(actual, sealed)| {
                        actual.node_id != sealed.node_id || actual.raw_path != sealed.raw_path
                    })
            {
                return Err(VaultcError::VerificationFailed(format!(
                    "copied Canvas semantics do not match the sealed workspace for `{destination}`"
                )));
            }
        }
    }
    Ok(())
}

fn validate_artifact_tree(root: &Path) -> Result<()> {
    for entry in ignore::WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(false)
        .build()
    {
        let entry = entry.map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
        if entry.path() == root {
            continue;
        }
        let relative = entry.path().strip_prefix(root).map_err(|_| {
            VaultcError::VerificationFailed("artifact entry escaped its root".into())
        })?;
        let logical = relative
            .components()
            .map(|component| {
                component.as_os_str().to_str().ok_or_else(|| {
                    VaultcError::VerificationFailed("artifact contains a non-UTF-8 path".into())
                })
            })
            .collect::<Result<Vec<_>>>()?
            .join("/");
        validate_output_logical_path(&logical, &CompilerPolicy::default()).map_err(|error| {
            VaultcError::VerificationFailed(format!("unsafe artifact path: {error}"))
        })?;
        let file_type = entry.file_type().ok_or_else(|| {
            VaultcError::VerificationFailed(format!(
                "artifact entry `{logical}` has an unknown file type"
            ))
        })?;
        if file_type.is_symlink() || !(file_type.is_file() || file_type.is_dir()) {
            return Err(VaultcError::VerificationFailed(format!(
                "artifact entry `{logical}` is not a regular file or directory"
            )));
        }
    }
    Ok(())
}

// Keep all closure checks for a record together so new provenance fields cannot
// accidentally be accepted without linkage validation.
#[allow(clippy::too_many_lines)]
fn validate_provenance_record(
    root: &Path,
    approved: &crate::approval::ApprovedPlan,
    output_files: &BTreeMap<&str, &crate::compile::ManifestFile>,
    record: &crate::provenance::ProvenanceRecord,
) -> Result<()> {
    let crate::provenance::ProvenanceRecord::Output {
        output_path,
        output_hash,
        operation_id,
        source_snapshot_ids,
        source_document_ids,
        sources,
        proposal_id,
        evidence,
        evidence_ids,
    } = record;
    let file = output_files.get(output_path.as_str()).ok_or_else(|| {
        VaultcError::VerificationFailed(format!(
            "provenance references unknown output `{output_path}`"
        ))
    })?;
    let output_bytes = fs::read(root.join(output_path))
        .map_err(|error| VaultcError::io(root.join(output_path), error))?;
    if output_bytes.len() as u64 != file.byte_len
        || *output_hash != ContentHash::from_bytes(&output_bytes)
        || sources.is_empty()
    {
        return Err(VaultcError::VerificationFailed(format!(
            "provenance hash or source closure is invalid for `{output_path}`"
        )));
    }
    let mut derived_snapshots: Vec<_> = sources
        .iter()
        .map(|source| source.snapshot_id.clone())
        .collect();
    derived_snapshots.sort();
    derived_snapshots.dedup();
    let mut derived_documents: Vec<_> = sources
        .iter()
        .filter_map(|source| source.source_document_id.clone())
        .collect();
    derived_documents.sort();
    derived_documents.dedup();
    if &derived_snapshots != source_snapshot_ids || &derived_documents != source_document_ids {
        return Err(VaultcError::VerificationFailed(format!(
            "provenance summary is not closed for `{output_path}`"
        )));
    }
    for source in sources {
        let snapshot = approved
            .plan
            .snapshots
            .iter()
            .find(|snapshot| snapshot.snapshot_id.to_string() == source.snapshot_id)
            .ok_or_else(|| {
                VaultcError::VerificationFailed(format!(
                    "provenance for `{output_path}` references an unknown snapshot"
                ))
            })?;
        let source_file = snapshot
            .files
            .iter()
            .find(|file| file.file_id.to_string() == source.source_file_id)
            .ok_or_else(|| {
                VaultcError::VerificationFailed(format!(
                    "provenance for `{output_path}` references an unknown source file"
                ))
            })?;
        if source.source_id != source_file.source_id.to_string()
            || source.source_path != source_file.logical_path
            || source.source_content_hash != source_file.content_hash
        {
            return Err(VaultcError::VerificationFailed(format!(
                "provenance source identity is stale for `{output_path}`"
            )));
        }
        if let Some(document_id) = &source.source_document_id {
            let document = approved
                .plan
                .workspace
                .documents
                .values()
                .find(|document| document.document_id.to_string() == *document_id)
                .ok_or_else(|| {
                    VaultcError::VerificationFailed(format!(
                        "provenance for `{output_path}` references an unknown document"
                    ))
                })?;
            if document.source_file.file_id != source_file.file_id {
                return Err(VaultcError::VerificationFailed(format!(
                    "provenance document/file linkage is invalid for `{output_path}`"
                )));
            }
        }
        match (source.byte_start, source.byte_end) {
            (Some(start), Some(end)) if start <= end && end <= source_file.byte_len => {}
            (None, None) => {}
            _ => {
                return Err(VaultcError::VerificationFailed(format!(
                    "provenance span is invalid for `{output_path}`"
                )));
            }
        }
    }
    if let Some(proposal_id) = proposal_id {
        let proposal = approved
            .approved_proposals
            .iter()
            .find(|proposal| proposal.proposal.proposal_id == *proposal_id)
            .ok_or_else(|| {
                VaultcError::VerificationFailed(format!(
                    "provenance for `{output_path}` references an unapproved proposal"
                ))
            })?;
        let crate::approval::ProposalMaterialization::GeneratedNote {
            destination,
            expected_output_hash,
            evidence_ids: sealed_evidence_ids,
            operation_id: sealed_operation_id,
            ..
        } = &proposal.materialization
        else {
            return Err(VaultcError::VerificationFailed(format!(
                "provenance for `{output_path}` references a non-materializing proposal"
            )));
        };
        if proposal.proposal.evidence.is_empty()
            || proposal.proposal.evidence != *evidence
            || sealed_evidence_ids != evidence_ids
            || destination != output_path
            || expected_output_hash != output_hash
            || sealed_operation_id.to_string() != *operation_id
            || sources.len() != evidence.len()
        {
            return Err(VaultcError::VerificationFailed(format!(
                "generated provenance linkage is invalid for `{output_path}`"
            )));
        }
        for ((source, evidence), evidence_id) in sources.iter().zip(evidence).zip(evidence_ids) {
            let derived_evidence_id = crate::identity::EvidenceId::from_evidence(evidence)
                .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
            let evidence_hash = ContentHash::parse_hex(&evidence.content_hash)
                .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
            if derived_evidence_id != *evidence_id
                || source.snapshot_id != evidence.snapshot_id
                || source.source_document_id.as_deref() != Some(evidence.document_id.as_str())
                || source.evidence_content_hash != Some(evidence_hash)
                || source.byte_start != evidence.byte_start
                || source.byte_end != evidence.byte_end
            {
                return Err(VaultcError::VerificationFailed(format!(
                    "generated evidence/source identity is invalid for `{output_path}`"
                )));
            }
        }
    } else {
        if !evidence.is_empty() || !evidence_ids.is_empty() {
            return Err(VaultcError::VerificationFailed(format!(
                "non-generated output `{output_path}` contains proposal evidence"
            )));
        }
        let operation = approved
            .plan
            .operations
            .iter()
            .find(|operation| operation.destination() == output_path)
            .ok_or_else(|| {
                VaultcError::VerificationFailed(format!(
                    "provenance for `{output_path}` has no sealed operation"
                ))
            })?;
        if operation.operation_id().to_string() != *operation_id {
            return Err(VaultcError::VerificationFailed(format!(
                "provenance operation ID is stale for `{output_path}`"
            )));
        }
    }
    Ok(())
}
