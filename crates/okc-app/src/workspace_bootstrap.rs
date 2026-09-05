//! Deterministic cwd/project/Vault discovery for the CLI and TUI.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{AppError, Result, SourceBinding};

const DISCOVERY_ENTRY_LIMIT: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectCandidateKind {
    Explicit,
    HiddenDefault,
    SafeSibling,
    DirectChild,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCandidate {
    pub path: PathBuf,
    pub kind: ProjectCandidateKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultCandidateKind {
    Directory,
    Zip,
    TarZst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultCandidate {
    pub path: PathBuf,
    pub kind: VaultCandidateKind,
    pub suggested_source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDiscovery {
    pub cwd: PathBuf,
    pub projects: Vec<ProjectCandidate>,
    pub vaults: Vec<VaultCandidate>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceBootstrap {
    cwd: PathBuf,
}

impl WorkspaceBootstrap {
    pub fn from_current_dir() -> Result<Self> {
        Self::new(std::env::current_dir()?)
    }

    pub fn new(cwd: impl AsRef<Path>) -> Result<Self> {
        let cwd = canonical_non_symlink(cwd.as_ref(), true)?;
        Ok(Self { cwd })
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn discover(&self, explicit_project: Option<&Path>) -> Result<WorkspaceDiscovery> {
        Ok(WorkspaceDiscovery {
            cwd: self.cwd.clone(),
            projects: self.discover_projects(explicit_project)?,
            vaults: self.discover_vaults()?,
        })
    }

    pub fn discover_projects(
        &self,
        explicit_project: Option<&Path>,
    ) -> Result<Vec<ProjectCandidate>> {
        if let Some(explicit) = explicit_project {
            return Ok(vec![ProjectCandidate {
                path: absolute_existing(&self.cwd, explicit)?,
                kind: ProjectCandidateKind::Explicit,
            }]);
        }

        let mut projects = BTreeMap::<PathBuf, ProjectCandidateKind>::new();
        let hidden = self.cwd.join(".okc-work/workspace.okc-project");
        add_project_if_safe(&mut projects, hidden, ProjectCandidateKind::HiddenDefault)?;

        let safe_sibling = Self::safe_sibling_project_path(&self.cwd);
        add_project_if_safe(
            &mut projects,
            safe_sibling,
            ProjectCandidateKind::SafeSibling,
        )?;

        for entry in sorted_entries(&self.cwd)? {
            let path = entry.path();
            if is_project_path(&path) {
                add_project_if_safe(&mut projects, path, ProjectCandidateKind::DirectChild)?;
            }
        }

        Ok(projects
            .into_iter()
            .map(|(path, kind)| ProjectCandidate { path, kind })
            .collect())
    }

    pub fn discover_vaults(&self) -> Result<Vec<VaultCandidate>> {
        let mut candidates = Vec::<(PathBuf, VaultCandidateKind)>::new();
        if is_vault_directory(&self.cwd)? && !is_managed_directory(&self.cwd) {
            candidates.push((self.cwd.clone(), VaultCandidateKind::Directory));
        }
        for entry in sorted_entries(&self.cwd)? {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() || is_managed_directory(&path) || is_project_path(&path) {
                continue;
            }
            if file_type.is_dir() {
                if is_vault_directory(&path)? {
                    candidates.push((path, VaultCandidateKind::Directory));
                }
            } else if file_type.is_file() {
                if is_zip(&path) {
                    candidates.push((path, VaultCandidateKind::Zip));
                } else if is_tar_zst(&path) {
                    candidates.push((path, VaultCandidateKind::TarZst));
                }
            }
        }
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        candidates.dedup_by(|left, right| left.0 == right.0);
        Ok(assign_source_ids(candidates))
    }

    pub fn manual_vault(&self, path: impl AsRef<Path>) -> Result<VaultCandidate> {
        let path = absolute_existing(&self.cwd, path.as_ref())?;
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || is_managed_directory(&path) {
            return Err(AppError::InvalidProject(
                "a source must not be a symlink or managed OKC directory".into(),
            ));
        }
        let kind = if metadata.is_dir() && is_vault_directory(&path)? {
            VaultCandidateKind::Directory
        } else if metadata.is_file() && is_zip(&path) {
            VaultCandidateKind::Zip
        } else if metadata.is_file() && is_tar_zst(&path) {
            VaultCandidateKind::TarZst
        } else {
            return Err(AppError::InvalidProject(
                "manual source must be a Vault directory, ZIP, tar.zst, or tzst".into(),
            ));
        };
        Ok(VaultCandidate {
            suggested_source_id: proposed_source_id(&path),
            path,
            kind,
        })
    }

    pub fn validate_source_selection(
        &self,
        bindings: &[SourceBinding],
    ) -> Result<Vec<SourceBinding>> {
        if bindings.is_empty() || bindings.len() > 10 {
            return Err(AppError::InvalidProject(
                "select between one and ten Vault sources".into(),
            ));
        }
        let mut normalized = Vec::with_capacity(bindings.len());
        let mut ids = BTreeSet::new();
        for binding in bindings {
            if !ids.insert(binding.source_id.clone()) {
                return Err(AppError::InvalidProject(format!(
                    "duplicate source ID `{}`",
                    binding.source_id
                )));
            }
            let path = absolute_existing(&self.cwd, &binding.path)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || is_managed_directory(&path) {
                return Err(AppError::InvalidProject(format!(
                    "unsafe source `{}`",
                    path.display()
                )));
            }
            let supported = (metadata.is_dir() && is_vault_directory(&path)?)
                || (metadata.is_file() && (is_zip(&path) || is_tar_zst(&path)));
            if !supported {
                return Err(AppError::InvalidProject(format!(
                    "unsupported or empty Vault source `{}`",
                    path.display()
                )));
            }
            normalized.push(SourceBinding {
                source_id: binding.source_id.clone(),
                owner_display_name: binding.owner_display_name.clone(),
                path,
                snapshot_id: binding.snapshot_id.clone(),
            });
        }
        normalized.sort_by(|left, right| {
            left.source_id
                .cmp(&right.source_id)
                .then_with(|| left.path.cmp(&right.path))
        });
        for (index, left) in normalized.iter().enumerate() {
            if !left.path.is_dir() {
                continue;
            }
            for right in normalized.iter().skip(index + 1) {
                if right.path.starts_with(&left.path)
                    || (right.path.is_dir() && left.path.starts_with(&right.path))
                {
                    return Err(AppError::InvalidProject(
                        "a source selection cannot contain both an ancestor and descendant".into(),
                    ));
                }
            }
        }
        Ok(normalized)
    }

    pub fn suggested_project_path(&self, bindings: &[SourceBinding]) -> Result<PathBuf> {
        let sources = self.validate_source_selection(bindings)?;
        if let Some(source) = sources
            .iter()
            .filter(|source| source.path.is_dir() && self.cwd.starts_with(&source.path))
            .max_by_key(|source| source.path.components().count())
        {
            let parent = source.path.parent().ok_or_else(|| {
                AppError::InvalidProject("a filesystem root cannot be a Vault source".into())
            })?;
            let name = source
                .path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("vault");
            return Ok(parent.join(".okc-work").join(format!(
                "{}-{}.okc-project",
                ascii_component(name),
                short_path_hash(&source.path)
            )));
        }
        Ok(self.cwd.join(".okc-work/workspace.okc-project"))
    }

    pub fn suggested_output_path(&self, sources: &[SourceBinding]) -> Result<PathBuf> {
        let sources = self.validate_source_selection(sources)?;
        let base = sources
            .iter()
            .filter(|source| source.path.is_dir() && self.cwd.starts_with(&source.path))
            .max_by_key(|source| source.path.components().count())
            .and_then(|source| source.path.parent())
            .unwrap_or(&self.cwd);
        for index in 1_u32..=10_000 {
            let name = if index == 1 {
                "IntegratedVault".to_owned()
            } else {
                format!("IntegratedVault-{index}")
            };
            let candidate = base.join(name);
            if fs::symlink_metadata(&candidate).is_err()
                && sources
                    .iter()
                    .all(|source| !paths_overlap(&candidate, &source.path))
            {
                return Ok(candidate);
            }
        }
        Err(AppError::InvalidProject(
            "could not allocate an unused IntegratedVault destination".into(),
        ))
    }

    fn safe_sibling_project_path(source: &Path) -> PathBuf {
        let parent = source.parent().unwrap_or(source);
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("workspace");
        parent.join(".okc-work").join(format!(
            "{}-{}.okc-project",
            ascii_component(name),
            short_path_hash(source)
        ))
    }
}

fn assign_source_ids(candidates: Vec<(PathBuf, VaultCandidateKind)>) -> Vec<VaultCandidate> {
    let mut base_counts = BTreeMap::<String, usize>::new();
    for (path, _) in &candidates {
        *base_counts.entry(proposed_source_id(path)).or_default() += 1;
    }
    candidates
        .into_iter()
        .map(|(path, kind)| {
            let base = proposed_source_id(&path);
            let suggested_source_id = if base_counts[&base] > 1 {
                format!("{base}-{}", short_path_hash(&path))
            } else {
                base
            };
            VaultCandidate {
                path,
                kind,
                suggested_source_id,
            }
        })
        .collect()
}

fn proposed_source_id(path: &Path) -> String {
    let name = if is_tar_zst(path) {
        path.file_stem()
            .and_then(|value| Path::new(value).file_stem())
            .and_then(|value| value.to_str())
    } else {
        path.file_stem().and_then(|value| value.to_str())
    }
    .or_else(|| path.file_name().and_then(|value| value.to_str()))
    .unwrap_or("source");
    let ascii = ascii_component(name);
    if ascii == "source" && !name.eq_ignore_ascii_case("source") {
        format!("source-{}", short_path_hash(path))
    } else {
        ascii
    }
}

fn ascii_component(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.') {
            output.push(char::from(byte).to_ascii_lowercase());
            separator = false;
        } else if byte.is_ascii() && !separator && !output.is_empty() {
            output.push('-');
            separator = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        "source".into()
    } else {
        output.truncate(96);
        output
    }
}

fn short_path_hash(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"okc:workspace-path:v3\0");
    hasher.update(path.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())[..10].to_owned()
}

fn add_project_if_safe(
    projects: &mut BTreeMap<PathBuf, ProjectCandidateKind>,
    path: PathBuf,
    kind: ProjectCandidateKind,
) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return Ok(());
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        let canonical = fs::canonicalize(path)?;
        projects.entry(canonical).or_insert(kind);
    }
    Ok(())
}

fn is_project_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("okc-project"))
}

fn is_managed_directory(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    name.eq_ignore_ascii_case(".okc-work")
        || name.starts_with("IntegratedVault")
        || path.join(".okc/manifest.json").is_file()
}

fn is_vault_directory(path: &Path) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_managed_directory(path) {
        return Ok(false);
    }
    let obsidian = path.join(".obsidian");
    if fs::symlink_metadata(obsidian)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
    {
        return Ok(true);
    }
    contains_markdown(path)
}

fn contains_markdown(root: &Path) -> Result<bool> {
    let mut pending = vec![root.to_path_buf()];
    let mut seen = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in sorted_entries(&directory)? {
            seen += 1;
            if seen > DISCOVERY_ENTRY_LIMIT {
                return Err(AppError::InvalidProject(
                    "Vault discovery exceeded its entry limit".into(),
                ));
            }
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() || is_managed_directory(&path) || is_project_path(&path) {
                continue;
            }
            if file_type.is_file()
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("md"))
            {
                return Ok(true);
            }
            if file_type.is_dir() && path.file_name().is_none_or(|name| name != ".obsidian") {
                pending.push(path);
            }
        }
    }
    Ok(false)
}

fn sorted_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
}

fn is_tar_zst(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("tzst"))
        || (path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("zst"))
            && path
                .file_stem()
                .and_then(|value| Path::new(value).extension())
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("tar")))
}

fn absolute_existing(cwd: &Path, path: &Path) -> Result<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    canonical_non_symlink(&joined, false)
}

fn canonical_non_symlink(path: &Path, require_directory: bool) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || (require_directory && !metadata.is_dir()) {
        return Err(AppError::InvalidProject(format!(
            "path must be a non-symlink{}: {}",
            if require_directory {
                " directory"
            } else {
                " entry"
            },
            path.display()
        )));
    }
    Ok(fs::canonicalize(path)?)
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use okc_core::SourceId;

    #[test]
    fn explicit_project_wins_and_discovery_is_bounded_to_cwd_children() {
        let temporary = tempfile::tempdir().expect("temporary");
        let cwd = temporary.path().join("work");
        fs::create_dir(&cwd).expect("cwd");
        fs::create_dir(cwd.join("A.okc-project")).expect("project");
        fs::create_dir(cwd.join("B.okc-project")).expect("project");
        fs::write(cwd.join("note.md"), b"# root\n").expect("note");
        let child = cwd.join("child");
        fs::create_dir(&child).expect("child");
        fs::write(child.join("nested.md"), b"# child\n").expect("note");
        let nested = child.join("nested");
        fs::create_dir(&nested).expect("nested");
        fs::write(nested.join("deep.md"), b"# deep\n").expect("note");
        fs::write(cwd.join("archive.tar.zst"), b"fixture").expect("archive");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&child, cwd.join("linked")).expect("symlink");

        let bootstrap = WorkspaceBootstrap::new(&cwd).expect("bootstrap");
        let projects = bootstrap.discover_projects(None).expect("projects");
        assert_eq!(projects.len(), 2);
        let explicit = bootstrap
            .discover_projects(Some(Path::new("B.okc-project")))
            .expect("explicit");
        assert_eq!(explicit.len(), 1);
        assert_eq!(explicit[0].kind, ProjectCandidateKind::Explicit);
        let vaults = bootstrap.discover_vaults().expect("vaults");
        assert!(vaults.iter().any(|vault| vault.path == bootstrap.cwd));
        assert!(
            vaults
                .iter()
                .any(|vault| vault.path == fs::canonicalize(&child).expect("canonical child"))
        );
        assert!(
            vaults
                .iter()
                .any(|vault| vault.kind == VaultCandidateKind::TarZst)
        );
        assert!(!vaults.iter().any(|vault| vault.path.ends_with("linked")));
        assert!(!vaults.iter().any(|vault| vault.path == nested));
    }

    #[test]
    fn selection_rejects_nested_sources_and_allocates_external_project() {
        let temporary = tempfile::tempdir().expect("temporary");
        let cwd = temporary.path().join("vault");
        let nested = cwd.join("nested");
        fs::create_dir_all(&nested).expect("vaults");
        fs::write(cwd.join("root.md"), b"root").expect("note");
        fs::write(nested.join("nested.md"), b"nested").expect("note");
        let bootstrap = WorkspaceBootstrap::new(&cwd).expect("bootstrap");
        let binding = |id: &str, path: PathBuf| SourceBinding {
            source_id: SourceId::new(id).expect("id"),
            owner_display_name: None,
            path,
            snapshot_id: None,
        };
        assert!(
            bootstrap
                .validate_source_selection(&[
                    binding("root", cwd.clone()),
                    binding("nested", nested)
                ])
                .is_err()
        );
        let selected = [binding("root", cwd.clone())];
        let project = bootstrap
            .suggested_project_path(&selected)
            .expect("project path");
        assert!(!project.starts_with(&cwd));
        let output = bootstrap
            .suggested_output_path(&selected)
            .expect("output path");
        assert!(!paths_overlap(&output, &cwd));
    }

    #[test]
    fn selection_limit_and_managed_directories_fail_closed() {
        let temporary = tempfile::tempdir().expect("temporary");
        let cwd = temporary.path().join("workspace");
        fs::create_dir(&cwd).expect("workspace");
        fs::create_dir(cwd.join(".okc-work")).expect("managed");
        fs::write(cwd.join(".okc-work/hidden.md"), b"hidden").expect("hidden note");
        fs::create_dir(cwd.join("IntegratedVault")).expect("output");
        fs::write(cwd.join("IntegratedVault/out.md"), b"output").expect("output note");
        let bootstrap = WorkspaceBootstrap::new(&cwd).expect("bootstrap");
        let vaults = bootstrap.discover_vaults().expect("vaults");
        assert!(!vaults.iter().any(|item| item.path.ends_with(".okc-work")));
        assert!(
            !vaults
                .iter()
                .any(|item| item.path.ends_with("IntegratedVault"))
        );

        let mut bindings = Vec::new();
        for index in 0..11 {
            let path = cwd.join(format!("vault-{index}"));
            fs::create_dir(&path).expect("vault");
            fs::write(path.join("note.md"), b"note").expect("note");
            bindings.push(SourceBinding {
                source_id: SourceId::new(format!("vault-{index}")).expect("source ID"),
                owner_display_name: None,
                path,
                snapshot_id: None,
            });
        }
        assert!(bootstrap.validate_source_selection(&bindings).is_err());
    }

    #[test]
    fn zero_one_multiple_projects_and_all_direct_archive_kinds_are_deterministic() {
        let temporary = tempfile::tempdir().expect("temporary");
        let cwd = temporary.path().join("workspace");
        fs::create_dir(&cwd).expect("workspace");
        let bootstrap = WorkspaceBootstrap::new(&cwd).expect("bootstrap");
        assert!(bootstrap.discover_projects(None).expect("zero").is_empty());

        let hidden = cwd.join(".okc-work/workspace.okc-project");
        fs::create_dir_all(&hidden).expect("hidden project");
        let one = bootstrap.discover_projects(None).expect("one");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].kind, ProjectCandidateKind::HiddenDefault);

        let sibling = WorkspaceBootstrap::safe_sibling_project_path(bootstrap.cwd());
        fs::create_dir_all(&sibling).expect("sibling project");
        fs::create_dir(cwd.join("direct.okc-project")).expect("direct project");
        let multiple = bootstrap.discover_projects(None).expect("multiple");
        assert_eq!(multiple.len(), 3);
        assert!(
            multiple
                .iter()
                .any(|item| item.kind == ProjectCandidateKind::SafeSibling)
        );
        assert!(
            multiple
                .iter()
                .any(|item| item.kind == ProjectCandidateKind::DirectChild)
        );

        fs::write(cwd.join("one.zip"), b"fixture").expect("zip");
        fs::write(cwd.join("two.tzst"), b"fixture").expect("tzst");
        for name in ["지식", "기록"] {
            let path = cwd.join(name);
            fs::create_dir(&path).expect("non-ASCII Vault");
            fs::write(path.join("note.md"), b"note").expect("note");
        }
        let vaults = bootstrap.discover_vaults().expect("vaults");
        assert!(
            vaults
                .iter()
                .any(|item| item.kind == VaultCandidateKind::Zip)
        );
        assert!(
            vaults
                .iter()
                .any(|item| item.kind == VaultCandidateKind::TarZst)
        );
        let hashed_ids = vaults
            .iter()
            .filter(|item| item.path.ends_with("지식") || item.path.ends_with("기록"))
            .map(|item| item.suggested_source_id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(hashed_ids.len(), 2);
        assert!(hashed_ids.iter().all(|id| id.starts_with("source-")));
    }
}
