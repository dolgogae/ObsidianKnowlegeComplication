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
    validate_canvas_outputs(root, &approved, &actual_paths)?;
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
    let expected_provenance = crate::provenance::records_for_output(&approved, &output_hashes);
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

#[allow(
    clippy::too_many_lines,
    reason = "copy and rewrite checks share one exhaustive Canvas operation/output closure invariant"
)]
fn validate_canvas_outputs(
    root: &Path,
    approved: &crate::approval::ApprovedPlan,
    artifact_paths: &BTreeSet<String>,
) -> Result<()> {
    for operation in &approved.plan.operations {
        let (
            source_id,
            snapshot_id,
            source_path,
            destination,
            expected_source_hash,
            rewrites,
            expected_output_hash,
        ) = match operation {
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
                *expected_hash,
            ),
            crate::plan::OutputOperation::RewriteCanvas {
                source_id,
                snapshot_id,
                source_path,
                destination,
                expected_hash,
                expected_output_hash,
                rewrites,
                ..
            } => (
                source_id,
                snapshot_id,
                source_path,
                destination,
                expected_hash,
                Some(rewrites.as_slice()),
                *expected_output_hash,
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
        if ContentHash::from_bytes(&actual) != expected_output_hash {
            return Err(VaultcError::VerificationFailed(format!(
                "Canvas output hash does not match the sealed operation for `{destination}`"
            )));
        }
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
        if proposal.proposal.evidence.is_empty()
            || proposal.proposal.evidence != *evidence
            || crate::provenance::generated_output_path(&proposal.proposal).as_deref()
                != Some(output_path)
            || crate::provenance::generated_operation_id(proposal_id).to_string() != *operation_id
        {
            return Err(VaultcError::VerificationFailed(format!(
                "generated provenance linkage is invalid for `{output_path}`"
            )));
        }
        validate_generated_frontmatter(root, output_path, proposal_id, evidence.len())?;
    } else {
        if !evidence.is_empty() {
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

fn validate_generated_frontmatter(
    root: &Path,
    output_path: &str,
    proposal_id: &str,
    evidence_count: usize,
) -> Result<()> {
    let text = fs::read_to_string(root.join(output_path)).map_err(|error| {
        VaultcError::VerificationFailed(format!(
            "generated note `{output_path}` is not readable UTF-8: {error}"
        ))
    })?;
    let Some(rest) = text.strip_prefix("---\n") else {
        return Err(VaultcError::VerificationFailed(format!(
            "generated note `{output_path}` lacks canonical frontmatter"
        )));
    };
    let Some((yaml, _)) = rest.split_once("\n---\n") else {
        return Err(VaultcError::VerificationFailed(format!(
            "generated note `{output_path}` has unterminated frontmatter"
        )));
    };
    let value: serde_json::Value = serde_json::to_value(
        serde_yaml_ng::from_str::<serde_yaml_ng::Value>(yaml).map_err(|error| {
            VaultcError::VerificationFailed(format!(
                "generated note `{output_path}` frontmatter is malformed: {error}"
            ))
        })?,
    )
    .map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
    if value
        .get("vaultc_generated")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || value
            .get("vaultc_proposal_id")
            .and_then(serde_json::Value::as_str)
            != Some(proposal_id)
        || value
            .get("vaultc_sources")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            != Some(evidence_count)
    {
        return Err(VaultcError::VerificationFailed(format!(
            "generated note `{output_path}` frontmatter is not bound to its proposal"
        )));
    }
    Ok(())
}
