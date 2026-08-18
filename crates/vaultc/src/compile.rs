use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vaultc_protocol::ProposalKind;

use crate::approval::ApprovedPlan;
use crate::canonical::{to_canonical_json, to_canonical_json_pretty};
use crate::config::CompilerPolicy;
use crate::error::{Result, VaultcError};
use crate::identity::ContentHash;
use crate::plan::OutputOperation;
use crate::snapshot::{read_source_entry, validate_output_logical_path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompileOptions {
    pub create_pack: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledArtifact {
    pub path: PathBuf,
    pub artifact_id: ContentHash,
    pub plan_id: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub byte_len: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub compiler_version: String,
    pub provenance_schema_version: u32,
    pub provenance_graph_hash: ContentHash,
    pub artifact_id: ContentHash,
    pub plan_id: String,
    pub policy_hash: ContentHash,
    pub projection_hash: ContentHash,
    pub source_snapshot_ids: Vec<String>,
    pub approved_proposal_hashes: Vec<ContentHash>,
    pub files: Vec<ManifestFile>,
}

pub fn compile_plan(
    approved: &ApprovedPlan,
    destination: &Path,
    policy: &CompilerPolicy,
) -> Result<CompiledArtifact> {
    compile_plan_with_options(approved, destination, policy, &CompileOptions::default())
}

pub fn compile_plan_with_options(
    approved: &ApprovedPlan,
    destination: &Path,
    policy: &CompilerPolicy,
    options: &CompileOptions,
) -> Result<CompiledArtifact> {
    validate_compile_request(approved, destination, policy)?;
    if let Some(pack) = &options.create_pack {
        crate::pack::preflight_pack_destination(destination, pack)?;
    }

    let artifact = compile_validated_plan(approved, destination, policy)?;
    if let Some(pack) = &options.create_pack
        && let Err(source) = crate::pack::create_pack(destination, pack, policy.output.zstd_level)
    {
        return Err(VaultcError::PackPublicationAfterCompile {
            compiled_vault: destination.to_path_buf(),
            pack: pack.clone(),
            source: Box::new(source),
        });
    }
    Ok(artifact)
}

fn validate_compile_request(
    approved: &ApprovedPlan,
    destination: &Path,
    policy: &CompilerPolicy,
) -> Result<()> {
    // Publishing over an existing destination is never safe: on some platforms
    // `rename` may replace an existing directory. Keep the policy field for
    // schema compatibility, but V1 compilation is unconditionally create-new.
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err(VaultcError::OutputExists(destination.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(VaultcError::io(destination, error)),
    }
    crate::approval::validate_approved_plan(approved, policy)?;
    if !approved.conflict_decisions_complete {
        return Err(VaultcError::ApprovalStale(
            "conflict decisions are incomplete".into(),
        ));
    }
    if policy.output.deny_warnings
        && approved
            .plan
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Warning)
    {
        return Err(VaultcError::ApprovalStale(
            "compilation policy denies plan warnings".into(),
        ));
    }
    if approved.plan.policy.semantic_hash()? != policy.semantic_hash()? {
        return Err(VaultcError::PlanStale(
            "approved plan policy does not match compiler policy".into(),
        ));
    }
    Ok(())
}

fn compile_validated_plan(
    approved: &ApprovedPlan,
    destination: &Path,
    policy: &CompilerPolicy,
) -> Result<CompiledArtifact> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| VaultcError::io(parent, error))?;
    let staging = tempfile::Builder::new()
        .prefix(".vaultc-staging-")
        .tempdir_in(parent)
        .map_err(|error| VaultcError::io(parent, error))?;
    let staging_path = staging.path().to_path_buf();

    let result = compile_into(approved, &staging_path, policy);
    let artifact = match result {
        Ok(artifact) => artifact,
        Err(error) => {
            if policy.output.retain_failed_staging {
                mark_incomplete(&staging_path, &error);
                let _retained = staging.keep();
            }
            return Err(error);
        }
    };
    let kept = staging.keep();
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            let error = VaultcError::OutputExists(destination.to_path_buf());
            if policy.output.retain_failed_staging {
                mark_incomplete(&kept, &error);
            } else {
                let _ = fs::remove_dir_all(&kept);
            }
            return Err(error);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            let error = VaultcError::io(destination, source);
            if policy.output.retain_failed_staging {
                mark_incomplete(&kept, &error);
            } else {
                let _ = fs::remove_dir_all(&kept);
            }
            return Err(error);
        }
    }
    if let Err(error) = fs::rename(&kept, destination) {
        let error = VaultcError::io(destination, error);
        if policy.output.retain_failed_staging {
            mark_incomplete(&kept, &error);
        } else {
            let _ = fs::remove_dir_all(&kept);
        }
        return Err(error);
    }
    if let Err(source) = sync_parent(parent) {
        return Err(VaultcError::PublishedButDurabilityUncertain {
            path: destination.to_path_buf(),
            source,
        });
    }
    Ok(CompiledArtifact {
        path: destination.to_path_buf(),
        ..artifact
    })
}

// The order is the format's transaction boundary; keeping it visible in one
// function makes accidental reordering of audit/identity writes reviewable.
#[allow(clippy::too_many_lines)]
fn compile_into(
    approved: &ApprovedPlan,
    root: &Path,
    policy: &CompilerPolicy,
) -> Result<CompiledArtifact> {
    let mut written = BTreeSet::new();
    for operation in &approved.plan.operations {
        let (source_id, snapshot_id, source_path, destination, expected_hash) = match operation {
            OutputOperation::Copy {
                source_id,
                snapshot_id,
                source_path,
                destination,
                expected_hash,
                ..
            }
            | OutputOperation::RewriteMarkdown {
                source_id,
                snapshot_id,
                source_path,
                destination,
                expected_hash,
                ..
            }
            | OutputOperation::RewriteCanvas {
                source_id,
                snapshot_id,
                source_path,
                destination,
                expected_hash,
                ..
            } => (
                source_id,
                snapshot_id,
                source_path,
                destination,
                expected_hash,
            ),
        };
        validate_output_logical_path(destination, policy)?;
        if !written.insert(destination.clone()) {
            return Err(VaultcError::Internal(format!(
                "duplicate output operation for `{destination}`"
            )));
        }
        let source = approved.plan.source(source_id).ok_or_else(|| {
            VaultcError::PlanStale(format!("source `{source_id}` is missing from plan"))
        })?;
        let source_file = approved
            .plan
            .snapshots
            .iter()
            .find(|snapshot| {
                &snapshot.source_id == source_id && snapshot.snapshot_id == *snapshot_id
            })
            .and_then(|snapshot| {
                snapshot
                    .files
                    .iter()
                    .find(|file| file.logical_path == *source_path)
            })
            .ok_or_else(|| {
                VaultcError::PlanStale(format!(
                    "source file `{source_id}/{source_path}` is missing from plan"
                ))
            })?;
        let bytes = read_source_entry(source, source_file, policy)?;
        if ContentHash::from_bytes(&bytes) != *expected_hash {
            return Err(VaultcError::IdentityMismatch(format!(
                "source `{source_id}/{source_path}` changed after planning"
            )));
        }
        let output = match operation {
            OutputOperation::Copy { .. } => bytes,
            OutputOperation::RewriteMarkdown {
                expected_output_hash,
                replacements,
                ..
            } => {
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
                        VaultcError::PlanStale(format!(
                            "Markdown rewrite source `{source_id}/{source_path}` is absent from the sealed workspace"
                        ))
                    })?;
                let planned = crate::plan::planned_markdown_replacements(
                    document,
                    &approved.plan.output_paths,
                    &approved.plan.asset_output_paths,
                )?;
                if &planned != replacements {
                    return Err(VaultcError::PlanStale(format!(
                        "Markdown rewrite recipe is stale for `{destination}`"
                    )));
                }
                validate_markdown_source_links(document, &bytes)?;
                let output = apply_replacements(bytes, replacements)?;
                if ContentHash::from_bytes(&output) != *expected_output_hash {
                    return Err(VaultcError::PlanStale(format!(
                        "Markdown rewrite output hash is stale for `{destination}`"
                    )));
                }
                output
            }
            OutputOperation::RewriteCanvas {
                expected_output_hash,
                rewrites,
                ..
            } => {
                let canvas = approved
                    .plan
                    .workspace
                    .canvases
                    .values()
                    .find(|canvas| {
                        &canvas.source_file.source_id == source_id
                            && canvas.source_file.logical_path == *source_path
                    })
                    .ok_or_else(|| {
                        VaultcError::PlanStale(format!(
                            "Canvas rewrite source `{source_id}/{source_path}` is absent from the sealed workspace"
                        ))
                    })?;
                let (source_value, source_references) =
                    crate::parse::parse_canvas_json(source_path, &bytes)?;
                if source_value != canvas.value
                    || source_references.len() != canvas.file_references.len()
                    || source_references.iter().zip(&canvas.file_references).any(
                        |(source, sealed)| {
                            source.node_id != sealed.node_id || source.raw_path != sealed.raw_path
                        },
                    )
                {
                    return Err(VaultcError::PlanStale(format!(
                        "Canvas source `{source_id}/{source_path}` does not match its sealed semantic value"
                    )));
                }
                let output = crate::plan::render_rewritten_canvas(canvas, rewrites)?;
                if ContentHash::from_bytes(&output) != *expected_output_hash {
                    return Err(VaultcError::PlanStale(format!(
                        "Canvas rewrite output hash is stale for `{destination}`"
                    )));
                }
                output
            }
        };
        write_new_file(root, destination, &output)?;
    }

    for proposal in &approved.approved_proposals {
        match (&proposal.proposal.kind, &proposal.materialization) {
            (
                ProposalKind::CreateGeneratedNote { markdown_body, .. },
                crate::approval::ProposalMaterialization::GeneratedNote {
                    destination,
                    body_hash,
                    expected_output_hash,
                    evidence_ids,
                    ..
                },
            ) => {
                let rebuilt = crate::approval::proposal_materialization(
                    &approved.plan,
                    &proposal.proposal,
                    proposal.content_hash,
                )?;
                if rebuilt != proposal.materialization {
                    return Err(VaultcError::ApprovalStale(format!(
                        "generated proposal `{}` materialization is stale",
                        proposal.proposal.proposal_id
                    )));
                }
                validate_output_logical_path(destination, policy)?;
                if !written.insert(destination.clone()) {
                    return Err(VaultcError::ProposalInvalid(format!(
                        "generated output collision at `{destination}`"
                    )));
                }
                let note = crate::generated::render_generated_note(
                    &proposal.proposal.proposal_id,
                    markdown_body,
                    evidence_ids,
                )?;
                if note.body_hash != *body_hash
                    || note.expected_output_hash != *expected_output_hash
                {
                    return Err(VaultcError::ApprovalStale(format!(
                        "generated proposal `{}` output commitment is stale",
                        proposal.proposal.proposal_id
                    )));
                }
                write_new_file(root, destination, &note.bytes)?;
            }
            (
                ProposalKind::ExplainConflict { .. },
                crate::approval::ProposalMaterialization::NonMaterializing,
            ) => {}
            _ => {
                return Err(VaultcError::ApprovalStale(format!(
                    "proposal `{}` kind does not match its materialization",
                    proposal.proposal.proposal_id
                )));
            }
        }
    }

    let audit = root.join(".vaultc");
    fs::create_dir_all(&audit).map_err(|error| VaultcError::io(&audit, error))?;
    write_new_file(root, ".vaultc/plan.json", &audit_plan_bytes(approved)?)?;
    write_new_file(
        root,
        ".vaultc/conflicts.json",
        &to_canonical_json_pretty(&serde_json::json!({
            "conflicts": &approved.plan.conflicts,
            "decisions": &approved.conflict_decisions,
        }))?,
    )?;
    write_new_file(
        root,
        ".vaultc/diagnostics.json",
        &to_canonical_json_pretty(&approved.plan.diagnostics)?,
    )?;
    let mut transcript = Vec::new();
    for record in &approved.transcript {
        transcript.extend(to_canonical_json(record)?);
        transcript.push(b'\n');
    }
    write_new_file(root, ".vaultc/ai-transcript.jsonl", &transcript)?;

    let initial_files = inventory(
        root,
        &[
            ".vaultc/manifest.json",
            ".vaultc/checksums.txt",
            ".vaultc/provenance.jsonl",
        ],
    )?;
    let graph_files: Vec<_> = initial_files
        .iter()
        .map(|file| {
            let path = root.join(&file.path);
            let bytes = fs::read(&path).map_err(|error| VaultcError::io(&path, error))?;
            Ok(crate::provenance::GraphFile {
                path: file.path.clone(),
                byte_len: file.byte_len,
                content_hash: ContentHash::from_bytes(&bytes),
            })
        })
        .collect::<Result<_>>()?;
    let provenance = crate::provenance::build_stored_records(approved, &graph_files)?;
    let provenance_bytes = crate::provenance::encode_jsonl(&provenance)?;
    let provenance_graph_hash = crate::provenance::stored_graph_hash(&provenance_bytes);
    write_new_file(root, ".vaultc/provenance.jsonl", &provenance_bytes)?;

    let files_for_identity = inventory(root, &[".vaultc/manifest.json", ".vaultc/checksums.txt"])?;
    let approved_proposal_hashes: Vec<_> = approved
        .approved_proposals
        .iter()
        .map(|proposal| proposal.content_hash)
        .collect();
    let artifact_id = artifact_identity(
        &approved.plan.plan_id.to_string(),
        &files_for_identity,
        &approved_proposal_hashes,
    )?;
    let manifest = ArtifactManifest {
        schema_version: 1,
        compiler_version: env!("CARGO_PKG_VERSION").into(),
        provenance_schema_version: crate::provenance::PROVENANCE_SCHEMA_VERSION,
        provenance_graph_hash,
        artifact_id,
        plan_id: approved.plan.plan_id.to_string(),
        policy_hash: policy.semantic_hash()?,
        projection_hash: approved.plan.projection_hash,
        source_snapshot_ids: approved
            .plan
            .snapshots
            .iter()
            .map(|snapshot| snapshot.snapshot_id.to_string())
            .collect(),
        approved_proposal_hashes,
        files: files_for_identity,
    };
    write_new_file(
        root,
        ".vaultc/manifest.json",
        &to_canonical_json_pretty(&manifest)?,
    )?;
    let checksum_files = inventory(root, &[".vaultc/checksums.txt"])?;
    let mut checksum_text = String::new();
    for file in &checksum_files {
        writeln!(&mut checksum_text, "{}  {}", file.sha256, file.path)
            .map_err(|error| VaultcError::Internal(error.to_string()))?;
    }
    write_new_file(root, ".vaultc/checksums.txt", checksum_text.as_bytes())?;

    crate::verify::verify_directory(root)?;
    Ok(CompiledArtifact {
        path: root.to_path_buf(),
        artifact_id,
        plan_id: approved.plan.plan_id.to_string(),
        files: manifest.files,
    })
}

fn audit_plan_bytes(approved: &ApprovedPlan) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(approved)?;
    let snapshots = value
        .pointer_mut("/plan/snapshots")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| VaultcError::Internal("approved plan lost its snapshot array".into()))?;
    for snapshot in snapshots {
        let path = snapshot.pointer_mut("/source/path").ok_or_else(|| {
            VaultcError::Internal("approved plan source lost its locator field".into())
        })?;
        *path = serde_json::Value::String("[redacted-source-locator]".into());
    }
    crate::canonical::canonical_value_pretty(value)
}

pub(crate) fn artifact_identity(
    plan_id: &str,
    files: &[ManifestFile],
    approved_proposal_hashes: &[ContentHash],
) -> Result<ContentHash> {
    crate::canonical::canonical_hash(
        "vaultc:artifact:v1\0",
        &(plan_id, files, approved_proposal_hashes),
    )
}

fn validate_markdown_source_links(document: &crate::ir::Document, source: &[u8]) -> Result<()> {
    let source_len = u64::try_from(source.len())
        .map_err(|_| VaultcError::ResourceLimit("Markdown source length overflow".into()))?;
    if source_len != document.source_file.byte_len
        || ContentHash::from_bytes(source) != document.source_file.content_hash
    {
        return Err(VaultcError::IdentityMismatch(format!(
            "Markdown source `{}/{}` does not match its sealed document",
            document.source_file.source_id, document.source_file.logical_path
        )));
    }
    for link in &document.links {
        let start = usize::try_from(link.span.byte_start)
            .map_err(|_| VaultcError::PlanStale("Markdown link start overflow".into()))?;
        let end = usize::try_from(link.span.byte_end)
            .map_err(|_| VaultcError::PlanStale("Markdown link end overflow".into()))?;
        if source.get(start..end) != Some(link.raw_target.as_bytes()) {
            return Err(VaultcError::PlanStale(format!(
                "Markdown link {} source slice does not match its sealed raw target",
                link.link_id
            )));
        }
    }
    Ok(())
}

fn apply_replacements(
    mut bytes: Vec<u8>,
    replacements: &[crate::plan::RewriteReplacement],
) -> Result<Vec<u8>> {
    let mut previous_start = u64::MAX;
    for replacement in replacements.iter().rev() {
        let start = usize::try_from(replacement.span.byte_start)
            .map_err(|_| VaultcError::Internal("replacement start overflow".into()))?;
        let end = usize::try_from(replacement.span.byte_end)
            .map_err(|_| VaultcError::Internal("replacement end overflow".into()))?;
        if start > end || end > bytes.len() || replacement.span.byte_end > previous_start {
            return Err(VaultcError::PlanStale(
                "rewrite spans overlap or are out of bounds".into(),
            ));
        }
        bytes.splice(
            start..end,
            replacement.replacement.as_bytes().iter().copied(),
        );
        previous_start = replacement.span.byte_start;
    }
    std::str::from_utf8(&bytes).map_err(|error| {
        VaultcError::Internal(format!("rewritten Markdown is not UTF-8: {error}"))
    })?;
    Ok(bytes)
}

fn mark_incomplete(staging: &Path, error: &VaultcError) {
    let marker = staging.join(".vaultc-INCOMPLETE");
    let message = format!(
        "This directory is an incomplete Vault Compiler staging artifact.\nReason: {error}\n"
    );
    let _ = fs::write(marker, message);
}

fn write_new_file(root: &Path, logical_path: &str, bytes: &[u8]) -> Result<()> {
    let path = root.join(logical_path);
    if !path.starts_with(root) {
        return Err(VaultcError::UnsafePath {
            path: logical_path.into(),
            reason: "output escaped staging root".into(),
        });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| VaultcError::io(parent, error))?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(&path)
        .map_err(|error| VaultcError::io(&path, error))?;
    file.write_all(bytes)
        .map_err(|error| VaultcError::io(&path, error))?;
    file.sync_all()
        .map_err(|error| VaultcError::io(&path, error))?;
    Ok(())
}

pub(crate) fn inventory(root: &Path, excluded: &[&str]) -> Result<Vec<ManifestFile>> {
    let mut files = Vec::new();
    for entry in ignore::WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(false)
        .build()
    {
        let entry = entry.map_err(|error| VaultcError::VerificationFailed(error.to_string()))?;
        if entry.path() == root || !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry.path().strip_prefix(root).map_err(|_| {
            VaultcError::VerificationFailed("inventory entry escaped artifact root".into())
        })?;
        let logical = relative
            .components()
            .map(|component| {
                component.as_os_str().to_str().ok_or_else(|| {
                    VaultcError::VerificationFailed(
                        "artifact inventory contains a non-UTF-8 path".into(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?
            .join("/");
        if excluded.contains(&logical.as_str()) {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(|error| VaultcError::io(entry.path(), error))?;
        files.push(ManifestFile {
            path: logical,
            byte_len: bytes.len() as u64,
            sha256: raw_sha256_hex(&bytes),
        });
    }
    files.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(files)
}

pub(crate) fn raw_sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn sync_parent(parent: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let directory = File::open(parent)?;
        directory.sync_all()?;
    }
    Ok(())
}
