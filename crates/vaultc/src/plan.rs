use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::canonical::canonical_hash;
use crate::config::CompilerPolicy;
use crate::dedup::{DedupReport, ExactDuplicateGroup, NearDuplicateCandidate};
use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity, SourceSpan};
use crate::error::{Result, VaultcError};
use crate::identity::{AssetId, ContentHash, DocumentId, OperationId, PlanId, SnapshotId};
use crate::ir::{CanonicalWorkspace, Document, FileKind, LinkResolution};
use crate::snapshot::{VaultSnapshot, validate_output_logical_path};
use crate::source::{SourceId, SourceSpec};

pub const INSPECTION_SCHEMA_VERSION: u32 = 1;
pub const PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inspection {
    pub schema_version: u32,
    pub compiler_version: String,
    pub policy_hash: ContentHash,
    pub inspection_hash: ContentHash,
    pub snapshots: Vec<VaultSnapshot>,
    pub workspace: CanonicalWorkspace,
    pub diagnostics: Vec<Diagnostic>,
}

impl Inspection {
    pub(crate) fn new(
        policy_hash: ContentHash,
        snapshots: Vec<VaultSnapshot>,
        workspace: CanonicalWorkspace,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Self> {
        let compiler_version = env!("CARGO_PKG_VERSION").to_owned();
        let inspection_hash = calculate_inspection_hash(
            INSPECTION_SCHEMA_VERSION,
            &compiler_version,
            policy_hash,
            &snapshots,
        )?;
        Ok(Self {
            schema_version: INSPECTION_SCHEMA_VERSION,
            compiler_version,
            policy_hash,
            inspection_hash,
            snapshots,
            workspace,
            diagnostics,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftPlan {
    pub schema_version: u32,
    pub compiler_version: String,
    pub plan_id: PlanId,
    pub projection_hash: ContentHash,
    pub policy: CompilerPolicy,
    pub inspection_hash: ContentHash,
    pub snapshots: Vec<VaultSnapshot>,
    pub workspace: CanonicalWorkspace,
    pub output_paths: BTreeMap<DocumentId, String>,
    pub asset_output_paths: BTreeMap<AssetId, String>,
    pub operations: Vec<OutputOperation>,
    pub exact_groups: Vec<ExactDuplicateGroup>,
    pub near_candidates: Vec<NearDuplicateCandidate>,
    pub conflicts: Vec<Conflict>,
    pub diagnostics: Vec<Diagnostic>,
}

impl DraftPlan {
    pub fn source(&self, source_id: &SourceId) -> Option<&SourceSpec> {
        self.snapshots
            .iter()
            .find(|snapshot| &snapshot.source_id == source_id)
            .map(|snapshot| &snapshot.source)
    }

    pub fn unresolved_required_conflicts(&self) -> impl Iterator<Item = &Conflict> {
        self.conflicts.iter().filter(|conflict| {
            conflict.required && conflict.resolution == ConflictResolution::Unresolved
        })
    }

    /// Validate a serialized plan before it is approved, compiled, or used as
    /// provider input. This checks the sealed identities and the safety-critical
    /// structural invariants that serde alone cannot express.
    pub fn validate_integrity(&self) -> Result<()> {
        if self.schema_version != PLAN_SCHEMA_VERSION {
            return Err(VaultcError::PlanStale(format!(
                "unsupported plan schema version {}",
                self.schema_version
            )));
        }
        if self.compiler_version != env!("CARGO_PKG_VERSION") {
            return Err(VaultcError::PlanStale(format!(
                "plan compiler version `{}` does not match `{}`",
                self.compiler_version,
                env!("CARGO_PKG_VERSION")
            )));
        }
        self.policy.validate()?;
        validate_snapshot_set(&self.snapshots, &self.policy)?;

        let policy_hash = self.policy.semantic_hash()?;
        let expected_inspection_hash = calculate_inspection_hash(
            INSPECTION_SCHEMA_VERSION,
            &self.compiler_version,
            policy_hash,
            &self.snapshots,
        )?;
        if self.inspection_hash != expected_inspection_hash {
            return Err(VaultcError::PlanStale(
                "inspection hash does not match the sealed snapshot set".into(),
            ));
        }

        let expected_projection_hash = projection_hash(&self.workspace)?;
        if self.projection_hash != expected_projection_hash {
            return Err(VaultcError::PlanStale(
                "projection hash does not match the canonical workspace".into(),
            ));
        }
        validate_workspace_sources(&self.workspace, &self.snapshots)?;
        validate_plan_paths_and_operations(self)?;

        let mut previous_conflict_id: Option<&str> = None;
        for conflict in &self.conflicts {
            conflict.validate_integrity()?;
            if previous_conflict_id
                .is_some_and(|previous| previous >= conflict.conflict_id.as_str())
            {
                return Err(VaultcError::PlanStale(
                    "conflicts must be strictly ordered by conflict_id".into(),
                ));
            }
            previous_conflict_id = Some(&conflict.conflict_id);
        }

        let expected_plan_id = calculate_plan_id(
            self.schema_version,
            &self.compiler_version,
            policy_hash,
            self.inspection_hash,
            self.projection_hash,
            &self.output_paths,
            &self.asset_output_paths,
            &self.operations,
            &self.exact_groups,
            &self.near_candidates,
            &self.conflicts,
            &self.diagnostics,
        )?;
        if self.plan_id != expected_plan_id {
            return Err(VaultcError::PlanStale(
                "plan ID does not match the canonical plan payload".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteReplacement {
    pub span: SourceSpan,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputOperation {
    Copy {
        operation_id: OperationId,
        source_id: SourceId,
        snapshot_id: SnapshotId,
        source_path: String,
        destination: String,
        expected_hash: ContentHash,
        kind: FileKind,
    },
    RewriteMarkdown {
        operation_id: OperationId,
        source_id: SourceId,
        snapshot_id: SnapshotId,
        source_path: String,
        destination: String,
        expected_hash: ContentHash,
        replacements: Vec<RewriteReplacement>,
    },
}

impl OutputOperation {
    pub fn operation_id(&self) -> OperationId {
        match self {
            Self::Copy { operation_id, .. } | Self::RewriteMarkdown { operation_id, .. } => {
                *operation_id
            }
        }
    }

    pub fn destination(&self) -> &str {
        match self {
            Self::Copy { destination, .. } | Self::RewriteMarkdown { destination, .. } => {
                destination
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictKind {
    PathExact,
    PathCasefold,
    UnicodeNormalization,
    TitleAmbiguity,
    AliasAmbiguity,
    FrontmatterValue,
    LinkAmbiguity,
    ContentNearDuplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    Unresolved,
    AutoResolvedByNormativeRule,
    UserResolved,
    ProviderSuggested,
    WaivedByPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub conflict_id: String,
    /// Stable hash of the conflict facts. Resolution is intentionally excluded
    /// so an immutable plan can be paired with a separately approved decision.
    pub content_hash: ContentHash,
    pub kind: ConflictKind,
    pub required: bool,
    pub resolution: ConflictResolution,
    pub documents: Vec<DocumentId>,
    pub message: String,
    pub score: Option<f64>,
}

impl Conflict {
    pub fn validate_integrity(&self) -> Result<()> {
        if self.documents.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(VaultcError::PlanStale(format!(
                "conflict `{}` document IDs are not strictly ordered",
                self.conflict_id
            )));
        }
        if self.message.contains('\0') {
            return Err(VaultcError::PlanStale(format!(
                "conflict `{}` message contains NUL",
                self.conflict_id
            )));
        }
        let expected = calculate_conflict_content_hash(
            self.kind,
            self.required,
            &self.documents,
            &self.message,
            self.score,
        )?;
        if self.content_hash != expected
            || self.conflict_id != format!("conflict_{}", expected.hex())
        {
            return Err(VaultcError::PlanStale(format!(
                "conflict `{}` content hash does not match its payload",
                self.conflict_id
            )));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct InspectionIdentity<'a> {
    schema_version: u32,
    compiler_version: &'a str,
    policy_hash: ContentHash,
    snapshots: Vec<SnapshotId>,
}

fn calculate_inspection_hash(
    schema_version: u32,
    compiler_version: &str,
    policy_hash: ContentHash,
    snapshots: &[VaultSnapshot],
) -> Result<ContentHash> {
    let identity = InspectionIdentity {
        schema_version,
        compiler_version,
        policy_hash,
        snapshots: snapshots
            .iter()
            .map(|snapshot| snapshot.snapshot_id)
            .collect(),
    };
    canonical_hash("vaultc:inspection:v1\0", &identity)
}

#[derive(Serialize)]
struct PlanIdentity<'a> {
    schema_version: u32,
    compiler_version: &'a str,
    policy_hash: ContentHash,
    inspection_hash: ContentHash,
    projection_hash: ContentHash,
    output_paths: &'a BTreeMap<DocumentId, String>,
    asset_output_paths: &'a BTreeMap<AssetId, String>,
    operations: &'a [OutputOperation],
    exact_groups: &'a [ExactDuplicateGroup],
    near_candidates: &'a [NearDuplicateCandidate],
    conflicts: &'a [Conflict],
    diagnostics: &'a [Diagnostic],
}

#[allow(clippy::too_many_arguments)]
fn calculate_plan_id(
    schema_version: u32,
    compiler_version: &str,
    policy_hash: ContentHash,
    inspection_hash: ContentHash,
    projection_hash: ContentHash,
    output_paths: &BTreeMap<DocumentId, String>,
    asset_output_paths: &BTreeMap<AssetId, String>,
    operations: &[OutputOperation],
    exact_groups: &[ExactDuplicateGroup],
    near_candidates: &[NearDuplicateCandidate],
    conflicts: &[Conflict],
    diagnostics: &[Diagnostic],
) -> Result<PlanId> {
    let identity = PlanIdentity {
        schema_version,
        compiler_version,
        policy_hash,
        inspection_hash,
        projection_hash,
        output_paths,
        asset_output_paths,
        operations,
        exact_groups,
        near_candidates,
        conflicts,
        diagnostics,
    };
    Ok(PlanId::from_hash(canonical_hash(
        "vaultc:plan:v1\0",
        &identity,
    )?))
}

#[derive(Serialize)]
struct ConflictContentIdentity<'a> {
    kind: ConflictKind,
    required: bool,
    documents: &'a [DocumentId],
    message: &'a str,
    score: Option<f64>,
}

fn calculate_conflict_content_hash(
    kind: ConflictKind,
    required: bool,
    documents: &[DocumentId],
    message: &str,
    score: Option<f64>,
) -> Result<ContentHash> {
    if score.is_some_and(|score| !score.is_finite()) {
        return Err(VaultcError::PlanStale(
            "conflict similarity score must be finite".into(),
        ));
    }
    canonical_hash(
        "vaultc:conflict:v1\0",
        &ConflictContentIdentity {
            kind,
            required,
            documents,
            message,
            score,
        },
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "snapshot sealing checks are kept together so no identity invariant is skipped"
)]
fn validate_snapshot_set(snapshots: &[VaultSnapshot], policy: &CompilerPolicy) -> Result<()> {
    if snapshots.is_empty() {
        return Err(VaultcError::PlanStale(
            "a plan must contain at least one snapshot".into(),
        ));
    }
    if snapshots.len() > policy.limits.max_sources as usize {
        return Err(VaultcError::ResourceLimit(format!(
            "{} plan snapshots exceeds configured maximum {}",
            snapshots.len(),
            policy.limits.max_sources
        )));
    }

    let mut previous_source: Option<&SourceId> = None;
    let mut total_files = 0_u64;
    let mut total_bytes = 0_u64;
    for snapshot in snapshots {
        SourceId::new(snapshot.source_id.as_str())?;
        if previous_source.is_some_and(|previous| previous >= &snapshot.source_id) {
            return Err(VaultcError::PlanStale(
                "snapshots must be strictly ordered by unique source ID".into(),
            ));
        }
        previous_source = Some(&snapshot.source_id);
        if snapshot.source.source_id() != &snapshot.source_id {
            return Err(VaultcError::PlanStale(format!(
                "snapshot {} source descriptor has a different source ID",
                snapshot.snapshot_id
            )));
        }
        if snapshot.policy_id != crate::snapshot::SOURCE_POLICY_ID {
            return Err(VaultcError::PlanStale(format!(
                "snapshot {} uses unsupported source policy `{}`",
                snapshot.snapshot_id, snapshot.policy_id
            )));
        }

        let mut previous_path: Option<&str> = None;
        for file in &snapshot.files {
            if previous_path
                .is_some_and(|previous| previous.as_bytes() >= file.logical_path.as_bytes())
            {
                return Err(VaultcError::PlanStale(format!(
                    "snapshot {} file manifest is not strictly path-sorted",
                    snapshot.snapshot_id
                )));
            }
            previous_path = Some(&file.logical_path);
            validate_output_logical_path(&file.logical_path, policy)?;
            if file.source_id != snapshot.source_id || file.snapshot_id != snapshot.snapshot_id {
                return Err(VaultcError::PlanStale(format!(
                    "file `{}` is not bound to its containing snapshot",
                    file.logical_path
                )));
            }
            if FileKind::classify(&file.logical_path) != file.kind {
                return Err(VaultcError::PlanStale(format!(
                    "file `{}` has an inconsistent media classification",
                    file.logical_path
                )));
            }
            if file.byte_len > policy.limits.max_file_bytes {
                return Err(VaultcError::ResourceLimit(format!(
                    "file `{}` exceeds the configured per-file limit",
                    file.logical_path
                )));
            }
            if file.kind != FileKind::Asset
                && file.byte_len > policy.limits.max_structured_text_bytes
            {
                return Err(VaultcError::ResourceLimit(format!(
                    "structured file `{}` exceeds the configured text limit",
                    file.logical_path
                )));
            }
            let expected_file_id = crate::identity::SourceFileId::from_file(
                &file.logical_path,
                file.kind.media_family(),
                file.content_hash,
            );
            if file.file_id != expected_file_id {
                return Err(VaultcError::PlanStale(format!(
                    "file `{}` identity does not match its manifest payload",
                    file.logical_path
                )));
            }
            total_files = total_files
                .checked_add(1)
                .ok_or_else(|| VaultcError::ResourceLimit("plan file count overflow".into()))?;
            total_bytes = total_bytes
                .checked_add(file.byte_len)
                .ok_or_else(|| VaultcError::ResourceLimit("plan byte count overflow".into()))?;
        }

        let expected_snapshot_id = SnapshotId::from_manifest(
            snapshot.source_id.as_str(),
            &snapshot.policy_id,
            snapshot
                .files
                .iter()
                .map(|file| (file.logical_path.as_str(), file.file_id)),
        );
        if snapshot.snapshot_id != expected_snapshot_id {
            return Err(VaultcError::PlanStale(format!(
                "snapshot {} identity does not match its file manifest",
                snapshot.snapshot_id
            )));
        }
    }
    if total_files > policy.limits.max_files {
        return Err(VaultcError::ResourceLimit(format!(
            "plan file count exceeds {}",
            policy.limits.max_files
        )));
    }
    if total_bytes > policy.limits.max_total_bytes {
        return Err(VaultcError::ResourceLimit(format!(
            "plan source bytes exceed {}",
            policy.limits.max_total_bytes
        )));
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "IR source-closure validation covers every record kind in one exhaustive pass"
)]
fn validate_workspace_sources(
    workspace: &CanonicalWorkspace,
    snapshots: &[VaultSnapshot],
) -> Result<()> {
    if workspace.schema_version != crate::ir::IR_SCHEMA_VERSION {
        return Err(VaultcError::PlanStale(format!(
            "unsupported canonical IR schema version {}",
            workspace.schema_version
        )));
    }
    let files: BTreeMap<_, _> = snapshots
        .iter()
        .flat_map(|snapshot| snapshot.files.iter())
        .map(|file| {
            (
                (
                    file.source_id.clone(),
                    file.snapshot_id,
                    file.logical_path.clone(),
                ),
                file,
            )
        })
        .collect();
    let validate_file = |file: &crate::ir::SourceFile| -> Result<()> {
        let key = (
            file.source_id.clone(),
            file.snapshot_id,
            file.logical_path.clone(),
        );
        if files.get(&key).copied() != Some(file) {
            return Err(VaultcError::PlanStale(format!(
                "IR source `{}` is absent from its sealed snapshot",
                file.logical_path
            )));
        }
        Ok(())
    };

    for (document_id, document) in &workspace.documents {
        validate_file(&document.source_file)?;
        let expected_id = DocumentId::from_parts(
            "vaultc:document:v1\0",
            &[
                document.source_file.snapshot_id.hash().as_bytes(),
                document.source_file.file_id.hash().as_bytes(),
            ],
        );
        if *document_id != document.document_id || document.document_id != expected_id {
            return Err(VaultcError::PlanStale(format!(
                "document {} identity does not match its source",
                document.document_id
            )));
        }
        for span in document
            .sections
            .iter()
            .map(|section| &section.span)
            .chain(document.blocks.iter().map(|block| &block.span))
            .chain(document.links.iter().map(|link| &link.span))
        {
            if span.byte_start > span.byte_end || span.byte_end > document.source_file.byte_len {
                return Err(VaultcError::PlanStale(format!(
                    "document {} contains an out-of-bounds source span",
                    document.document_id
                )));
            }
        }
    }
    for (canvas_id, canvas) in &workspace.canvases {
        validate_file(&canvas.source_file)?;
        let expected_id = crate::identity::CanvasId::from_parts(
            "vaultc:canvas:v1\0",
            &[
                canvas.source_file.snapshot_id.hash().as_bytes(),
                canvas.source_file.file_id.hash().as_bytes(),
            ],
        );
        if *canvas_id != canvas.canvas_id || canvas.canvas_id != expected_id {
            return Err(VaultcError::PlanStale(format!(
                "Canvas {} identity does not match its source",
                canvas.canvas_id
            )));
        }
    }
    for (asset_id, asset) in &workspace.assets {
        if *asset_id != asset.asset_id || asset.sources.is_empty() {
            return Err(VaultcError::PlanStale(format!(
                "asset {} has an invalid identity or no source occurrences",
                asset.asset_id
            )));
        }
        let mut previous: Option<&crate::ir::SourceFile> = None;
        for source in &asset.sources {
            validate_file(source)?;
            if source.kind != FileKind::Asset {
                return Err(VaultcError::PlanStale(format!(
                    "asset {} references a non-asset source",
                    asset.asset_id
                )));
            }
            let expected_asset_id =
                AssetId::from_parts("vaultc:asset:v1\0", &[source.content_hash.as_bytes()]);
            if expected_asset_id != asset.asset_id {
                return Err(VaultcError::PlanStale(format!(
                    "asset {} groups source files with different content",
                    asset.asset_id
                )));
            }
            if previous.is_some_and(|previous| {
                (
                    &previous.source_id,
                    previous.logical_path.as_bytes(),
                    previous.file_id,
                ) >= (
                    &source.source_id,
                    source.logical_path.as_bytes(),
                    source.file_id,
                )
            }) {
                return Err(VaultcError::PlanStale(format!(
                    "asset {} sources are not strictly canonical",
                    asset.asset_id
                )));
            }
            previous = Some(source);
        }
        let expected_basename = asset
            .sources
            .first()
            .and_then(|source| Path::new(&source.logical_path).file_name())
            .and_then(|value| value.to_str())
            .unwrap_or("asset");
        if asset.basename != expected_basename {
            return Err(VaultcError::PlanStale(format!(
                "asset {} basename is not derived from its canonical source",
                asset.asset_id
            )));
        }
    }
    for base in &workspace.bases {
        validate_file(&base.source_file)?;
        if base.source_file.kind != FileKind::Base {
            return Err(VaultcError::PlanStale(
                "opaque Base IR references a non-Base source".into(),
            ));
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "operation identity, source, path, and span checks form one sealed-plan invariant"
)]
fn validate_plan_paths_and_operations(plan: &DraftPlan) -> Result<()> {
    if plan.output_paths.keys().copied().collect::<BTreeSet<_>>()
        != plan.workspace.documents.keys().copied().collect()
    {
        return Err(VaultcError::PlanStale(
            "document output path map does not cover the canonical workspace".into(),
        ));
    }
    if plan
        .asset_output_paths
        .keys()
        .copied()
        .collect::<BTreeSet<_>>()
        != plan.workspace.assets.keys().copied().collect()
    {
        return Err(VaultcError::PlanStale(
            "asset output path map does not cover the canonical workspace".into(),
        ));
    }
    for path in plan
        .output_paths
        .values()
        .chain(plan.asset_output_paths.values())
    {
        validate_output_logical_path(path, &plan.policy)?;
    }

    let files: BTreeMap<_, _> = plan
        .snapshots
        .iter()
        .flat_map(|snapshot| snapshot.files.iter())
        .map(|file| {
            (
                (
                    file.source_id.clone(),
                    file.snapshot_id,
                    file.logical_path.clone(),
                ),
                file,
            )
        })
        .collect();
    let mut previous_operation: Option<(&str, OperationId)> = None;
    let mut destinations = BTreeSet::new();
    for operation in &plan.operations {
        validate_output_logical_path(operation.destination(), &plan.policy)?;
        if !destinations.insert(portable_key(operation.destination())) {
            return Err(VaultcError::PlanStale(format!(
                "multiple operations target portable path `{}`",
                operation.destination()
            )));
        }
        let current = (operation.destination(), operation.operation_id());
        if previous_operation.is_some_and(|previous| previous >= current) {
            return Err(VaultcError::PlanStale(
                "operations must be strictly ordered by destination and operation ID".into(),
            ));
        }
        previous_operation = Some(current);

        let (source_id, snapshot_id, source_path, expected_hash) = match operation {
            OutputOperation::Copy {
                source_id,
                snapshot_id,
                source_path,
                expected_hash,
                ..
            }
            | OutputOperation::RewriteMarkdown {
                source_id,
                snapshot_id,
                source_path,
                expected_hash,
                ..
            } => (source_id, snapshot_id, source_path, expected_hash),
        };
        let source = files
            .get(&(source_id.clone(), *snapshot_id, source_path.clone()))
            .copied()
            .ok_or_else(|| {
                VaultcError::PlanStale(format!(
                    "operation source `{source_path}` is absent from the sealed snapshots"
                ))
            })?;
        if source.content_hash != *expected_hash {
            return Err(VaultcError::PlanStale(format!(
                "operation source `{source_path}` has an inconsistent content hash"
            )));
        }

        let expected_operation_id = match operation {
            OutputOperation::Copy {
                operation_id: _,
                destination,
                kind,
                ..
            } => {
                if source.kind != *kind {
                    return Err(VaultcError::PlanStale(format!(
                        "copy operation source `{source_path}` has the wrong kind"
                    )));
                }
                operation_id(&("copy", source_id, source_path, destination, expected_hash))?
            }
            OutputOperation::RewriteMarkdown {
                operation_id: _,
                destination,
                replacements,
                ..
            } => {
                if source.kind != FileKind::Markdown {
                    return Err(VaultcError::PlanStale(format!(
                        "rewrite operation source `{source_path}` is not Markdown"
                    )));
                }
                let mut previous_end = 0_u64;
                for replacement in replacements {
                    if replacement.span.byte_start < previous_end
                        || replacement.span.byte_start > replacement.span.byte_end
                        || replacement.span.byte_end > source.byte_len
                    {
                        return Err(VaultcError::PlanStale(format!(
                            "rewrite operation for `{source_path}` has overlapping or invalid spans"
                        )));
                    }
                    previous_end = replacement.span.byte_end;
                }
                operation_id(&(
                    "rewrite_markdown",
                    source_id,
                    source_path,
                    destination,
                    replacements,
                ))?
            }
        };
        if operation.operation_id() != expected_operation_id {
            return Err(VaultcError::PlanStale(format!(
                "operation {} identity does not match its payload",
                operation.operation_id()
            )));
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "the public planning stage intentionally sequences all deterministic subphases"
)]
pub fn build_plan(inspection: &Inspection, policy: &CompilerPolicy) -> Result<DraftPlan> {
    policy.validate()?;
    if inspection.policy_hash != policy.semantic_hash()? {
        return Err(VaultcError::PlanStale(
            "inspection policy hash does not match current policy".into(),
        ));
    }
    let mut workspace = inspection.workspace.clone();
    let dedup = crate::dedup::analyze(workspace.documents.values(), &policy.dedup)?;
    let representative_map = representative_map(&workspace, &dedup);
    let (mut output_paths, mut conflicts) =
        allocate_document_paths(&workspace, &representative_map, policy)?;
    for (member, representative) in &representative_map {
        if let Some(path) = output_paths.get(representative).cloned() {
            output_paths.insert(*member, path);
        }
    }
    let (asset_output_paths, asset_path_diagnostics) = allocate_asset_paths(&workspace, policy)?;

    add_lookup_conflicts(&workspace, &representative_map, &mut conflicts)?;
    add_frontmatter_conflicts(&workspace, &mut conflicts)?;
    for candidate in &dedup.near_candidates {
        conflicts.push(conflict(
            ConflictKind::ContentNearDuplicate,
            false,
            vec![candidate.left, candidate.right],
            format!(
                "near-duplicate review candidate at {:.6}",
                candidate.estimated_similarity
            ),
            Some(candidate.estimated_similarity),
            ConflictResolution::Unresolved,
        )?);
    }

    let mut diagnostics = inspection.diagnostics.clone();
    diagnostics.extend(asset_path_diagnostics);
    for document_id in &dedup.truncated_documents {
        diagnostics.push(Diagnostic {
            code: DiagnosticCode::NearDuplicateCandidate,
            severity: Severity::Warning,
            message: "near-duplicate candidate pool was truncated by the configured resource bound"
                .into(),
            logical_path: workspace
                .documents
                .get(document_id)
                .map(|document| document.source_file.logical_path.clone()),
            source_file_id: workspace
                .documents
                .get(document_id)
                .map(|document| document.source_file.file_id),
            document_id: Some(*document_id),
            span: None,
            remediation: Some(
                "review common templates or raise max_candidates_per_document explicitly".into(),
            ),
        });
    }
    resolve_links(
        &mut workspace,
        &representative_map,
        &output_paths,
        &asset_output_paths,
        &mut conflicts,
        &mut diagnostics,
    )?;

    let (operations, operation_conflicts) = build_operations(
        &workspace,
        &representative_map,
        &output_paths,
        &asset_output_paths,
        policy,
    )?;
    conflicts.extend(operation_conflicts);
    conflicts.sort_by(|left, right| left.conflict_id.cmp(&right.conflict_id));
    diagnostics.sort_by(|left, right| {
        (&left.logical_path, left.code, &left.message).cmp(&(
            &right.logical_path,
            right.code,
            &right.message,
        ))
    });

    let projection_hash = projection_hash(&workspace)?;
    let plan_id = calculate_plan_id(
        PLAN_SCHEMA_VERSION,
        env!("CARGO_PKG_VERSION"),
        policy.semantic_hash()?,
        inspection.inspection_hash,
        projection_hash,
        &output_paths,
        &asset_output_paths,
        &operations,
        &dedup.exact_groups,
        &dedup.near_candidates,
        &conflicts,
        &diagnostics,
    )?;
    let plan = DraftPlan {
        schema_version: PLAN_SCHEMA_VERSION,
        compiler_version: env!("CARGO_PKG_VERSION").into(),
        plan_id,
        projection_hash,
        policy: policy.clone(),
        inspection_hash: inspection.inspection_hash,
        snapshots: inspection.snapshots.clone(),
        workspace,
        output_paths,
        asset_output_paths,
        operations,
        exact_groups: dedup.exact_groups,
        near_candidates: dedup.near_candidates,
        conflicts,
        diagnostics,
    };
    plan.validate_integrity()?;
    Ok(plan)
}

fn representative_map(
    workspace: &CanonicalWorkspace,
    report: &DedupReport,
) -> BTreeMap<DocumentId, DocumentId> {
    let mut map: BTreeMap<_, _> = workspace
        .documents
        .keys()
        .copied()
        .map(|document_id| (document_id, document_id))
        .collect();
    for group in &report.exact_groups {
        for member in &group.members {
            map.insert(*member, group.canonical);
        }
    }
    map
}

fn allocate_document_paths(
    workspace: &CanonicalWorkspace,
    representative_map: &BTreeMap<DocumentId, DocumentId>,
    policy: &CompilerPolicy,
) -> Result<(BTreeMap<DocumentId, String>, Vec<Conflict>)> {
    let representatives: BTreeSet<_> = representative_map.values().copied().collect();
    let mut documents: Vec<_> = representatives
        .into_iter()
        .filter_map(|id| workspace.documents.get(&id))
        .collect();
    documents.sort_by_key(|document| allocation_tuple(document));
    let mut allocated: BTreeMap<String, DocumentId> = BTreeMap::new();
    let mut output: BTreeMap<DocumentId, String> = BTreeMap::new();
    let mut conflicts = Vec::new();
    for document in documents {
        let preferred = format!("knowledge/{}", document.source_file.logical_path);
        validate_output_logical_path(&preferred, policy)?;
        let key = portable_key(&preferred);
        let destination = if let Some(existing) = allocated.get(&key) {
            let existing_path = output.get(existing).ok_or_else(|| {
                VaultcError::Internal("allocated document path is missing".into())
            })?;
            let kind = classify_path_collision(existing_path, &preferred);
            let mut length = 8;
            let candidate = loop {
                let candidate = insert_suffix(
                    &preferred,
                    &format!("~{}", document.document_id.suffix_base32(length)),
                    policy.limits.max_component_bytes,
                );
                if !allocated.contains_key(&portable_key(&candidate)) {
                    break candidate;
                }
                length += 1;
                if length > 52 {
                    return Err(VaultcError::Internal(
                        "full document ID could not resolve path collision".into(),
                    ));
                }
            };
            conflicts.push(conflict(
                kind,
                false,
                vec![*existing, document.document_id],
                format!("portable path collision at `{preferred}`; allocated `{candidate}`"),
                None,
                ConflictResolution::AutoResolvedByNormativeRule,
            )?);
            candidate
        } else {
            preferred
        };
        allocated.insert(portable_key(&destination), document.document_id);
        output.insert(document.document_id, destination);
    }
    Ok((output, conflicts))
}

fn allocate_asset_paths(
    workspace: &CanonicalWorkspace,
    policy: &CompilerPolicy,
) -> Result<(BTreeMap<AssetId, String>, Vec<Diagnostic>)> {
    let mut output = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (asset_id, asset) in &workspace.assets {
        let source = asset.canonical_source().ok_or_else(|| {
            VaultcError::Internal(format!("asset {asset_id} has no source occurrences"))
        })?;
        let hash = source.content_hash.hex();
        let basename_budget = policy
            .limits
            .max_component_bytes
            .checked_sub(hash.len() + 1)
            .filter(|budget| *budget > 0)
            .ok_or_else(|| {
                VaultcError::InvalidConfig(
                    "component byte limit is too small for content-addressed attachments".into(),
                )
            })?;
        let basename = truncate_filename(&asset.basename, basename_budget);
        if basename != asset.basename {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::PathSanitized,
                    format!("attachment basename truncated to `{basename}` for portability"),
                )
                .for_path(source.logical_path.clone()),
            );
        }
        let destination = format!("attachments/{}/{}-{}", &hash[..2], hash, basename);
        validate_output_logical_path(&destination, policy)?;
        output.insert(*asset_id, destination);
    }
    Ok((output, diagnostics))
}

fn add_lookup_conflicts(
    workspace: &CanonicalWorkspace,
    representative_map: &BTreeMap<DocumentId, DocumentId>,
    conflicts: &mut Vec<Conflict>,
) -> Result<()> {
    let mut title_index: BTreeMap<String, Vec<DocumentId>> = BTreeMap::new();
    let mut alias_index: BTreeMap<String, Vec<DocumentId>> = BTreeMap::new();
    for document in workspace.documents.values() {
        let representative = representative_map[&document.document_id];
        if let Some(title) = &document.title {
            title_index
                .entry(crate::parse::normalize_lookup_key(title))
                .or_default()
                .push(representative);
        }
        if let Some(stem) = Path::new(&document.source_file.logical_path)
            .file_stem()
            .and_then(|value| value.to_str())
        {
            title_index
                .entry(crate::parse::normalize_lookup_key(stem))
                .or_default()
                .push(representative);
        }
        for alias in &document.aliases {
            alias_index
                .entry(crate::parse::normalize_lookup_key(alias))
                .or_default()
                .push(representative);
        }
    }
    add_ambiguity_conflicts(
        title_index,
        ConflictKind::TitleAmbiguity,
        "title lookup key",
        conflicts,
    )?;
    add_ambiguity_conflicts(
        alias_index,
        ConflictKind::AliasAmbiguity,
        "alias lookup key",
        conflicts,
    )?;
    Ok(())
}

fn add_ambiguity_conflicts(
    index: BTreeMap<String, Vec<DocumentId>>,
    kind: ConflictKind,
    label: &str,
    conflicts: &mut Vec<Conflict>,
) -> Result<()> {
    for (key, mut documents) in index {
        documents.sort();
        documents.dedup();
        if documents.len() > 1 {
            conflicts.push(conflict(
                kind,
                false,
                documents,
                format!("{label} `{key}` has multiple candidates"),
                None,
                ConflictResolution::Unresolved,
            )?);
        }
    }
    Ok(())
}

fn add_frontmatter_conflicts(
    workspace: &CanonicalWorkspace,
    conflicts: &mut Vec<Conflict>,
) -> Result<()> {
    let mut by_body: BTreeMap<ContentHash, Vec<&Document>> = BTreeMap::new();
    for document in workspace.documents.values() {
        by_body
            .entry(document.body_hash)
            .or_default()
            .push(document);
    }
    for documents in by_body.values() {
        let metadata: BTreeSet<_> = documents
            .iter()
            .map(|document| document.frontmatter_hash)
            .collect();
        if documents.len() > 1 && metadata.len() > 1 {
            conflicts.push(conflict(
                ConflictKind::FrontmatterValue,
                false,
                documents.iter().map(|document| document.document_id).collect(),
                "equivalent bodies have differing normalized frontmatter; retaining each source-specific note"
                    .into(),
                None,
                ConflictResolution::AutoResolvedByNormativeRule,
            )?);
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "link resolution keeps all terminal states adjacent to prevent implicit fallbacks"
)]
fn resolve_links(
    workspace: &mut CanonicalWorkspace,
    representative_map: &BTreeMap<DocumentId, DocumentId>,
    output_paths: &BTreeMap<DocumentId, String>,
    asset_output_paths: &BTreeMap<AssetId, String>,
    conflicts: &mut Vec<Conflict>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let documents = workspace.documents.clone();
    let mut path_index: BTreeMap<(SourceId, String), Vec<DocumentId>> = BTreeMap::new();
    let mut key_index: BTreeMap<String, Vec<DocumentId>> = BTreeMap::new();
    for document in documents.values() {
        let representative = representative_map[&document.document_id];
        path_index
            .entry((
                document.source_file.source_id.clone(),
                normalize_link_path(&document.source_file.logical_path),
            ))
            .or_default()
            .push(representative);
        for key in crate::parse::lookup_keys(document) {
            key_index.entry(key).or_default().push(representative);
        }
    }
    let mut asset_index: BTreeMap<(SourceId, String), AssetId> = BTreeMap::new();
    for (asset_id, asset) in &workspace.assets {
        for source in &asset.sources {
            asset_index.insert(
                (
                    source.source_id.clone(),
                    normalize_link_path(&source.logical_path),
                ),
                *asset_id,
            );
        }
    }

    for document in workspace.documents.values_mut() {
        let document_id = representative_map[&document.document_id];
        let source_id = document.source_file.source_id.clone();
        let logical_path = document.source_file.logical_path.clone();
        let source_file_id = document.source_file.file_id;
        for link in &mut document.links {
            let mut candidates = resolve_document_candidates(
                &source_id,
                &logical_path,
                link.path.as_deref(),
                &path_index,
                &key_index,
            );
            candidates.retain(|candidate| {
                documents.get(candidate).is_some_and(|candidate| {
                    fragment_matches(candidate, link.heading.as_deref(), link.block_id.as_deref())
                })
            });
            if candidates.len() == 1 {
                link.resolution = LinkResolution::Resolved {
                    document_id: candidates[0],
                };
            } else if candidates.len() > 1 {
                link.resolution = LinkResolution::Ambiguous {
                    candidates: candidates.clone(),
                };
                let mut conflict_documents = vec![document_id];
                conflict_documents.extend(candidates);
                conflict_documents.sort();
                conflict_documents.dedup();
                conflicts.push(conflict(
                    ConflictKind::LinkAmbiguity,
                    true,
                    conflict_documents,
                    format!("link `{}` resolves to multiple documents", link.raw_target),
                    None,
                    ConflictResolution::Unresolved,
                )?);
            } else if let Some(path) = link.path.as_deref() {
                let asset_id =
                    resolve_relative_source_path(&logical_path, path).and_then(|resolved_path| {
                        asset_index
                            .get(&(source_id.clone(), normalize_link_path(&resolved_path)))
                            .copied()
                    });
                if let Some(asset_id) = asset_id {
                    if asset_output_paths.contains_key(&asset_id) {
                        link.resolution = LinkResolution::Asset { asset_id };
                    }
                } else {
                    link.resolution = LinkResolution::Unresolved;
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::LinkUnresolved,
                        severity: Severity::Warning,
                        message: format!("unresolved link `{}`", link.raw_target),
                        logical_path: Some(logical_path.clone()),
                        source_file_id: Some(source_file_id),
                        document_id: Some(document_id),
                        span: Some(link.span.clone()),
                        remediation: None,
                    });
                }
            } else if link.heading.is_some() || link.block_id.is_some() {
                if documents.get(&document_id).is_some_and(|candidate| {
                    fragment_matches(candidate, link.heading.as_deref(), link.block_id.as_deref())
                }) {
                    link.resolution = LinkResolution::Resolved { document_id };
                } else {
                    link.resolution = LinkResolution::Unresolved;
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::LinkUnresolved,
                        severity: Severity::Warning,
                        message: format!("unresolved heading/block link `{}`", link.raw_target),
                        logical_path: Some(logical_path.clone()),
                        source_file_id: Some(source_file_id),
                        document_id: Some(document_id),
                        span: Some(link.span.clone()),
                        remediation: None,
                    });
                }
            } else {
                link.resolution = LinkResolution::Unresolved;
            }
        }
    }

    let _ = output_paths;
    Ok(())
}

fn fragment_matches(document: &Document, heading: Option<&str>, block_id: Option<&str>) -> bool {
    let heading_matches = heading.is_none_or(|heading| {
        heading.is_empty()
            || document.sections.iter().any(|section| {
                crate::parse::normalize_lookup_key(&section.heading)
                    == crate::parse::normalize_lookup_key(heading)
            })
    });
    let block_matches = block_id.is_none_or(|block_id| {
        document
            .blocks
            .iter()
            .any(|block| block.explicit_id.as_deref() == Some(block_id))
    });
    heading_matches && block_matches
}

fn resolve_document_candidates(
    source_id: &SourceId,
    logical_path: &str,
    target: Option<&str>,
    path_index: &BTreeMap<(SourceId, String), Vec<DocumentId>>,
    key_index: &BTreeMap<String, Vec<DocumentId>>,
) -> Vec<DocumentId> {
    let Some(target) = target else {
        return Vec::new();
    };
    let Some(relative) = resolve_relative_source_path(logical_path, target) else {
        return Vec::new();
    };
    let mut candidates = path_index
        .get(&(source_id.clone(), normalize_link_path(&relative)))
        .cloned()
        .unwrap_or_default();
    if candidates.is_empty() && !relative.to_ascii_lowercase().ends_with(".md") {
        candidates = path_index
            .get(&(
                source_id.clone(),
                normalize_link_path(&format!("{relative}.md")),
            ))
            .cloned()
            .unwrap_or_default();
    }
    if candidates.is_empty() {
        let stem = Path::new(target)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(target);
        candidates = key_index
            .get(&crate::parse::normalize_lookup_key(stem))
            .cloned()
            .unwrap_or_default();
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

#[allow(
    clippy::too_many_lines,
    reason = "operation materialization exhaustively covers notes, assets, canvases, and Bases"
)]
fn build_operations(
    workspace: &CanonicalWorkspace,
    representative_map: &BTreeMap<DocumentId, DocumentId>,
    output_paths: &BTreeMap<DocumentId, String>,
    asset_output_paths: &BTreeMap<AssetId, String>,
    policy: &CompilerPolicy,
) -> Result<(Vec<OutputOperation>, Vec<Conflict>)> {
    let representatives: BTreeSet<_> = representative_map.values().copied().collect();
    let mut operations = Vec::new();
    for document_id in representatives {
        let document = &workspace.documents[&document_id];
        let destination = output_paths[&document_id].clone();
        let mut replacements = Vec::new();
        for link in &document.links {
            let target_output = match &link.resolution {
                LinkResolution::Resolved {
                    document_id: target,
                } if link.path.is_none() && *target == document_id => None,
                LinkResolution::Resolved { document_id } => output_paths.get(document_id),
                LinkResolution::Asset { asset_id } => asset_output_paths.get(asset_id),
                _ => None,
            };
            if let Some(target_output) = target_output {
                let relative = relative_output_target(&destination, target_output);
                let replacement = render_link_target(link, &relative);
                if replacement != link.raw_target {
                    replacements.push(RewriteReplacement {
                        span: link.span.clone(),
                        replacement,
                    });
                }
            }
        }
        replacements.sort_by_key(|replacement| replacement.span.byte_start);
        let operation = if replacements.is_empty() {
            copy_operation(&document.source_file, destination, FileKind::Markdown)?
        } else {
            let operation_id = operation_id(&(
                "rewrite_markdown",
                &document.source_file.source_id,
                &document.source_file.logical_path,
                &destination,
                &replacements,
            ))?;
            OutputOperation::RewriteMarkdown {
                operation_id,
                source_id: document.source_file.source_id.clone(),
                snapshot_id: document.source_file.snapshot_id,
                source_path: document.source_file.logical_path.clone(),
                destination,
                expected_hash: document.source_file.content_hash,
                replacements,
            }
        };
        operations.push(operation);
    }
    for (asset_id, asset) in &workspace.assets {
        let source = asset.canonical_source().ok_or_else(|| {
            VaultcError::Internal(format!("asset {asset_id} has no source occurrences"))
        })?;
        operations.push(copy_operation(
            source,
            asset_output_paths[asset_id].clone(),
            FileKind::Asset,
        )?);
    }
    let mut allocated: BTreeMap<String, String> = operations
        .iter()
        .map(|operation| {
            (
                portable_key(operation.destination()),
                operation.destination().to_owned(),
            )
        })
        .collect();
    let mut path_conflicts = Vec::new();
    for canvas in workspace.canvases.values() {
        let preferred = format!("canvases/{}", canvas.source_file.logical_path);
        let (destination, collision) = allocate_auxiliary_path(
            &preferred,
            &canvas.canvas_id.suffix_base32(52),
            &mut allocated,
            policy,
        )?;
        if let Some((kind, existing)) = collision {
            path_conflicts.push(conflict(
                kind,
                false,
                Vec::new(),
                format!(
                    "portable Canvas path collision between `{existing}` and `{preferred}`; allocated `{destination}`"
                ),
                None,
                ConflictResolution::AutoResolvedByNormativeRule,
            )?);
        }
        operations.push(copy_operation(
            &canvas.source_file,
            destination,
            FileKind::Canvas,
        )?);
    }
    let mut bases: Vec<_> = workspace.bases.iter().collect();
    bases.sort_by(|left, right| {
        left.source_file
            .source_id
            .cmp(&right.source_file.source_id)
            .then_with(|| {
                left.source_file
                    .logical_path
                    .as_bytes()
                    .cmp(right.source_file.logical_path.as_bytes())
            })
            .then_with(|| left.source_file.file_id.cmp(&right.source_file.file_id))
    });
    for base in bases {
        let preferred = format!("views/{}", base.source_file.logical_path);
        let (destination, collision) = allocate_auxiliary_path(
            &preferred,
            &base.source_file.file_id.suffix_base32(52),
            &mut allocated,
            policy,
        )?;
        if let Some((kind, existing)) = collision {
            path_conflicts.push(conflict(
                kind,
                false,
                Vec::new(),
                format!(
                    "portable Base path collision between `{existing}` and `{preferred}`; allocated `{destination}`"
                ),
                None,
                ConflictResolution::AutoResolvedByNormativeRule,
            )?);
        }
        operations.push(copy_operation(
            &base.source_file,
            destination,
            FileKind::Base,
        )?);
    }
    operations.sort_by(|left, right| {
        left.destination()
            .as_bytes()
            .cmp(right.destination().as_bytes())
            .then_with(|| left.operation_id().cmp(&right.operation_id()))
    });
    let mut destinations = BTreeSet::new();
    for operation in &operations {
        if !destinations.insert(portable_key(operation.destination())) {
            return Err(VaultcError::Internal(format!(
                "operation destination collision at `{}`",
                operation.destination()
            )));
        }
    }
    Ok((operations, path_conflicts))
}

fn allocate_auxiliary_path(
    preferred: &str,
    identity_suffix: &str,
    allocated: &mut BTreeMap<String, String>,
    policy: &CompilerPolicy,
) -> Result<(String, Option<(ConflictKind, String)>)> {
    validate_output_logical_path(preferred, policy)?;
    let key = portable_key(preferred);
    let Some(existing) = allocated.get(&key).cloned() else {
        allocated.insert(key, preferred.to_owned());
        return Ok((preferred.to_owned(), None));
    };
    let kind = classify_path_collision(&existing, preferred);
    let mut length = 8_usize;
    let destination = loop {
        let suffix = &identity_suffix[..length.min(identity_suffix.len())];
        let candidate = insert_suffix(
            preferred,
            &format!("~{suffix}"),
            policy.limits.max_component_bytes,
        );
        let candidate_key = portable_key(&candidate);
        if let std::collections::btree_map::Entry::Vacant(entry) = allocated.entry(candidate_key) {
            validate_output_logical_path(&candidate, policy)?;
            entry.insert(candidate.clone());
            break candidate;
        }
        if length >= identity_suffix.len() {
            return Err(VaultcError::Internal(
                "full artifact identity could not resolve path collision".into(),
            ));
        }
        length += 1;
    };
    Ok((destination, Some((kind, existing))))
}

fn copy_operation(
    file: &crate::ir::SourceFile,
    destination: String,
    kind: FileKind,
) -> Result<OutputOperation> {
    let operation_id = operation_id(&(
        "copy",
        &file.source_id,
        &file.logical_path,
        &destination,
        file.content_hash,
    ))?;
    Ok(OutputOperation::Copy {
        operation_id,
        source_id: file.source_id.clone(),
        snapshot_id: file.snapshot_id,
        source_path: file.logical_path.clone(),
        destination,
        expected_hash: file.content_hash,
        kind,
    })
}

fn operation_id<T: Serialize>(value: &T) -> Result<OperationId> {
    Ok(OperationId::from_hash(canonical_hash(
        "vaultc:operation:v1\0",
        value,
    )?))
}

fn projection_hash(workspace: &CanonicalWorkspace) -> Result<ContentHash> {
    canonical_hash("vaultc:projection:v1\0", workspace)
}

fn conflict(
    kind: ConflictKind,
    required: bool,
    mut documents: Vec<DocumentId>,
    message: String,
    score: Option<f64>,
    resolution: ConflictResolution,
) -> Result<Conflict> {
    documents.sort();
    documents.dedup();
    let content_hash =
        calculate_conflict_content_hash(kind, required, &documents, &message, score)?;
    Ok(Conflict {
        conflict_id: format!("conflict_{}", content_hash.hex()),
        content_hash,
        kind,
        required,
        resolution,
        documents,
        message,
        score,
    })
}

fn allocation_tuple(document: &Document) -> (String, String, SnapshotId, DocumentId) {
    (
        document.source_file.logical_path.nfc().collect(),
        document.source_file.source_id.to_string(),
        document.source_file.snapshot_id,
        document.document_id,
    )
}

fn portable_key(path: &str) -> String {
    path.to_lowercase().nfc().collect()
}

fn classify_path_collision(existing: &str, requested: &str) -> ConflictKind {
    if existing == requested {
        ConflictKind::PathExact
    } else if existing.nfc().collect::<String>() == requested.nfc().collect::<String>() {
        ConflictKind::UnicodeNormalization
    } else {
        ConflictKind::PathCasefold
    }
}

fn insert_suffix(path: &str, suffix: &str, max_component_bytes: usize) -> String {
    let path = Path::new(path);
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty());
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    let extension = extension
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let budget = max_component_bytes.saturating_sub(suffix.len() + extension.len());
    let stem = truncate_utf8(stem, budget);
    let name = format!("{stem}{suffix}{extension}");
    parent.map_or(name.clone(), |parent| {
        format!("{}/{}", parent.display(), name)
    })
}

fn truncate_filename(filename: &str, maximum_bytes: usize) -> String {
    if filename.len() <= maximum_bytes {
        return filename.to_owned();
    }
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let extension = if extension.len() < maximum_bytes {
        extension
    } else {
        String::new()
    };
    let stem_budget = maximum_bytes.saturating_sub(extension.len());
    let mut stem = truncate_utf8(stem, stem_budget).to_owned();
    if stem.is_empty() {
        truncate_utf8("asset", stem_budget).clone_into(&mut stem);
    }
    format!("{stem}{extension}")
}

fn truncate_utf8(value: &str, maximum_bytes: usize) -> &str {
    let mut end = value.len().min(maximum_bytes);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn normalize_link_path(path: &str) -> String {
    path.trim_start_matches("./")
        .replace('\\', "/")
        .to_lowercase()
        .nfc()
        .collect()
}

fn resolve_relative_source_path(current: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = Path::new(current)
        .parent()
        .and_then(Path::to_str)
        .unwrap_or_default()
        .split('/')
        .filter(|value| !value.is_empty())
        .collect();
    if target.starts_with('/') {
        parts.clear();
    }
    let normalized_target = target.trim_start_matches('/').replace('\\', "/");
    for component in normalized_target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            value => parts.push(value),
        }
    }
    Some(parts.join("/"))
}

fn relative_output_target(from: &str, to: &str) -> String {
    let from_parent: Vec<_> = Path::new(from)
        .parent()
        .and_then(Path::to_str)
        .unwrap_or_default()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to_parts: Vec<_> = to.split('/').filter(|part| !part.is_empty()).collect();
    let common = from_parent
        .iter()
        .zip(&to_parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = vec![".."; from_parent.len().saturating_sub(common)];
    result.extend_from_slice(&to_parts[common..]);
    if result.is_empty() {
        PathBuf::from(to)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(to)
            .to_owned()
    } else {
        result.join("/")
    }
}

fn render_link_target(link: &crate::ir::Link, path: &str) -> String {
    let markdown = link.syntax == crate::ir::LinkSyntax::Markdown;
    let mut target = if markdown {
        encode_markdown_destination_component(path)
    } else {
        path.to_owned()
    };
    if let Some(heading) = &link.heading {
        target.push('#');
        if markdown {
            target.push_str(&encode_markdown_destination_component(heading));
        } else {
            target.push_str(heading);
        }
    }
    if let Some(block_id) = &link.block_id {
        target.push('^');
        if markdown {
            target.push_str(&encode_markdown_destination_component(block_id));
        } else {
            target.push_str(block_id);
        }
    }
    if link.syntax == crate::ir::LinkSyntax::Wiki
        && let Some(display) = &link.display
    {
        target.push('|');
        target.push_str(display);
    }
    target
}

fn encode_markdown_destination_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            ' ' => encoded.push_str("%20"),
            '%' => encoded.push_str("%25"),
            '#' => encoded.push_str("%23"),
            '(' => encoded.push_str("%28"),
            ')' => encoded.push_str("%29"),
            '<' => encoded.push_str("%3C"),
            '>' => encoded.push_str("%3E"),
            '\\' => encoded.push_str("%5C"),
            _ => encoded.push(character),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_relative_paths_are_stable() {
        assert_eq!(
            relative_output_target("knowledge/a/A.md", "knowledge/b/B.md"),
            "../b/B.md"
        );
        assert_eq!(
            relative_output_target("knowledge/A.md", "attachments/aa/file.png"),
            "../attachments/aa/file.png"
        );
    }

    #[test]
    fn collision_kind_distinguishes_exact_case_and_normalization() {
        assert_eq!(
            classify_path_collision("knowledge/Topic.md", "knowledge/Topic.md"),
            ConflictKind::PathExact
        );
        assert_eq!(
            classify_path_collision("knowledge/Topic.md", "knowledge/topic.md"),
            ConflictKind::PathCasefold
        );
        assert_eq!(
            classify_path_collision("knowledge/Caf\u{e9}.md", "knowledge/Cafe\u{301}.md"),
            ConflictKind::UnicodeNormalization
        );
    }

    #[test]
    fn markdown_rewrites_escape_destination_delimiters() {
        assert_eq!(
            encode_markdown_destination_component("../Topic (one)%/A.md"),
            "../Topic%20%28one%29%25/A.md"
        );
    }
}
