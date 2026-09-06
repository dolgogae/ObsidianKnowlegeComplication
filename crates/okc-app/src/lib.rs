//! Long-lived OKC project storage and application orchestration primitives.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use okc_core::{CancellationToken, SourceId};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub mod artifact_service;
mod integration_execution;
pub mod integration_service;
pub mod provider_service;
pub mod v3;
pub mod worker;
pub mod workspace_bootstrap;

pub use artifact_service::{
    ArtifactExplanation, ArtifactFamily, ArtifactService, ArtifactVerification,
};
pub use integration_execution::IntegrationExecution;
pub use integration_service::{IntegrationCheckpoint, IntegrationService};
pub use provider_service::{CredentialStore, ProviderService};
pub use workspace_bootstrap::{VaultCandidate, WorkspaceBootstrap, WorkspaceDiscovery};

pub const PROJECT_SCHEMA_VERSION: u32 = 3;
pub const APPLICATION_STATE_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Core(#[from] okc_core::OkcError),
    #[error(transparent)]
    Provider(#[from] okc_ai::ProviderError),
    #[error(transparent)]
    Credential(#[from] provider_service::CredentialError),
    #[error("invalid OKC project: {0}")]
    InvalidProject(String),
    #[error("project is already locked: {0}")]
    Locked(PathBuf),
    #[error("update unavailable: {0}")]
    Update(String),
    #[error("artifact error: {0}")]
    Artifact(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(feature = "updater")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateTarget {
    Stable,
    Latest,
    Exact(String),
}

#[cfg(feature = "updater")]
impl UpdateTarget {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "stable" => Ok(Self::Stable),
            "latest" => Ok(Self::Latest),
            exact => {
                let parsed = axoupdater::Version::parse(exact).map_err(|error| {
                    AppError::Update(format!("invalid version `{exact}`: {error}"))
                })?;
                if parsed.to_string() != exact {
                    return Err(AppError::Update(format!(
                        "exact version must use canonical semantic-version spelling: `{exact}`"
                    )));
                }
                Ok(Self::Exact(exact.to_owned()))
            }
        }
    }

    fn request(&self) -> axoupdater::UpdateRequest {
        match self {
            Self::Stable => axoupdater::UpdateRequest::Latest,
            Self::Latest => axoupdater::UpdateRequest::LatestMaybePrerelease,
            Self::Exact(version) => axoupdater::UpdateRequest::SpecificVersion(version.clone()),
        }
    }
}

#[cfg(feature = "updater")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    pub update_needed: bool,
}

#[cfg(feature = "updater")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledUpdate {
    pub old_version: Option<String>,
    pub new_version: String,
    pub release_tag: String,
    pub install_prefix: String,
}

#[cfg(feature = "updater")]
fn configured_updater(target: &UpdateTarget) -> Result<axoupdater::AxoUpdater> {
    let mut updater = axoupdater::AxoUpdater::new_for("okc");
    updater.load_receipt().map_err(|error| {
        AppError::Update(format!(
            "no usable cargo-dist receipt for this installation: {error}"
        ))
    })?;
    let is_installer_copy = updater
        .check_receipt_is_for_this_executable()
        .map_err(|error| AppError::Update(error.to_string()))?;
    if !is_installer_copy {
        return Err(AppError::Update(
            "the running executable is a cargo/manual copy; refusing to overwrite it".into(),
        ));
    }
    updater.configure_version_specifier(target.request());
    Ok(updater)
}

/// Check the receipt-bound channel without installing anything.
#[cfg(feature = "updater")]
pub fn check_for_update(target: &UpdateTarget) -> Result<UpdateStatus> {
    let mut updater = configured_updater(target)?;
    let update_needed = updater
        .is_update_needed_sync()
        .map_err(|error| AppError::Update(error.to_string()))?;
    Ok(UpdateStatus { update_needed })
}

/// Install only into the prefix named by the matching cargo-dist receipt.
/// The caller owns the interactive confirmation boundary.
#[cfg(feature = "updater")]
pub fn install_update(target: &UpdateTarget) -> Result<Option<InstalledUpdate>> {
    let mut updater = configured_updater(target)?;
    let result = updater
        .run_sync()
        .map_err(|error| AppError::Update(error.to_string()))?;
    Ok(result.map(|result| InstalledUpdate {
        old_version: result.old_version.map(|version| version.to_string()),
        new_version: result.new_version.to_string(),
        release_tag: result.new_version_tag,
        install_prefix: result.install_prefix.to_string(),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBinding {
    pub source_id: SourceId,
    pub owner_display_name: Option<String>,
    pub path: PathBuf,
    pub snapshot_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub format_family: String,
    pub schema_version: u32,
    pub product_version: String,
    pub name: String,
    pub curator_id: String,
    pub policy_version: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub ai_routes: v3::AiRouteConfig,
    pub sources: Vec<SourceBinding>,
}

impl ProjectManifest {
    fn validate(&self) -> Result<()> {
        if self.format_family != "okc" || self.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(AppError::InvalidProject(format!(
                "expected okc schema {PROJECT_SCHEMA_VERSION}, got {} schema {}",
                self.format_family, self.schema_version
            )));
        }
        if self.name.trim().is_empty()
            || self.curator_id.trim().is_empty()
            || self.policy_version.trim().is_empty()
            || self
                .name
                .chars()
                .chain(self.curator_id.chars())
                .chain(self.policy_version.chars())
                .any(char::is_control)
        {
            return Err(AppError::InvalidProject(
                "name, curator_id, or policy_version is empty or unsafe".into(),
            ));
        }
        if let Some(language) = &self.language {
            v3::validate_bcp47(language)?;
        }
        self.ai_routes.validate()?;
        if self.sources.len() > 10 {
            return Err(AppError::InvalidProject(
                "a project supports at most 10 sources".into(),
            ));
        }
        let mut ids: Vec<_> = self
            .sources
            .iter()
            .map(|source| &source.source_id)
            .collect();
        ids.sort();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(AppError::InvalidProject(
                "source IDs must be unique within a project".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ProjectStore {
    root: PathBuf,
    manifest: ProjectManifest,
}

impl ProjectStore {
    pub fn create(
        root: impl AsRef<Path>,
        name: impl Into<String>,
        curator_id: impl Into<String>,
        policy_version: impl Into<String>,
    ) -> Result<Self> {
        let root = root.as_ref();
        validate_project_path(root)?;
        // Validate all caller-controlled manifest fields before creating any
        // directories so rejected SDK/CLI input cannot leave a partial project.
        let manifest = ProjectManifest {
            format_family: "okc".into(),
            schema_version: PROJECT_SCHEMA_VERSION,
            product_version: env!("CARGO_PKG_VERSION").into(),
            name: name.into(),
            curator_id: curator_id.into(),
            policy_version: policy_version.into(),
            language: None,
            ai_routes: v3::AiRouteConfig::default(),
            sources: Vec::new(),
        };
        manifest.validate()?;
        if let Some(parent) = root.parent().filter(|value| !value.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(root)?;
        let root = fs::canonicalize(root)?;
        set_private_directory(&root)?;
        fs::create_dir(root.join("objects"))?;
        fs::create_dir(root.join("workspace"))?;
        set_private_directory(&root.join("objects"))?;
        set_private_directory(&root.join("workspace"))?;
        write_new_json(&root.join("manifest.json"), &manifest)?;
        initialize_state(&root.join("state.sqlite3"))?;
        create_private_empty_file(&root.join("workspace/build.sqlite3"))?;
        Ok(Self { root, manifest })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        validate_project_path(root)?;
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(AppError::InvalidProject(
                "project root must be a non-symlink directory".into(),
            ));
        }
        let root = fs::canonicalize(root)?;
        let manifest: ProjectManifest =
            serde_json::from_reader(File::open(root.join("manifest.json"))?)?;
        manifest.validate()?;
        initialize_state(&root.join("state.sqlite3"))?;
        Ok(Self { root, manifest })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &ProjectManifest {
        &self.manifest
    }

    pub fn privacy_warning(&self) -> Option<&'static str> {
        self.manifest
            .sources
            .iter()
            .any(|source| is_probably_shared_location(&source.path))
            .then_some(
                "Project files contain plaintext source paths, plans, decisions, and AI records.",
            )
    }

    pub fn acquire_writer_lock(&self) -> Result<ProjectLock> {
        let path = self.root.join("project.lock");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    AppError::Locked(path.clone())
                } else {
                    AppError::Io(error)
                }
            })?;
        writeln!(file, "pid={}", std::process::id())?;
        file.sync_all()?;
        set_private_file(&path)?;
        Ok(ProjectLock { path: Some(path) })
    }

    pub fn add_source(&mut self, binding: SourceBinding) -> Result<()> {
        self.add_source_with_mode(binding, false)
    }

    /// Add an explicit absolute source without consulting the process cwd.
    pub fn add_source_explicit(&mut self, binding: SourceBinding) -> Result<()> {
        self.add_source_with_mode(binding, true)
    }

    fn add_source_with_mode(&mut self, binding: SourceBinding, explicit: bool) -> Result<()> {
        if self.manifest.sources.len() >= 10 {
            return Err(AppError::InvalidProject(
                "a project supports at most 10 sources".into(),
            ));
        }
        if self
            .manifest
            .sources
            .iter()
            .any(|source| source.source_id == binding.source_id)
        {
            return Err(AppError::InvalidProject(format!(
                "duplicate source ID `{}`",
                binding.source_id
            )));
        }
        let mut sources = self.manifest.sources.clone();
        sources.push(binding);
        if explicit {
            self.replace_sources_explicit(sources)
        } else {
            self.replace_sources(sources)
        }
    }

    pub fn rebind_source(
        &mut self,
        source_id: &SourceId,
        path: impl Into<PathBuf>,
        observed_snapshot_id: Option<String>,
    ) -> Result<()> {
        self.rebind_source_with_mode(source_id, path.into(), observed_snapshot_id, false)
    }

    /// Rebind an explicit absolute source without consulting the process cwd.
    pub fn rebind_source_explicit(
        &mut self,
        source_id: &SourceId,
        path: impl Into<PathBuf>,
        observed_snapshot_id: Option<String>,
    ) -> Result<()> {
        self.rebind_source_with_mode(source_id, path.into(), observed_snapshot_id, true)
    }

    fn rebind_source_with_mode(
        &mut self,
        source_id: &SourceId,
        path: PathBuf,
        observed_snapshot_id: Option<String>,
        explicit: bool,
    ) -> Result<()> {
        let mut sources = self.manifest.sources.clone();
        let source = sources
            .iter_mut()
            .find(|source| &source.source_id == source_id)
            .ok_or_else(|| AppError::InvalidProject(format!("unknown source `{source_id}`")))?;
        if source.path == path && source.snapshot_id.as_ref() == observed_snapshot_id.as_ref() {
            return Ok(());
        }
        source.path = path;
        source.snapshot_id = observed_snapshot_id;
        if explicit {
            self.replace_sources_explicit(sources)
        } else {
            self.replace_sources(sources)
        }
    }

    /// Atomically replace the complete active source set while retaining all
    /// historical runs and approvals in the append-only journal.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the public mutation boundary intentionally takes ownership of the replacement set"
    )]
    pub fn replace_sources(&mut self, sources: Vec<SourceBinding>) -> Result<()> {
        let _lock = self.acquire_writer_lock()?;
        let cwd = std::env::current_dir()?;
        let bootstrap = workspace_bootstrap::WorkspaceBootstrap::new(cwd)?;
        let sources = bootstrap.validate_source_selection(&sources)?;
        self.replace_sources_locked(sources)
    }

    /// Atomically replace explicit absolute sources without cwd discovery.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the public mutation boundary intentionally takes ownership of the replacement set"
    )]
    pub fn replace_sources_explicit(&mut self, sources: Vec<SourceBinding>) -> Result<()> {
        let _lock = self.acquire_writer_lock()?;
        let sources =
            workspace_bootstrap::WorkspaceBootstrap::validate_explicit_source_selection(&sources)?;
        self.replace_sources_locked(sources)
    }

    fn replace_sources_locked(&mut self, sources: Vec<SourceBinding>) -> Result<()> {
        if sources
            .iter()
            .any(|source| workspace_bootstrap::paths_overlap(&self.root, &source.path))
        {
            return Err(AppError::InvalidProject(
                "project storage must be outside every immutable source".into(),
            ));
        }
        if self.manifest.sources == sources {
            return Ok(());
        }
        let mut next_manifest = self.manifest.clone();
        next_manifest.sources = sources;
        next_manifest.validate()?;
        let source_bytes = serde_json::to_vec(&next_manifest.sources)?;
        let source_object = self.put_object(&source_bytes)?;
        let revision_hash = domain_digest(b"okc:source-set:v3\0", &source_bytes);
        let config_bytes = serde_json::to_vec(&(
            &next_manifest.policy_version,
            &next_manifest.language,
            &next_manifest.ai_routes,
        ))?;
        let config_hash = domain_digest(b"okc:integration-config:v3\0", &config_bytes);
        let connection = Connection::open(self.root.join("state.sqlite3"))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let transaction = connection.unchecked_transaction()?;
        let sequence: u64 = transaction.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM runs",
            [],
            |row| row.get(0),
        )?;
        let run_id = format!(
            "run_{}",
            domain_digest(
                b"okc:run:v3\0",
                format!("{revision_hash}:{config_hash}:{sequence}").as_bytes()
            )
        );
        transaction.execute(
            "INSERT INTO runs(run_id,input_hash,config_hash,sequence) VALUES (?1,?2,?3,?4)",
            params![run_id, revision_hash, config_hash, sequence],
        )?;
        transaction.execute(
            "INSERT INTO source_set_revisions_v4(revision_hash,object_id,run_id) VALUES (?1,?2,?3)",
            params![revision_hash, source_object, run_id],
        )?;
        transaction.execute(
            "INSERT INTO project_events(kind, detail) VALUES ('source_invalidation', 'active source set replaced')",
            [],
        )?;
        self.save_manifest_value(&next_manifest)?;
        transaction.commit()?;
        self.manifest = next_manifest;
        Ok(())
    }

    pub fn put_object(&self, bytes: &[u8]) -> Result<String> {
        let digest = hex_digest(bytes);
        let path = self.root.join("objects").join(&digest);
        if path.exists() {
            let mut existing = Vec::new();
            File::open(&path)?.read_to_end(&mut existing)?;
            if existing != bytes {
                return Err(AppError::InvalidProject(format!(
                    "object `{digest}` does not match its content address"
                )));
            }
            return Ok(digest);
        }
        let mut staged = tempfile::NamedTempFile::new_in(self.root.join("objects"))?;
        staged.write_all(bytes)?;
        staged.as_file_mut().sync_all()?;
        match staged.persist_noclobber(&path) {
            Ok(_) => {
                set_private_file(&path)?;
                Ok(digest)
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let mut existing = Vec::new();
                File::open(&path)?.read_to_end(&mut existing)?;
                if existing == bytes {
                    Ok(digest)
                } else {
                    Err(AppError::InvalidProject(
                        "content-addressed object collision".into(),
                    ))
                }
            }
            Err(error) => Err(AppError::Io(error.error)),
        }
    }

    fn save_manifest(&self) -> Result<()> {
        self.save_manifest_value(&self.manifest)
    }

    fn save_manifest_value(&self, manifest: &ProjectManifest) -> Result<()> {
        let destination = self.root.join("manifest.json");
        let mut staged = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer_pretty(&mut staged, manifest)?;
        staged.write_all(b"\n")?;
        staged.as_file_mut().sync_all()?;
        staged.persist(&destination).map_err(|error| error.error)?;
        set_private_file(&destination)?;
        Ok(())
    }

    fn invalidate_downstream(&self, reason: &str) -> Result<()> {
        let connection = Connection::open(self.root.join("state.sqlite3"))?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO project_events(kind, detail) VALUES ('source_invalidation', ?1)",
            params![reason],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn domain_digest(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug)]
pub struct ProjectLock {
    path: Option<PathBuf>,
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Inspect,
    Plan,
    Augment,
    Integrate,
    Compile,
    Pack,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Queued,
    Reading,
    Processing,
    Staging,
    Publishing,
    Verifying,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub operation: OperationKind,
    pub phase: OperationPhase,
    pub completed: u64,
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_item: Option<String>,
}

pub trait ProgressObserver: Send + Sync {
    fn observe(&self, event: &ProgressEvent);
}

#[derive(Clone)]
pub struct OperationControl {
    pub cancellation: CancellationToken,
    pub observer: Arc<dyn ProgressObserver>,
}

impl OperationControl {
    pub fn quiet() -> Self {
        Self {
            cancellation: CancellationToken::default(),
            observer: Arc::new(NoopProgressObserver),
        }
    }
}

#[derive(Debug)]
struct NoopProgressObserver;

impl ProgressObserver for NoopProgressObserver {
    fn observe(&self, _event: &ProgressEvent) {}
}

fn validate_project_path(path: &Path) -> Result<()> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("okc-project"))
    {
        return Err(AppError::InvalidProject(
            "project directory must end in .okc-project".into(),
        ));
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "one auditable SQLite schema definition"
)]
fn initialize_state(path: &Path) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > APPLICATION_STATE_SCHEMA_VERSION {
        return Err(AppError::InvalidProject(format!(
            "state schema {version} is newer than supported schema {APPLICATION_STATE_SCHEMA_VERSION}"
        )));
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS stage_state (\
             stage TEXT PRIMARY KEY,\
             object_id TEXT,\
             invalidation_reason TEXT\
         );\
         CREATE TABLE IF NOT EXISTS decisions (\
             conflict_id TEXT PRIMARY KEY,\
             object_id TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS approvals (\
             proposal_id TEXT PRIMARY KEY,\
             object_id TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS project_events (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             kind TEXT NOT NULL,\
             detail TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS runs (\
             run_id TEXT PRIMARY KEY,\
             input_hash TEXT NOT NULL,\
             config_hash TEXT NOT NULL,\
             sequence INTEGER NOT NULL UNIQUE\
         );\
         CREATE TABLE IF NOT EXISTS task_definitions (\
             task_id TEXT PRIMARY KEY,\
             run_id TEXT NOT NULL REFERENCES runs(run_id),\
             stage TEXT NOT NULL,\
             cache_key TEXT NOT NULL,\
             request_object TEXT NOT NULL,\
             UNIQUE(run_id, cache_key)\
         );\
         CREATE TABLE IF NOT EXISTS task_events (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             task_id TEXT NOT NULL REFERENCES task_definitions(task_id),\
             status TEXT NOT NULL,\
             response_object TEXT,\
             error_kind TEXT\
         );\
         CREATE TABLE IF NOT EXISTS exchanges_v3 (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             run_id TEXT NOT NULL REFERENCES runs(run_id),\
             task_id TEXT NOT NULL REFERENCES task_definitions(task_id),\
             recording_object TEXT NOT NULL,\
             recording_hash TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS cluster_revisions_v3 (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             run_id TEXT NOT NULL REFERENCES runs(run_id),\
             cluster_id TEXT NOT NULL,\
             revision_hash TEXT NOT NULL,\
             object_id TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS approvals_v3 (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             run_id TEXT NOT NULL REFERENCES runs(run_id),\
             approval_kind TEXT NOT NULL,\
             target_id TEXT NOT NULL,\
             target_hash TEXT NOT NULL,\
             object_id TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS source_set_revisions_v4 (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             revision_hash TEXT NOT NULL,\
             object_id TEXT NOT NULL,\
             run_id TEXT NOT NULL REFERENCES runs(run_id)\
         );\
         CREATE TABLE IF NOT EXISTS integration_plans_v4 (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             run_id TEXT NOT NULL REFERENCES runs(run_id),\
             plan_id TEXT NOT NULL,\
             plan_hash TEXT NOT NULL,\
             object_id TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS verified_outputs_v4 (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             run_id TEXT NOT NULL REFERENCES runs(run_id),\
             plan_id TEXT NOT NULL,\
             output_path TEXT NOT NULL,\
             manifest_object TEXT NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS cluster_feedback_v4 (\
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,\
             run_id TEXT NOT NULL REFERENCES runs(run_id),\
             cluster_id TEXT NOT NULL,\
             revision INTEGER NOT NULL,\
             feedback_hash TEXT NOT NULL,\
             feedback_object TEXT NOT NULL,\
             previous_proposal_hash TEXT NOT NULL,\
             previous_critic_hash TEXT NOT NULL\
         );\
         PRAGMA user_version = 4;",
    )?;
    set_private_file(path)?;
    Ok(())
}

fn write_new_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    set_private_file(path)?;
    Ok(())
}

fn create_private_empty_file(path: &Path) -> Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.sync_all()?;
    set_private_file(path)
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"okc:project-object:v3\0");
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn is_probably_shared_location(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with("//")
        || text.starts_with("\\\\")
        || text.starts_with("/Volumes/")
        || text.contains("OneDrive")
        || text.contains("Dropbox")
}

#[cfg(unix)]
pub(crate) fn set_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_private_file(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_layout_lock_objects_and_rebind_are_safe() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let root = temporary.path().join("Knowledge.okc-project");
        let mut project = ProjectStore::create(&root, "Knowledge", "curator", "policy-v2")
            .expect("create project");
        assert!(root.join("manifest.json").is_file());
        assert!(root.join("state.sqlite3").is_file());
        assert!(root.join("workspace/build.sqlite3").is_file());
        let lock = project.acquire_writer_lock().expect("writer lock");
        assert!(matches!(
            project.acquire_writer_lock(),
            Err(AppError::Locked(_))
        ));
        drop(lock);
        let object_id = project.put_object(b"sealed").expect("store object");
        assert_eq!(
            project.put_object(b"sealed").expect("reuse object"),
            object_id
        );

        let source_id = SourceId::new("alpha").expect("source ID");
        let first_source = temporary.path().join("alpha");
        let moved_source = temporary.path().join("moved");
        fs::create_dir(&first_source).expect("first source");
        fs::create_dir(&moved_source).expect("moved source");
        fs::write(first_source.join("note.md"), b"first").expect("first note");
        fs::write(moved_source.join("note.md"), b"moved").expect("moved note");
        let first_metadata = fs::metadata(first_source.join("note.md")).expect("metadata");
        project
            .add_source(SourceBinding {
                source_id: source_id.clone(),
                owner_display_name: Some("Alice".into()),
                path: first_source.clone(),
                snapshot_id: Some("snap_old".into()),
            })
            .expect("add source");
        project
            .rebind_source(&source_id, &moved_source, Some("snap_new".into()))
            .expect("rebind source");
        assert_eq!(
            fs::read(first_source.join("note.md")).expect("unchanged source"),
            b"first"
        );
        let after_metadata = fs::metadata(first_source.join("note.md")).expect("metadata");
        assert_eq!(first_metadata.len(), after_metadata.len());
        assert_eq!(
            first_metadata.modified().expect("modified time"),
            after_metadata.modified().expect("modified time")
        );
        let reopened = ProjectStore::open(&root).expect("reopen project");
        assert_eq!(
            reopened.manifest().sources[0].path,
            fs::canonicalize(moved_source).expect("canonical moved source")
        );
    }

    #[cfg(feature = "updater")]
    #[test]
    fn update_targets_are_explicit_and_semver_canonical() {
        assert_eq!(
            UpdateTarget::parse("stable").expect("stable"),
            UpdateTarget::Stable
        );
        assert_eq!(
            UpdateTarget::parse("latest").expect("latest"),
            UpdateTarget::Latest
        );
        assert_eq!(
            UpdateTarget::parse("1.2.3").expect("exact"),
            UpdateTarget::Exact("1.2.3".into())
        );
        assert!(UpdateTarget::parse("v1.2.3").is_err());
        assert!(UpdateTarget::parse("1.2").is_err());
    }
}
