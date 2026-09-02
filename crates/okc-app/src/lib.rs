//! Long-lived OKC project storage and application orchestration primitives.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use okc_core::{CancellationToken, SourceId};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub const PROJECT_SCHEMA_VERSION: u32 = 2;

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
    #[error("invalid OKC project: {0}")]
    InvalidProject(String),
    #[error("project is already locked: {0}")]
    Locked(PathBuf),
    #[error("update unavailable: {0}")]
    Update(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateTarget {
    Stable,
    Latest,
    Exact(String),
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    pub update_needed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledUpdate {
    pub old_version: Option<String>,
    pub new_version: String,
    pub release_tag: String,
    pub install_prefix: String,
}

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
pub fn check_for_update(target: &UpdateTarget) -> Result<UpdateStatus> {
    let mut updater = configured_updater(target)?;
    let update_needed = updater
        .is_update_needed_sync()
        .map_err(|error| AppError::Update(error.to_string()))?;
    Ok(UpdateStatus { update_needed })
}

/// Install only into the prefix named by the matching cargo-dist receipt.
/// The caller owns the interactive confirmation boundary.
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
        fs::create_dir(root)?;
        set_private_directory(root)?;
        fs::create_dir(root.join("objects"))?;
        fs::create_dir(root.join("workspace"))?;
        set_private_directory(&root.join("objects"))?;
        set_private_directory(&root.join("workspace"))?;
        let manifest = ProjectManifest {
            format_family: "okc".into(),
            schema_version: PROJECT_SCHEMA_VERSION,
            product_version: env!("CARGO_PKG_VERSION").into(),
            name: name.into(),
            curator_id: curator_id.into(),
            policy_version: policy_version.into(),
            sources: Vec::new(),
        };
        manifest.validate()?;
        write_new_json(&root.join("manifest.json"), &manifest)?;
        initialize_state(&root.join("state.sqlite3"))?;
        create_private_empty_file(&root.join("workspace/build.sqlite3"))?;
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
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
        let manifest: ProjectManifest =
            serde_json::from_reader(File::open(root.join("manifest.json"))?)?;
        manifest.validate()?;
        initialize_state(&root.join("state.sqlite3"))?;
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
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
        self.manifest.sources.push(binding);
        self.manifest.sources.sort_by(|left, right| {
            left.source_id
                .cmp(&right.source_id)
                .then_with(|| left.path.as_os_str().cmp(right.path.as_os_str()))
        });
        self.manifest.validate()?;
        self.save_manifest()?;
        self.invalidate_downstream("source added")
    }

    pub fn rebind_source(
        &mut self,
        source_id: &SourceId,
        path: impl Into<PathBuf>,
        observed_snapshot_id: Option<String>,
    ) -> Result<()> {
        let path = path.into();
        let source = self
            .manifest
            .sources
            .iter_mut()
            .find(|source| &source.source_id == source_id)
            .ok_or_else(|| AppError::InvalidProject(format!("unknown source `{source_id}`")))?;
        let changed =
            source.path != path || source.snapshot_id.as_ref() != observed_snapshot_id.as_ref();
        source.path = path;
        source.snapshot_id = observed_snapshot_id;
        self.save_manifest()?;
        if changed {
            self.invalidate_downstream("source rebound or snapshot changed")?;
        }
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
        let destination = self.root.join("manifest.json");
        let mut staged = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer_pretty(&mut staged, &self.manifest)?;
        staged.write_all(b"\n")?;
        staged.as_file_mut().sync_all()?;
        staged.persist(&destination).map_err(|error| error.error)?;
        set_private_file(&destination)?;
        Ok(())
    }

    fn invalidate_downstream(&self, reason: &str) -> Result<()> {
        let connection = Connection::open(self.root.join("state.sqlite3"))?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute("DELETE FROM stage_state WHERE stage != 'sources'", [])?;
        transaction.execute("DELETE FROM decisions", [])?;
        transaction.execute("DELETE FROM approvals", [])?;
        transaction.execute(
            "INSERT INTO stage_state(stage, object_id, invalidation_reason)\
             VALUES ('sources', NULL, ?1)\
             ON CONFLICT(stage) DO UPDATE SET object_id=NULL, invalidation_reason=excluded.invalidation_reason",
            params![reason],
        )?;
        transaction.commit()?;
        Ok(())
    }
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
}

pub trait ProgressObserver: Send + Sync {
    fn observe(&self, event: &ProgressEvent);
}

#[derive(Clone)]
pub struct OperationControl {
    pub cancellation: CancellationToken,
    pub observer: Arc<dyn ProgressObserver>,
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

fn initialize_state(path: &Path) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > PROJECT_SCHEMA_VERSION {
        return Err(AppError::InvalidProject(format!(
            "state schema {version} is newer than supported schema {PROJECT_SCHEMA_VERSION}"
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
         PRAGMA user_version = 2;",
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
    hasher.update(b"okc:project-object:v2\0");
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
fn set_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<()> {
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
        project
            .add_source(SourceBinding {
                source_id: source_id.clone(),
                owner_display_name: Some("Alice".into()),
                path: PathBuf::from("/vault/alpha"),
                snapshot_id: Some("snap_old".into()),
            })
            .expect("add source");
        project
            .rebind_source(&source_id, "/vault/moved", Some("snap_new".into()))
            .expect("rebind source");
        let reopened = ProjectStore::open(&root).expect("reopen project");
        assert_eq!(
            reopened.manifest().sources[0].path,
            PathBuf::from("/vault/moved")
        );
    }

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
