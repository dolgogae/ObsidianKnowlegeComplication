use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
#[cfg(unix)]
use std::fs::File;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use okc_protocol::ProposalKind;
use serde::{Deserialize, Serialize};

use crate::approval::ApprovedPlan;
use crate::canonical::{to_canonical_json, to_canonical_json_pretty};
use crate::config::CompilerPolicy;
use crate::error::{OkcError, Result, StagingDispositionAction};
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
    pub media_type: String,
    pub byte_len: u64,
    pub raw_sha256: String,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainManifest {
    pub rust_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributionSummary {
    pub documents_with_author_declarations: u64,
    pub documents_with_license_declarations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionMetadata {
    pub pack_profile: String,
    pub user_pack_publisher_signed: bool,
    pub release_binary_signing: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub format_family: String,
    pub schema_version: u32,
    pub compiler_version: String,
    pub toolchain: ToolchainManifest,
    pub provenance_schema_version: u32,
    pub provenance_graph_hash: ContentHash,
    pub artifact_id: ContentHash,
    pub plan_id: String,
    pub materialization_id: String,
    pub policy_hash: ContentHash,
    pub projection_hash: ContentHash,
    pub source_snapshot_ids: Vec<String>,
    pub approved_proposal_hashes: Vec<ContentHash>,
    pub approved_proposal_ids: Vec<String>,
    pub attribution_summary: AttributionSummary,
    pub creation_policy: String,
    pub distribution: DistributionMetadata,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryPublicationStep {
    StageCreated,
    MaterializedAndFilesSynced,
    TreeSynced,
    StagedVerified,
    BeforePublish,
    Published,
    ParentSynchronized,
}

trait DirectoryPublicationHook {
    fn checkpoint(&self, _step: DirectoryPublicationStep, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }

    fn publish_noreplace(&self, staging: &Path, destination: &Path) -> std::io::Result<()> {
        publish_directory_noreplace(staging, destination)
    }

    fn sync_parent(&self, parent: &Path) -> std::io::Result<()> {
        sync_directory(parent)
    }

    fn remove_staging(&self, staging: tempfile::TempDir) -> std::io::Result<()> {
        staging.close()
    }

    fn write_incomplete_marker(&self, staging: &Path, error: &OkcError) -> std::io::Result<()> {
        write_incomplete_marker(staging, error)
    }
}

struct ProductionDirectoryPublicationHook;

impl DirectoryPublicationHook for ProductionDirectoryPublicationHook {}

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
    compile_plan_with_options_and_hook(
        approved,
        destination,
        policy,
        options,
        &ProductionDirectoryPublicationHook,
    )
}

fn compile_plan_with_options_and_hook(
    approved: &ApprovedPlan,
    destination: &Path,
    policy: &CompilerPolicy,
    options: &CompileOptions,
    hook: &impl DirectoryPublicationHook,
) -> Result<CompiledArtifact> {
    validate_compile_request(approved, destination, policy)?;
    if let Some(pack) = &options.create_pack {
        validate_source_output_disjoint(approved, pack)?;
        crate::pack::preflight_pack_destination(destination, pack)?;
    }

    let artifact = compile_validated_plan(approved, destination, policy, hook)?;
    if let Some(pack) = &options.create_pack
        && let Err(source) = crate::pack::create_pack(destination, pack, policy.output.zstd_level)
    {
        return Err(OkcError::PackPublicationAfterCompile {
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
    crate::approval::validate_approved_plan(approved, policy)?;
    if !approved.conflict_decisions_complete {
        return Err(OkcError::ApprovalStale(
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
        return Err(OkcError::ApprovalStale(
            "compilation policy denies plan warnings".into(),
        ));
    }
    if approved.plan.policy.semantic_hash()? != policy.semantic_hash()? {
        return Err(OkcError::PlanStale(
            "approved plan policy does not match compiler policy".into(),
        ));
    }
    validate_source_output_disjoint(approved, destination)?;

    // This fast check produces a useful error but is not the exclusivity
    // primitive. The native no-replace publish below closes the late race.
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err(OkcError::OutputExists(destination.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(OkcError::io(destination, error)),
    }
    Ok(())
}

fn compile_validated_plan(
    approved: &ApprovedPlan,
    destination: &Path,
    policy: &CompilerPolicy,
    hook: &impl DirectoryPublicationHook,
) -> Result<CompiledArtifact> {
    let parent = destination_parent(destination);
    fs::create_dir_all(parent).map_err(|error| OkcError::io(parent, error))?;
    let staging = tempfile::Builder::new()
        .prefix(".okc-staging-")
        .tempdir_in(parent)
        .map_err(|error| OkcError::io(parent, error))?;
    let staging_path = staging.path().to_path_buf();
    if let Err(source) = restrict_staging_permissions(&staging_path) {
        let error = OkcError::io(&staging_path, source);
        return Err(dispose_failed_staging(
            staging,
            error,
            policy.output.retain_failed_staging,
            hook,
        ));
    }
    if let Err(error) = checkpoint(hook, DirectoryPublicationStep::StageCreated, &staging_path) {
        return Err(dispose_failed_staging(
            staging,
            error,
            policy.output.retain_failed_staging,
            hook,
        ));
    }

    let artifact = match compile_into(approved, &staging_path, policy) {
        Ok(artifact) => artifact,
        Err(error) => {
            return Err(dispose_failed_staging(
                staging,
                error,
                policy.output.retain_failed_staging,
                hook,
            ));
        }
    };
    for step in [
        DirectoryPublicationStep::MaterializedAndFilesSynced,
        DirectoryPublicationStep::TreeSynced,
        DirectoryPublicationStep::StagedVerified,
        DirectoryPublicationStep::BeforePublish,
    ] {
        let result = match step {
            DirectoryPublicationStep::TreeSynced => sync_directory_tree(&staging_path),
            DirectoryPublicationStep::StagedVerified => {
                crate::verify::verify_directory(&staging_path).map(|_| ())
            }
            _ => Ok(()),
        }
        .and_then(|()| checkpoint(hook, step, &staging_path));
        if let Err(error) = result {
            return Err(dispose_failed_staging(
                staging,
                error,
                policy.output.retain_failed_staging,
                hook,
            ));
        }
    }

    if let Err(source) = hook.publish_noreplace(&staging_path, destination) {
        let error = classify_publish_error(destination, source);
        return Err(dispose_failed_staging(
            staging,
            error,
            policy.output.retain_failed_staging,
            hook,
        ));
    }
    let _published_stage = staging.keep();
    if let Err(source) = hook.checkpoint(DirectoryPublicationStep::Published, destination) {
        return Err(OkcError::PublishedButDurabilityUncertain {
            path: destination.to_path_buf(),
            source,
        });
    }
    if let Err(source) = hook
        .sync_parent(parent)
        .and_then(|()| hook.checkpoint(DirectoryPublicationStep::ParentSynchronized, destination))
    {
        return Err(OkcError::PublishedButDurabilityUncertain {
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
    for operation in &approved.materialization.effective_operations {
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
            return Err(OkcError::Internal(format!(
                "duplicate output operation for `{destination}`"
            )));
        }
        let source = approved.plan.source(source_id).ok_or_else(|| {
            OkcError::PlanStale(format!("source `{source_id}` is missing from plan"))
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
                OkcError::PlanStale(format!(
                    "source file `{source_id}/{source_path}` is missing from plan"
                ))
            })?;
        let bytes = read_source_entry(source, source_file, policy)?;
        if ContentHash::from_bytes(&bytes) != *expected_hash {
            return Err(OkcError::IdentityMismatch(format!(
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
                        OkcError::PlanStale(format!(
                            "Markdown rewrite source `{source_id}/{source_path}` is absent from the sealed workspace"
                        ))
                    })?;
                validate_markdown_source_links(document, &bytes)?;
                let output = apply_replacements(bytes, replacements)?;
                if ContentHash::from_bytes(&output) != *expected_output_hash {
                    return Err(OkcError::PlanStale(format!(
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
                        OkcError::PlanStale(format!(
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
                    return Err(OkcError::PlanStale(format!(
                        "Canvas source `{source_id}/{source_path}` does not match its sealed semantic value"
                    )));
                }
                let output = crate::plan::render_rewritten_canvas(canvas, rewrites)?;
                if ContentHash::from_bytes(&output) != *expected_output_hash {
                    return Err(OkcError::PlanStale(format!(
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
                    return Err(OkcError::ApprovalStale(format!(
                        "generated proposal `{}` materialization is stale",
                        proposal.proposal.proposal_id
                    )));
                }
                validate_output_logical_path(destination, policy)?;
                if !written.insert(destination.clone()) {
                    return Err(OkcError::ProposalInvalid(format!(
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
                    return Err(OkcError::ApprovalStale(format!(
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
                return Err(OkcError::ApprovalStale(format!(
                    "proposal `{}` kind does not match its materialization",
                    proposal.proposal.proposal_id
                )));
            }
        }
    }

    let audit = root.join(".okc");
    fs::create_dir_all(&audit).map_err(|error| OkcError::io(&audit, error))?;
    write_new_file(root, ".okc/plan.json", &audit_plan_bytes(approved)?)?;
    write_new_file(
        root,
        ".okc/conflicts.json",
        &to_canonical_json_pretty(&serde_json::json!({
            "conflicts": &approved.plan.conflicts,
            "decisions": &approved.conflict_decisions,
        }))?,
    )?;
    write_new_file(
        root,
        ".okc/diagnostics.json",
        &to_canonical_json_pretty(&approved.plan.diagnostics)?,
    )?;
    let mut transcript = Vec::new();
    for record in &approved.transcript {
        transcript.extend(to_canonical_json(record)?);
        transcript.push(b'\n');
    }
    write_new_file(root, ".okc/ai-transcript.jsonl", &transcript)?;

    let initial_files = inventory(
        root,
        &[
            ".okc/manifest.json",
            ".okc/checksums.txt",
            ".okc/provenance.jsonl",
        ],
    )?;
    let graph_files: Vec<_> = initial_files
        .iter()
        .map(|file| {
            let path = root.join(&file.path);
            let bytes = fs::read(&path).map_err(|error| OkcError::io(&path, error))?;
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
    write_new_file(root, ".okc/provenance.jsonl", &provenance_bytes)?;

    let files_for_identity = inventory(root, &[".okc/manifest.json", ".okc/checksums.txt"])?;
    let approved_proposal_hashes: Vec<_> = approved
        .approved_proposals
        .iter()
        .map(|proposal| proposal.content_hash)
        .collect();
    let artifact_id = artifact_identity(
        &approved.plan.plan_id.to_string(),
        &approved.materialization.materialization_id.to_string(),
        &files_for_identity,
        &approved_proposal_hashes,
    )?;
    let approved_proposal_ids = approved
        .approved_proposals
        .iter()
        .map(|proposal| proposal.proposal.proposal_id.clone())
        .collect();
    let manifest = ArtifactManifest {
        format_family: "okc".into(),
        schema_version: 2,
        compiler_version: env!("CARGO_PKG_VERSION").into(),
        toolchain: ToolchainManifest {
            rust_version: env!("CARGO_PKG_RUST_VERSION").into(),
        },
        provenance_schema_version: crate::provenance::PROVENANCE_SCHEMA_VERSION,
        provenance_graph_hash,
        artifact_id,
        plan_id: approved.plan.plan_id.to_string(),
        materialization_id: approved.materialization.materialization_id.to_string(),
        policy_hash: policy.semantic_hash()?,
        projection_hash: approved.plan.projection_hash,
        source_snapshot_ids: approved
            .plan
            .snapshots
            .iter()
            .map(|snapshot| snapshot.snapshot_id.to_string())
            .collect(),
        approved_proposal_hashes,
        approved_proposal_ids,
        attribution_summary: attribution_summary(approved),
        creation_policy: "deterministic-approved-materialization-v2".into(),
        distribution: DistributionMetadata {
            pack_profile: crate::pack::PACK_PROFILE.into(),
            user_pack_publisher_signed: false,
            release_binary_signing: "native-release-artifacts-only".into(),
        },
        files: files_for_identity,
    };
    write_new_file(
        root,
        ".okc/manifest.json",
        &to_canonical_json_pretty(&manifest)?,
    )?;
    let checksum_files = inventory(root, &[".okc/checksums.txt"])?;
    let mut checksum_text = String::new();
    for file in &checksum_files {
        writeln!(&mut checksum_text, "{}  {}", file.raw_sha256, file.path)
            .map_err(|error| OkcError::Internal(error.to_string()))?;
    }
    write_new_file(root, ".okc/checksums.txt", checksum_text.as_bytes())?;

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
        .ok_or_else(|| OkcError::Internal("approved plan lost its snapshot array".into()))?;
    for snapshot in snapshots {
        let path = snapshot.pointer_mut("/source/path").ok_or_else(|| {
            OkcError::Internal("approved plan source lost its locator field".into())
        })?;
        *path = serde_json::Value::String("[redacted-source-locator]".into());
    }
    crate::canonical::canonical_value_pretty(value)
}

pub(crate) fn artifact_identity(
    plan_id: &str,
    materialization_id: &str,
    files: &[ManifestFile],
    approved_proposal_hashes: &[ContentHash],
) -> Result<ContentHash> {
    crate::canonical::canonical_hash(
        "okc:artifact:v2\0",
        &(plan_id, materialization_id, files, approved_proposal_hashes),
    )
}

fn attribution_summary(approved: &ApprovedPlan) -> AttributionSummary {
    let mut author_count = 0_u64;
    let mut license_count = 0_u64;
    for document in approved.plan.workspace.documents.values() {
        let Some(serde_json::Value::Object(frontmatter)) = &document.frontmatter else {
            continue;
        };
        if frontmatter
            .keys()
            .any(|key| matches!(key.to_ascii_lowercase().as_str(), "author" | "authors"))
        {
            author_count += 1;
        }
        if frontmatter
            .keys()
            .any(|key| matches!(key.to_ascii_lowercase().as_str(), "license" | "licenses"))
        {
            license_count += 1;
        }
    }
    AttributionSummary {
        documents_with_author_declarations: author_count,
        documents_with_license_declarations: license_count,
    }
}

fn validate_markdown_source_links(document: &crate::ir::Document, source: &[u8]) -> Result<()> {
    let source_len = u64::try_from(source.len())
        .map_err(|_| OkcError::ResourceLimit("Markdown source length overflow".into()))?;
    if source_len != document.source_file.byte_len
        || ContentHash::from_bytes(source) != document.source_file.content_hash
    {
        return Err(OkcError::IdentityMismatch(format!(
            "Markdown source `{}/{}` does not match its sealed document",
            document.source_file.source_id, document.source_file.logical_path
        )));
    }
    for link in &document.links {
        let start = usize::try_from(link.span.byte_start)
            .map_err(|_| OkcError::PlanStale("Markdown link start overflow".into()))?;
        let end = usize::try_from(link.span.byte_end)
            .map_err(|_| OkcError::PlanStale("Markdown link end overflow".into()))?;
        if source.get(start..end) != Some(link.raw_target.as_bytes()) {
            return Err(OkcError::PlanStale(format!(
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
            .map_err(|_| OkcError::Internal("replacement start overflow".into()))?;
        let end = usize::try_from(replacement.span.byte_end)
            .map_err(|_| OkcError::Internal("replacement end overflow".into()))?;
        if start > end || end > bytes.len() || replacement.span.byte_end > previous_start {
            return Err(OkcError::PlanStale(
                "rewrite spans overlap or are out of bounds".into(),
            ));
        }
        bytes.splice(
            start..end,
            replacement.replacement.as_bytes().iter().copied(),
        );
        previous_start = replacement.span.byte_start;
    }
    std::str::from_utf8(&bytes)
        .map_err(|error| OkcError::Internal(format!("rewritten Markdown is not UTF-8: {error}")))?;
    Ok(bytes)
}

fn checkpoint(
    hook: &impl DirectoryPublicationHook,
    step: DirectoryPublicationStep,
    path: &Path,
) -> Result<()> {
    hook.checkpoint(step, path)
        .map_err(|error| OkcError::io(path, error))
}

fn dispose_failed_staging(
    staging: tempfile::TempDir,
    original: OkcError,
    retain: bool,
    hook: &impl DirectoryPublicationHook,
) -> OkcError {
    let staging_path = staging.path().to_path_buf();
    if !retain {
        return match hook.remove_staging(staging) {
            Ok(()) => original,
            Err(disposition_error) => OkcError::StagingDispositionFailed {
                staging: staging_path,
                action: StagingDispositionAction::Remove,
                original: Box::new(original),
                disposition_error,
            },
        };
    }

    match hook.write_incomplete_marker(&staging_path, &original) {
        Ok(()) => {
            let _retained = staging.keep();
            original
        }
        Err(marker_error) => match hook.remove_staging(staging) {
            Ok(()) => OkcError::StagingDispositionFailed {
                staging: staging_path,
                action: StagingDispositionAction::MarkIncompleteAndRetain,
                original: Box::new(original),
                disposition_error: marker_error,
            },
            Err(cleanup_error) => OkcError::StagingDispositionFailed {
                staging: staging_path,
                action: StagingDispositionAction::MarkIncompleteAndRetain,
                original: Box::new(original),
                disposition_error: std::io::Error::new(
                    cleanup_error.kind(),
                    format!(
                        "incomplete marker failed: {marker_error}; fallback cleanup failed: {cleanup_error}"
                    ),
                ),
            },
        },
    }
}

fn write_incomplete_marker(staging: &Path, error: &OkcError) -> std::io::Result<()> {
    let marker = staging.join(".okc-INCOMPLETE");
    let message = format!(
        "This directory is an incomplete Vault Compiler staging artifact.\nReason: {error}\n"
    );
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(marker)?;
    file.write_all(message.as_bytes())?;
    file.sync_all()?;
    sync_directory(staging)
}

#[cfg(unix)]
fn restrict_staging_permissions(staging: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(staging, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_staging_permissions(_staging: &Path) -> std::io::Result<()> {
    Ok(())
}

fn sync_directory_tree(root: &Path) -> Result<()> {
    let mut directories = Vec::new();
    for entry in ignore::WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(false)
        .build()
    {
        let entry = entry.map_err(|error| {
            OkcError::VerificationFailed(format!(
                "unable to traverse staged directory before publication: {error}"
            ))
        })?;
        let kind = entry.file_type().ok_or_else(|| {
            OkcError::VerificationFailed(format!(
                "staged entry `{}` has no file type",
                entry.path().display()
            ))
        })?;
        if kind.is_dir() {
            directories.push(entry.path().to_path_buf());
        } else if !kind.is_file() {
            return Err(OkcError::VerificationFailed(format!(
                "staged entry `{}` is neither a regular file nor directory",
                entry.path().display()
            )));
        }
    }
    directories.sort_by(|left, right| {
        right
            .components()
            .count()
            .cmp(&left.components().count())
            .then_with(|| left.as_os_str().cmp(right.as_os_str()))
    });
    for directory in directories {
        sync_directory(&directory).map_err(|error| OkcError::io(&directory, error))?;
    }
    Ok(())
}

fn validate_source_output_disjoint(approved: &ApprovedPlan, destination: &Path) -> Result<()> {
    let output = resolve_existing_ancestor(destination)?;
    let output_key = portable_host_path_key(&output)?;
    for snapshot in &approved.plan.snapshots {
        let source = resolve_existing_ancestor(snapshot.source.path())?;
        let source_key = portable_host_path_key(&source)?;
        if source_key == output_key
            || source_key.starts_with(&output_key)
            || output_key.starts_with(&source_key)
        {
            return Err(OkcError::UnsafePath {
                path: destination.display().to_string(),
                reason: format!(
                    "publication destination and source `{}` must be disjoint in both containment directions",
                    snapshot.source.path().display()
                ),
            });
        }
    }
    Ok(())
}

fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| OkcError::io(path, error))?
            .join(path)
    };
    for ancestor in absolute.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => match fs::canonicalize(ancestor) {
                Ok(resolved) => {
                    let suffix = absolute.strip_prefix(ancestor).map_err(|_| {
                        OkcError::Internal(
                            "existing publication ancestor was not a lexical prefix".into(),
                        )
                    })?;
                    return lexical_normalize(&resolved.join(suffix));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(OkcError::io(ancestor, error)),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(OkcError::io(ancestor, error)),
        }
    }
    Err(OkcError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "publication path has no resolvable existing ancestor",
        ),
    })
}

fn lexical_normalize(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(OkcError::UnsafePath {
                        path: path.display().to_string(),
                        reason: "host publication path traverses above its root".into(),
                    });
                }
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    Ok(normalized)
}

fn portable_host_path_key(path: &Path) -> Result<Vec<String>> {
    path.components()
        .map(|component| match component {
            Component::RootDir => Ok("root:".to_owned()),
            Component::Prefix(prefix) => portable_component_key(prefix.as_os_str(), path),
            Component::Normal(segment) => portable_component_key(segment, path),
            Component::CurDir | Component::ParentDir => Err(OkcError::Internal(
                "portable host path key received a non-normalized path".into(),
            )),
        })
        .collect()
}

fn portable_component_key(component: &std::ffi::OsStr, path: &Path) -> Result<String> {
    let component = component.to_str().ok_or_else(|| OkcError::UnsafePath {
        path: path.display().to_string(),
        reason: "publication paths must be valid UTF-8 for portable comparison".into(),
    })?;
    Ok(crate::parse::full_casefold_nfc(component))
}

fn destination_parent(destination: &Path) -> &Path {
    destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn classify_publish_error(destination: &Path, source: std::io::Error) -> OkcError {
    if source.kind() == std::io::ErrorKind::AlreadyExists
        || fs::symlink_metadata(destination).is_ok()
    {
        OkcError::OutputExists(destination.to_path_buf())
    } else {
        OkcError::io(destination, source)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn publish_directory_noreplace(staging: &Path, destination: &Path) -> std::io::Result<()> {
    let staging_parent = destination_parent(staging);
    let destination_parent = destination_parent(destination);
    if staging_parent != destination_parent {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "staging and destination are not siblings",
        ));
    }
    let staging_name = staging.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "staging has no leaf name")
    })?;
    let destination_name = destination.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination has no leaf name",
        )
    })?;
    let parent = File::open(destination_parent)?;
    rustix::fs::renameat_with(
        &parent,
        staging_name,
        &parent,
        destination_name,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)
}

#[cfg(windows)]
fn publish_directory_noreplace(staging: &Path, destination: &Path) -> std::io::Result<()> {
    atomicwrites::move_atomic(staging, destination)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn publish_directory_noreplace(_staging: &Path, _destination: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace directory publication is unsupported on this platform",
    ))
}

fn write_new_file(root: &Path, logical_path: &str, bytes: &[u8]) -> Result<()> {
    let path = root.join(logical_path);
    if !path.starts_with(root) {
        return Err(OkcError::UnsafePath {
            path: logical_path.into(),
            reason: "output escaped staging root".into(),
        });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| OkcError::io(parent, error))?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(&path)
        .map_err(|error| OkcError::io(&path, error))?;
    file.write_all(bytes)
        .map_err(|error| OkcError::io(&path, error))?;
    file.sync_all()
        .map_err(|error| OkcError::io(&path, error))?;
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
        let entry = entry.map_err(|error| OkcError::VerificationFailed(error.to_string()))?;
        if entry.path() == root || !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry.path().strip_prefix(root).map_err(|_| {
            OkcError::VerificationFailed("inventory entry escaped artifact root".into())
        })?;
        let logical = relative
            .components()
            .map(|component| {
                component.as_os_str().to_str().ok_or_else(|| {
                    OkcError::VerificationFailed(
                        "artifact inventory contains a non-UTF-8 path".into(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?
            .join("/");
        if excluded.contains(&logical.as_str()) {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(|error| OkcError::io(entry.path(), error))?;
        files.push(ManifestFile {
            path: logical,
            media_type: media_type_for_path(relative),
            byte_len: bytes.len() as u64,
            raw_sha256: raw_sha256_hex(&bytes),
            content_hash: ContentHash::from_bytes(&bytes),
        });
    }
    files.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(files)
}

fn media_type_for_path(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("md") {
        "text/markdown; charset=utf-8"
    } else if extension.eq_ignore_ascii_case("canvas") {
        "application/vnd.obsidian.canvas+json"
    } else if extension.eq_ignore_ascii_case("base") {
        "application/vnd.obsidian.base"
    } else if extension.eq_ignore_ascii_case("json") {
        "application/json"
    } else if extension.eq_ignore_ascii_case("jsonl") {
        "application/x-ndjson"
    } else if extension.eq_ignore_ascii_case("txt") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
    .into()
}

pub(crate) fn raw_sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    hex::encode(Sha256::digest(bytes))
}

#[cfg(unix)]
fn sync_directory(parent: &Path) -> std::io::Result<()> {
    let directory = File::open(parent)?;
    directory.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_parent: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        CompileOptions, DirectoryPublicationHook, DirectoryPublicationStep,
        compile_plan_with_options_and_hook, publish_directory_noreplace, sync_directory,
        write_incomplete_marker,
    };
    use crate::config::CompilerPolicy;
    use crate::error::{OkcError, StagingDispositionAction};
    use crate::{ApprovedPlan, OkcCompiler, SourceSpec};

    #[derive(Debug, Clone, Copy)]
    enum LateWinner {
        File,
        Directory,
        #[cfg(unix)]
        Symlink,
        #[cfg(unix)]
        DanglingSymlink,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum HookFault {
        UnsupportedPublish,
        ParentSync,
        Marker,
        Remove,
    }

    #[derive(Default)]
    struct RecordingHook {
        steps: RefCell<Vec<DirectoryPublicationStep>>,
        fail_at: Option<DirectoryPublicationStep>,
        tamper_at: Option<DirectoryPublicationStep>,
        winner: Option<(PathBuf, LateWinner)>,
        faults: Vec<HookFault>,
    }

    impl RecordingHook {
        fn observed(&self) -> Vec<DirectoryPublicationStep> {
            self.steps.borrow().clone()
        }
    }

    impl DirectoryPublicationHook for RecordingHook {
        fn checkpoint(&self, step: DirectoryPublicationStep, path: &Path) -> std::io::Result<()> {
            self.steps.borrow_mut().push(step);
            if step == DirectoryPublicationStep::BeforePublish
                && let Some((destination, kind)) = &self.winner
            {
                match kind {
                    LateWinner::File => fs::write(destination, b"external winner\n")?,
                    LateWinner::Directory => fs::create_dir(destination)?,
                    #[cfg(unix)]
                    LateWinner::Symlink => {
                        use std::os::unix::fs::symlink;

                        let referent = destination.with_extension("winner-referent");
                        fs::write(&referent, b"external symlink referent\n")?;
                        symlink(referent, destination)?;
                    }
                    #[cfg(unix)]
                    LateWinner::DanglingSymlink => {
                        use std::os::unix::fs::symlink;

                        symlink(destination.with_extension("missing-referent"), destination)?;
                    }
                }
            }
            if self.tamper_at == Some(step) {
                fs::write(
                    path.join("knowledge/Index.md"),
                    b"tampered after tree synchronization\n",
                )?;
            }
            if self.fail_at == Some(step) {
                return Err(std::io::Error::other(format!(
                    "injected directory publication failure at {step:?}"
                )));
            }
            Ok(())
        }

        fn publish_noreplace(&self, staging: &Path, destination: &Path) -> std::io::Result<()> {
            if self.faults.contains(&HookFault::UnsupportedPublish) {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "injected unsupported no-replace primitive",
                ))
            } else {
                publish_directory_noreplace(staging, destination)
            }
        }

        fn sync_parent(&self, parent: &Path) -> std::io::Result<()> {
            if self.faults.contains(&HookFault::ParentSync) {
                Err(std::io::Error::other(
                    "injected output-parent synchronization failure",
                ))
            } else {
                sync_directory(parent)
            }
        }

        fn remove_staging(&self, staging: tempfile::TempDir) -> std::io::Result<()> {
            if self.faults.contains(&HookFault::Remove) {
                let _retained = staging.keep();
                Err(std::io::Error::other("injected staging cleanup failure"))
            } else {
                staging.close()
            }
        }

        fn write_incomplete_marker(&self, staging: &Path, error: &OkcError) -> std::io::Result<()> {
            if self.faults.contains(&HookFault::Marker) {
                Err(std::io::Error::other("injected incomplete-marker failure"))
            } else {
                write_incomplete_marker(staging, error)
            }
        }
    }

    fn approved_fixture(retain_failed_staging: bool) -> (OkcCompiler, ApprovedPlan) {
        let mut policy = CompilerPolicy::default();
        policy.output.retain_failed_staging = retain_failed_staging;
        let compiler = OkcCompiler::builder()
            .policy(policy)
            .build()
            .expect("build directory publication test compiler");
        let source =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/basic_vault");
        let inspection = compiler
            .inspect([SourceSpec::directory("directory-publication-unit", source)
                .expect("directory publication unit source")])
            .expect("inspect directory publication unit source");
        let plan = compiler
            .plan(&inspection)
            .expect("plan directory publication unit source");
        let approved = compiler
            .approve_without_augmentation(plan)
            .expect("approve directory publication unit source");
        (compiler, approved)
    }

    fn staging_paths(parent: &Path) -> Vec<PathBuf> {
        let mut paths = fs::read_dir(parent)
            .expect("read directory publication parent")
            .filter_map(|entry| {
                let path = entry.expect("directory publication entry").path();
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".okc-staging-"))
                    .then_some(path)
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    #[test]
    fn directory_publication_checkpoints_follow_the_normative_order() {
        let temporary = tempfile::tempdir().expect("checkpoint temporary parent");
        let (compiler, approved) = approved_fixture(false);
        let destination = temporary.path().join("compiled");
        let hook = RecordingHook::default();

        compile_plan_with_options_and_hook(
            &approved,
            &destination,
            compiler.policy(),
            &CompileOptions::default(),
            &hook,
        )
        .expect("publish through every directory checkpoint");

        assert_eq!(
            hook.observed(),
            vec![
                DirectoryPublicationStep::StageCreated,
                DirectoryPublicationStep::MaterializedAndFilesSynced,
                DirectoryPublicationStep::TreeSynced,
                DirectoryPublicationStep::StagedVerified,
                DirectoryPublicationStep::BeforePublish,
                DirectoryPublicationStep::Published,
                DirectoryPublicationStep::ParentSynchronized,
            ]
        );
        assert!(
            compiler
                .verify(&destination)
                .expect("verify checkpoint artifact")
                .valid
        );
    }

    #[test]
    fn late_race_winners_are_never_replaced_and_staging_is_cleaned() {
        let (compiler, approved) = approved_fixture(false);
        let winner_kinds = [LateWinner::File, LateWinner::Directory];
        for (index, winner) in winner_kinds.into_iter().enumerate() {
            let temporary = tempfile::tempdir().expect("late-race temporary parent");
            let destination = temporary.path().join(format!("winner-{index}"));
            let hook = RecordingHook {
                winner: Some((destination.clone(), winner)),
                ..RecordingHook::default()
            };
            let error = compile_plan_with_options_and_hook(
                &approved,
                &destination,
                compiler.policy(),
                &CompileOptions::default(),
                &hook,
            )
            .expect_err("late race winner must reject publication");

            assert!(matches!(error, OkcError::OutputExists(ref path) if path == &destination));
            match winner {
                LateWinner::File => assert_eq!(
                    fs::read(&destination).expect("read file race winner"),
                    b"external winner\n"
                ),
                LateWinner::Directory => assert!(
                    fs::read_dir(&destination)
                        .expect("read directory race winner")
                        .next()
                        .is_none()
                ),
                #[cfg(unix)]
                LateWinner::Symlink | LateWinner::DanglingSymlink => unreachable!(),
            }
            assert!(staging_paths(temporary.path()).is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn late_live_and_dangling_symlink_race_winners_are_never_replaced() {
        let (compiler, approved) = approved_fixture(false);
        for (index, winner) in [LateWinner::Symlink, LateWinner::DanglingSymlink]
            .into_iter()
            .enumerate()
        {
            let temporary = tempfile::tempdir().expect("symlink-race temporary parent");
            let destination = temporary.path().join(format!("symlink-winner-{index}"));
            let hook = RecordingHook {
                winner: Some((destination.clone(), winner)),
                ..RecordingHook::default()
            };

            let error = compile_plan_with_options_and_hook(
                &approved,
                &destination,
                compiler.policy(),
                &CompileOptions::default(),
                &hook,
            )
            .expect_err("late symlink winner must reject publication");

            assert!(matches!(error, OkcError::OutputExists(ref path) if path == &destination));
            assert!(
                fs::symlink_metadata(&destination)
                    .expect("stat symlink winner")
                    .file_type()
                    .is_symlink()
            );
            if matches!(winner, LateWinner::Symlink) {
                assert_eq!(
                    fs::read(destination.with_extension("winner-referent"))
                        .expect("read live symlink winner referent"),
                    b"external symlink referent\n"
                );
            } else {
                assert!(!destination.exists(), "dangling winner must stay dangling");
            }
            assert!(staging_paths(temporary.path()).is_empty());
        }
    }

    #[test]
    fn every_precommit_checkpoint_failure_cleans_stage_by_default() {
        let (compiler, approved) = approved_fixture(false);
        let order = [
            DirectoryPublicationStep::StageCreated,
            DirectoryPublicationStep::MaterializedAndFilesSynced,
            DirectoryPublicationStep::TreeSynced,
            DirectoryPublicationStep::StagedVerified,
            DirectoryPublicationStep::BeforePublish,
        ];
        for (index, fail_at) in order.into_iter().enumerate() {
            let temporary = tempfile::tempdir().expect("staging disposition temporary parent");
            let destination = temporary.path().join(format!("precommit-{index}"));
            let hook = RecordingHook {
                fail_at: Some(fail_at),
                ..RecordingHook::default()
            };

            let error = compile_plan_with_options_and_hook(
                &approved,
                &destination,
                compiler.policy(),
                &CompileOptions::default(),
                &hook,
            )
            .expect_err("injected precommit failure must fail");

            assert!(matches!(error, OkcError::Io { .. }));
            assert!(!destination.exists());
            assert!(staging_paths(temporary.path()).is_empty());
            assert_eq!(hook.observed(), order[..=index]);
        }
    }

    #[test]
    fn independent_staged_verification_rejects_post_sync_tamper_before_publish() {
        let temporary = tempfile::tempdir().expect("staged-tamper temporary parent");
        let (compiler, approved) = approved_fixture(false);
        let destination = temporary.path().join("tampered");
        let hook = RecordingHook {
            tamper_at: Some(DirectoryPublicationStep::TreeSynced),
            ..RecordingHook::default()
        };

        let error = compile_plan_with_options_and_hook(
            &approved,
            &destination,
            compiler.policy(),
            &CompileOptions::default(),
            &hook,
        )
        .expect_err("post-sync staged tamper must fail independent verification");

        assert!(matches!(error, OkcError::VerificationFailed(_)));
        assert_eq!(
            hook.observed(),
            vec![
                DirectoryPublicationStep::StageCreated,
                DirectoryPublicationStep::MaterializedAndFilesSynced,
                DirectoryPublicationStep::TreeSynced,
            ]
        );
        assert!(!destination.exists());
        assert!(staging_paths(temporary.path()).is_empty());
    }

    #[test]
    fn retain_policy_marks_and_keeps_precommit_failure() {
        let temporary = tempfile::tempdir().expect("retained staging temporary parent");
        let (compiler, approved) = approved_fixture(true);
        let destination = temporary.path().join("compiled");
        let hook = RecordingHook {
            fail_at: Some(DirectoryPublicationStep::BeforePublish),
            ..RecordingHook::default()
        };

        let error = compile_plan_with_options_and_hook(
            &approved,
            &destination,
            compiler.policy(),
            &CompileOptions::default(),
            &hook,
        )
        .expect_err("retained precommit failure must fail");

        assert!(matches!(error, OkcError::Io { .. }));
        assert!(!destination.exists());
        let stages = staging_paths(temporary.path());
        assert_eq!(stages.len(), 1);
        let marker = fs::read_to_string(stages[0].join(".okc-INCOMPLETE"))
            .expect("read synchronized incomplete marker");
        assert!(marker.contains("injected directory publication failure"));
    }

    #[test]
    fn disposition_failures_preserve_original_and_every_disposition_error() {
        let temporary = tempfile::tempdir().expect("disposition failure temporary parent");
        let (compiler, approved) = approved_fixture(false);
        let destination = temporary.path().join("remove-failure");
        let hook = RecordingHook {
            fail_at: Some(DirectoryPublicationStep::BeforePublish),
            faults: vec![HookFault::Remove],
            ..RecordingHook::default()
        };
        let error = compile_plan_with_options_and_hook(
            &approved,
            &destination,
            compiler.policy(),
            &CompileOptions::default(),
            &hook,
        )
        .expect_err("cleanup failure must be explicit");
        let retained_remove_stage = match &error {
            OkcError::StagingDispositionFailed { staging, .. } => staging.clone(),
            other => panic!("unexpected cleanup error: {other:?}"),
        };
        assert!(matches!(
            error,
            OkcError::StagingDispositionFailed {
                action: StagingDispositionAction::Remove,
                ref original,
                ref disposition_error,
                ..
            } if matches!(original.as_ref(), OkcError::Io { .. })
                && disposition_error.to_string().contains("cleanup failure")
        ));
        assert_eq!(
            staging_paths(temporary.path()),
            vec![retained_remove_stage.clone()]
        );
        assert!(retained_remove_stage.is_dir());

        let temporary = tempfile::tempdir().expect("combined disposition temporary parent");
        let (compiler, approved) = approved_fixture(true);
        let destination = temporary.path().join("combined-failure");
        let hook = RecordingHook {
            fail_at: Some(DirectoryPublicationStep::BeforePublish),
            faults: vec![HookFault::Marker, HookFault::Remove],
            ..RecordingHook::default()
        };
        let error = compile_plan_with_options_and_hook(
            &approved,
            &destination,
            compiler.policy(),
            &CompileOptions::default(),
            &hook,
        )
        .expect_err("marker and fallback cleanup failures must be explicit");
        let retained_combined_stage = match &error {
            OkcError::StagingDispositionFailed { staging, .. } => staging.clone(),
            other => panic!("unexpected combined disposition error: {other:?}"),
        };
        assert!(matches!(
            error,
            OkcError::StagingDispositionFailed {
                action: StagingDispositionAction::MarkIncompleteAndRetain,
                ref original,
                ref disposition_error,
                ..
            } if matches!(original.as_ref(), OkcError::Io { .. })
                && disposition_error.to_string().contains("incomplete-marker failure")
                && disposition_error.to_string().contains("cleanup failure")
        ));
        assert_eq!(
            staging_paths(temporary.path()),
            vec![retained_combined_stage.clone()]
        );
        assert!(retained_combined_stage.is_dir());
    }

    #[test]
    fn unsupported_publish_never_falls_back() {
        let temporary = tempfile::tempdir().expect("unsupported primitive temporary parent");
        let (compiler, approved) = approved_fixture(false);
        let unsupported_destination = temporary.path().join("unsupported");
        let unsupported_hook = RecordingHook {
            faults: vec![HookFault::UnsupportedPublish],
            ..RecordingHook::default()
        };
        let error = compile_plan_with_options_and_hook(
            &approved,
            &unsupported_destination,
            compiler.policy(),
            &CompileOptions::default(),
            &unsupported_hook,
        )
        .expect_err("unsupported primitive must not fall back");
        assert!(matches!(
            error,
            OkcError::Io { ref source, .. }
                if source.kind() == std::io::ErrorKind::Unsupported
        ));
        assert!(!unsupported_destination.exists());
        assert!(staging_paths(temporary.path()).is_empty());
    }

    #[test]
    fn postcommit_faults_retain_verified_output_and_never_start_optional_pack() {
        let (compiler, approved) = approved_fixture(false);
        for (index, fail_at_parent_sync) in [false, true].into_iter().enumerate() {
            let temporary = tempfile::tempdir().expect("postcommit fault temporary parent");
            let destination = temporary.path().join(format!("uncertain-{index}"));
            let pack = temporary
                .path()
                .join(format!("must-not-start-{index}.okcpack"));
            let hook = if fail_at_parent_sync {
                RecordingHook {
                    faults: vec![HookFault::ParentSync],
                    ..RecordingHook::default()
                }
            } else {
                RecordingHook {
                    fail_at: Some(DirectoryPublicationStep::Published),
                    ..RecordingHook::default()
                }
            };

            let error = compile_plan_with_options_and_hook(
                &approved,
                &destination,
                compiler.policy(),
                &CompileOptions {
                    create_pack: Some(pack.clone()),
                },
                &hook,
            )
            .expect_err("postcommit failure must report uncertain durability");
            assert!(matches!(
                error,
                OkcError::PublishedButDurabilityUncertain { ref path, .. }
                    if path == &destination
            ));
            assert!(
                compiler
                    .verify(&destination)
                    .expect("verify retained uncertain artifact")
                    .valid
            );
            assert!(
                !pack.exists(),
                "optional pack must not start after uncertainty"
            );
            assert!(staging_paths(temporary.path()).is_empty());
        }
    }
}
