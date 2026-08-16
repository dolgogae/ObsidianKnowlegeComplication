use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::config::CompilerPolicy;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::error::{Result, VaultcError};
use crate::identity::{ContentHash, SnapshotId, SourceFileId};
use crate::ir::{CanonicalWorkspace, FileKind, SourceFile};
use crate::plan::Inspection;
use crate::source::{SourceId, SourceSpec};

pub(crate) const SOURCE_POLICY_ID: &str = "vaultc-source-v1";
const INITIAL_READ_CAPACITY: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSnapshot {
    pub snapshot_id: SnapshotId,
    pub source_id: SourceId,
    pub source: SourceSpec,
    pub policy_id: String,
    pub files: Vec<SourceFile>,
}

#[derive(Debug)]
pub(crate) struct RawEntry {
    pub logical_path: String,
    pub kind: FileKind,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
struct PendingFile {
    logical_path: String,
    kind: FileKind,
    byte_len: u64,
    content_hash: ContentHash,
    file_id: SourceFileId,
}

pub fn inspect_sources(
    sources: impl IntoIterator<Item = SourceSpec>,
    policy: &CompilerPolicy,
    workspace_path: Option<&Path>,
) -> Result<Inspection> {
    policy.validate()?;
    let sources: Vec<_> = sources.into_iter().collect();
    if sources.is_empty() {
        return Err(VaultcError::InvalidConfig(
            "at least one source is required".into(),
        ));
    }
    if sources.len() > policy.limits.max_sources as usize {
        return Err(VaultcError::ResourceLimit(format!(
            "{} sources exceeds configured maximum {}",
            sources.len(),
            policy.limits.max_sources
        )));
    }
    let unique: BTreeSet<_> = sources.iter().map(SourceSpec::source_id).collect();
    if unique.len() != sources.len() {
        return Err(VaultcError::InvalidConfig(
            "source IDs must be unique within an inspection".into(),
        ));
    }

    let mut snapshots = Vec::with_capacity(sources.len());
    let mut canonical = CanonicalWorkspace::default();
    let mut diagnostics = Vec::new();
    let mut total_files = 0_u64;
    let mut total_bytes = 0_u64;

    for source in sources {
        let entries = collect_entries(&source, policy, &mut diagnostics)?;
        total_files = total_files.saturating_add(entries.len() as u64);
        total_bytes = total_bytes.saturating_add(
            entries
                .iter()
                .map(|entry| entry.bytes.len() as u64)
                .sum::<u64>(),
        );
        if total_files > policy.limits.max_files {
            return Err(VaultcError::ResourceLimit(format!(
                "file count exceeds {}",
                policy.limits.max_files
            )));
        }
        if total_bytes > policy.limits.max_total_bytes {
            return Err(VaultcError::ResourceLimit(format!(
                "total bytes exceed {}",
                policy.limits.max_total_bytes
            )));
        }

        let (snapshot, parsed) = seal_and_parse(source, entries, policy, &mut diagnostics)?;
        snapshots.push(snapshot);
        canonical.documents.extend(parsed.documents);
        canonical.canvases.extend(parsed.canvases);
        for (id, asset) in parsed.assets {
            match canonical.assets.entry(id) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(asset);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().merge_sources(asset.sources);
                }
            }
        }
        canonical.bases.extend(parsed.bases);
    }

    snapshots.sort_by(|left, right| left.source_id.cmp(&right.source_id));
    diagnostics.sort_by(|left, right| {
        (&left.logical_path, left.code, &left.message).cmp(&(
            &right.logical_path,
            right.code,
            &right.message,
        ))
    });
    let inspection = Inspection::new(policy.semantic_hash()?, snapshots, canonical, diagnostics)?;

    #[cfg(feature = "sqlite")]
    if let Some(path) = workspace_path {
        crate::workspace::persist_inspection(path, &inspection)?;
    }
    #[cfg(not(feature = "sqlite"))]
    let _ = workspace_path;

    Ok(inspection)
}

fn seal_and_parse(
    source: SourceSpec,
    mut entries: Vec<RawEntry>,
    policy: &CompilerPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(VaultSnapshot, CanonicalWorkspace)> {
    entries.sort_by(|left, right| {
        left.logical_path
            .as_bytes()
            .cmp(right.logical_path.as_bytes())
    });
    let mut pending = Vec::with_capacity(entries.len());
    for entry in &entries {
        let content_hash = ContentHash::from_bytes(&entry.bytes);
        let file_id =
            SourceFileId::from_file(&entry.logical_path, entry.kind.media_family(), content_hash);
        pending.push(PendingFile {
            logical_path: entry.logical_path.clone(),
            kind: entry.kind.clone(),
            byte_len: entry.bytes.len() as u64,
            content_hash,
            file_id,
        });
    }

    let snapshot_id = SnapshotId::from_manifest(
        source.source_id().as_str(),
        SOURCE_POLICY_ID,
        pending
            .iter()
            .map(|file| (file.logical_path.as_str(), file.file_id)),
    );

    let files: Vec<_> = pending
        .into_iter()
        .map(|file| SourceFile {
            source_id: source.source_id().clone(),
            snapshot_id,
            file_id: file.file_id,
            logical_path: file.logical_path,
            kind: file.kind,
            byte_len: file.byte_len,
            content_hash: file.content_hash,
        })
        .collect();

    let mut canonical = CanonicalWorkspace::default();
    for (file, entry) in files.iter().zip(entries.iter()) {
        crate::parse::parse_file(
            file.clone(),
            &entry.bytes,
            policy,
            &mut canonical,
            diagnostics,
        )?;
    }

    Ok((
        VaultSnapshot {
            snapshot_id,
            source_id: source.source_id().clone(),
            source,
            policy_id: SOURCE_POLICY_ID.into(),
            files,
        },
        canonical,
    ))
}

pub(crate) fn collect_entries(
    source: &SourceSpec,
    policy: &CompilerPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    match source {
        SourceSpec::Directory { path, .. } => collect_directory(path, policy, diagnostics),
        SourceSpec::Zip { path, .. } => collect_zip(path, policy, diagnostics),
        SourceSpec::TarZst { path, .. } => collect_tar_zst(path, policy, diagnostics),
    }
}

pub(crate) fn read_source_entry(
    source: &SourceSpec,
    logical_path: &str,
    policy: &CompilerPolicy,
) -> Result<Vec<u8>> {
    let mut diagnostics = Vec::new();
    let entries = collect_entries(source, policy, &mut diagnostics)?;
    entries
        .into_iter()
        .find(|entry| entry.logical_path == logical_path)
        .map(|entry| entry.bytes)
        .ok_or_else(|| VaultcError::IdentityMismatch(logical_path.into()))
}

#[allow(
    clippy::too_many_lines,
    reason = "directory traversal keeps exclusion, containment, race, and limit checks adjacent"
)]
fn collect_directory(
    root: &Path,
    policy: &CompilerPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    let metadata = fs::metadata(root).map_err(|error| VaultcError::io(root, error))?;
    if !metadata.is_dir() {
        return Err(VaultcError::UnsupportedSource(root.to_path_buf()));
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| VaultcError::io(root, error))?;
    let excludes = build_excludes(policy)?;
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .follow_links(false)
        .git_ignore(false)
        .git_exclude(false);
    let pruned_directories = Arc::new(Mutex::new(Vec::new()));
    let filter_pruned = Arc::clone(&pruned_directories);
    let filter_excludes = excludes.clone();
    let filter_root = root.to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.path() == filter_root || !entry.file_type().is_some_and(|kind| kind.is_dir()) {
            return true;
        }
        let Some(relative) = entry.path().strip_prefix(&filter_root).ok() else {
            return true;
        };
        let Some(relative) = relative.to_str() else {
            return true;
        };
        let logical: String = relative
            .replace(std::path::MAIN_SEPARATOR, "/")
            .nfc()
            .collect();
        let should_prune = is_excluded(&logical, false, true, &filter_excludes);
        if should_prune && let Ok(mut paths) = filter_pruned.lock() {
            paths.push(logical);
        }
        !should_prune
    });
    let mut entries = Vec::new();
    let mut total_bytes = 0_u64;
    for result in builder.build() {
        let entry = result.map_err(|error| VaultcError::MalformedInput {
            path: root.display().to_string(),
            reason: error.to_string(),
        })?;
        if entry.path() == root {
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| VaultcError::UnsafePath {
                path: entry.path().display().to_string(),
                reason: "entry escaped source root".into(),
            })?;
        let logical = normalize_logical_path(relative, policy)?;
        if is_excluded(
            &logical,
            file_type.is_symlink(),
            file_type.is_dir(),
            &excludes,
        ) {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V1 policy")
                    .for_path(logical),
            );
            continue;
        }
        if file_type.is_dir() {
            continue;
        }
        if file_type.is_symlink() {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "symlink not followed")
                    .for_path(logical),
            );
            continue;
        }
        if !file_type.is_file() {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "special file excluded")
                    .for_path(logical),
            );
            continue;
        }
        let canonical_file =
            fs::canonicalize(entry.path()).map_err(|error| VaultcError::io(entry.path(), error))?;
        if !canonical_file.starts_with(&canonical_root) {
            return Err(VaultcError::UnsafePath {
                path: entry.path().display().to_string(),
                reason: "resolved entry escaped source root".into(),
            });
        }
        let bytes = read_limited_file(entry.path(), policy.limits.max_file_bytes)?;
        let canonical_after =
            fs::canonicalize(entry.path()).map_err(|error| VaultcError::io(entry.path(), error))?;
        if !canonical_after.starts_with(&canonical_root) || canonical_after != canonical_file {
            return Err(VaultcError::IdentityMismatch(format!(
                "source path changed while reading `{}`",
                entry.path().display()
            )));
        }
        validate_structured_size(&logical, &bytes, policy)?;
        let next_files = entries.len().saturating_add(1);
        let next_bytes = total_bytes.saturating_add(bytes.len() as u64);
        enforce_file_limits(next_files, next_bytes, policy)?;
        total_bytes = next_bytes;
        entries.push(RawEntry {
            kind: FileKind::classify(&logical),
            logical_path: logical,
            bytes,
        });
    }
    let mut pruned = pruned_directories
        .lock()
        .map_err(|_| VaultcError::Internal("excluded-directory collector was poisoned".into()))?;
    pruned.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    pruned.dedup();
    diagnostics.extend(pruned.drain(..).map(|logical| {
        Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V1 policy")
            .for_path(logical)
    }));
    reject_duplicate_paths(&entries)?;
    Ok(entries)
}

#[cfg(feature = "archives")]
fn collect_zip(
    path: &Path,
    policy: &CompilerPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    let mut file = File::open(path).map_err(|error| VaultcError::io(path, error))?;
    let compressed_len = file
        .metadata()
        .map_err(|error| VaultcError::io(path, error))?
        .len();
    let declared_entries = declared_zip_entry_count(&mut file, compressed_len, path)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| VaultcError::io(path, error))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| VaultcError::MalformedInput {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;
    if archive.len() as u64 != declared_entries {
        return Err(VaultcError::UnsafePath {
            path: path.display().to_string(),
            reason: format!(
                "ZIP central directory declares {declared_entries} entries but the reader exposes {}; duplicate or shadowed members are forbidden",
                archive.len()
            ),
        });
    }
    let mut entries = Vec::new();
    let mut expanded = 0_u64;
    let excludes = build_excludes(policy)?;
    for index in 0..archive.len() {
        let mut item = archive
            .by_index(index)
            .map_err(|error| VaultcError::MalformedInput {
                path: path.display().to_string(),
                reason: error.to_string(),
            })?;
        if item.is_dir() {
            continue;
        }
        if item
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "archive symlink excluded")
                    .for_path(item.name()),
            );
            continue;
        }
        let enclosed = item
            .enclosed_name()
            .ok_or_else(|| VaultcError::UnsafePath {
                path: item.name().into(),
                reason: "ZIP member is absolute or traverses parents".into(),
            })?;
        let logical = normalize_logical_path(&enclosed, policy)?;
        if is_excluded(&logical, false, false, &excludes) {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V1 policy")
                    .for_path(logical),
            );
            continue;
        }
        if item.size() > policy.limits.max_file_bytes {
            return Err(VaultcError::ResourceLimit(format!(
                "ZIP member `{logical}` exceeds per-file limit"
            )));
        }
        expanded = expanded.saturating_add(item.size());
        enforce_archive_limits(compressed_len, expanded, entries.len() + 1, policy)?;
        enforce_expansion_ratio(item.compressed_size(), item.size(), policy)?;
        let declared_size = item.size();
        let bytes = read_exact_limited(
            &mut item,
            declared_size,
            policy.limits.max_file_bytes,
            path,
            &logical,
        )?;
        validate_structured_size(&logical, &bytes, policy)?;
        entries.push(RawEntry {
            kind: FileKind::classify(&logical),
            logical_path: logical,
            bytes,
        });
    }
    reject_duplicate_paths(&entries)?;
    Ok(entries)
}

#[cfg(feature = "archives")]
fn declared_zip_entry_count(file: &mut File, length: u64, path: &Path) -> Result<u64> {
    const EOCD_MINIMUM: usize = 22;
    const EOCD_SEARCH: u64 = EOCD_MINIMUM as u64 + u16::MAX as u64;
    if length < EOCD_MINIMUM as u64 {
        return Err(VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP is too short to contain an end-of-central-directory record".into(),
        });
    }
    let tail_length = length.min(EOCD_SEARCH) as usize;
    file.seek(SeekFrom::Start(length - tail_length as u64))
        .map_err(|error| VaultcError::io(path, error))?;
    let mut tail = vec![0_u8; tail_length];
    file.read_exact(&mut tail)
        .map_err(|error| VaultcError::io(path, error))?;

    let eocd = (0..=tail.len() - EOCD_MINIMUM)
        .rev()
        .find(|&offset| {
            tail[offset..].starts_with(b"PK\x05\x06")
                && read_u16_le(&tail, offset + 20).is_some_and(|comment_length| {
                    offset + EOCD_MINIMUM + comment_length as usize == tail.len()
                })
        })
        .ok_or_else(|| VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP end-of-central-directory record is missing or has trailing data".into(),
        })?;
    let disk = read_u16_le(&tail, eocd + 4).unwrap_or(u16::MAX);
    let directory_disk = read_u16_le(&tail, eocd + 6).unwrap_or(u16::MAX);
    let entries_on_disk = read_u16_le(&tail, eocd + 8).unwrap_or(u16::MAX);
    let total_entries = read_u16_le(&tail, eocd + 10).unwrap_or(u16::MAX);
    if disk != 0 || directory_disk != 0 || entries_on_disk != total_entries {
        return Err(VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "multi-disk ZIP archives are unsupported".into(),
        });
    }
    if total_entries != u16::MAX {
        return Ok(u64::from(total_entries));
    }

    if eocd < 20 || !tail[eocd - 20..].starts_with(b"PK\x06\x07") {
        return Err(VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP64 end-of-central-directory locator is missing".into(),
        });
    }
    let locator = eocd - 20;
    let zip64_disk = read_u32_le(&tail, locator + 4).unwrap_or(u32::MAX);
    let zip64_offset =
        read_u64_le(&tail, locator + 8).ok_or_else(|| VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "truncated ZIP64 locator".into(),
        })?;
    let total_disks = read_u32_le(&tail, locator + 16).unwrap_or(u32::MAX);
    if zip64_disk != 0 || total_disks != 1 || zip64_offset > length.saturating_sub(40) {
        return Err(VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "invalid or multi-disk ZIP64 locator".into(),
        });
    }
    file.seek(SeekFrom::Start(zip64_offset))
        .map_err(|error| VaultcError::io(path, error))?;
    let mut zip64 = [0_u8; 40];
    file.read_exact(&mut zip64)
        .map_err(|error| VaultcError::io(path, error))?;
    if !zip64.starts_with(b"PK\x06\x06") {
        return Err(VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP64 end-of-central-directory record is missing".into(),
        });
    }
    let zip64_record_disk = read_u32_le(&zip64, 16).unwrap_or(u32::MAX);
    let zip64_directory_disk = read_u32_le(&zip64, 20).unwrap_or(u32::MAX);
    let zip64_entries_on_disk = read_u64_le(&zip64, 24).unwrap_or(u64::MAX);
    let zip64_total_entries = read_u64_le(&zip64, 32).unwrap_or(u64::MAX);
    if zip64_record_disk != 0
        || zip64_directory_disk != 0
        || zip64_entries_on_disk != zip64_total_entries
    {
        return Err(VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: "multi-disk ZIP64 archives are unsupported".into(),
        });
    }
    Ok(zip64_total_entries)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64_le(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(not(feature = "archives"))]
fn collect_zip(
    path: &Path,
    _policy: &CompilerPolicy,
    _diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    Err(VaultcError::UnsupportedSource(path.to_path_buf()))
}

#[cfg(feature = "archives")]
fn collect_tar_zst(
    path: &Path,
    policy: &CompilerPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    let compressed_len = fs::metadata(path)
        .map_err(|error| VaultcError::io(path, error))?
        .len();
    let file = File::open(path).map_err(|error| VaultcError::io(path, error))?;
    let decoder = zstd::Decoder::new(file).map_err(|error| VaultcError::io(path, error))?;
    let mut archive = tar::Archive::new(decoder);
    let mut entries = Vec::new();
    let mut expanded = 0_u64;
    let excludes = build_excludes(policy)?;
    for item in archive
        .entries()
        .map_err(|error| VaultcError::io(path, error))?
    {
        let mut item = item.map_err(|error| VaultcError::io(path, error))?;
        let entry_type = item.header().entry_type();
        if !entry_type.is_file() {
            let display = item.path().map_or_else(
                |_| "<invalid tar path>".into(),
                |value| value.display().to_string(),
            );
            if entry_type.is_symlink() || entry_type.is_hard_link() {
                diagnostics.push(
                    Diagnostic::warning(DiagnosticCode::ExcludedPath, "archive link excluded")
                        .for_path(display),
                );
            }
            continue;
        }
        let member_path = item.path().map_err(|error| VaultcError::MalformedInput {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        let logical = normalize_logical_path(&member_path, policy)?;
        if is_excluded(&logical, false, false, &excludes) {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V1 policy")
                    .for_path(logical),
            );
            continue;
        }
        let size = item
            .header()
            .size()
            .map_err(|error| VaultcError::MalformedInput {
                path: path.display().to_string(),
                reason: error.to_string(),
            })?;
        if size > policy.limits.max_file_bytes {
            return Err(VaultcError::ResourceLimit(format!(
                "tar member `{logical}` exceeds per-file limit"
            )));
        }
        expanded = expanded.saturating_add(size);
        enforce_archive_limits(compressed_len, expanded, entries.len() + 1, policy)?;
        let bytes = read_exact_limited(
            &mut item,
            size,
            policy.limits.max_file_bytes,
            path,
            &logical,
        )?;
        validate_structured_size(&logical, &bytes, policy)?;
        entries.push(RawEntry {
            kind: FileKind::classify(&logical),
            logical_path: logical,
            bytes,
        });
    }
    reject_duplicate_paths(&entries)?;
    Ok(entries)
}

#[cfg(not(feature = "archives"))]
fn collect_tar_zst(
    path: &Path,
    _policy: &CompilerPolicy,
    _diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    Err(VaultcError::UnsupportedSource(path.to_path_buf()))
}

fn normalize_logical_path(path: &Path, policy: &CompilerPolicy) -> Result<String> {
    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or_else(|| VaultcError::UnsafePath {
                    path: path.display().to_string(),
                    reason: "non-UTF-8 path is forbidden".into(),
                })?;
                let normalized: String = value.nfc().collect();
                validate_component(&normalized, policy)?;
                segments.push(normalized);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(VaultcError::UnsafePath {
                    path: path.display().to_string(),
                    reason: "absolute and parent-traversing paths are forbidden".into(),
                });
            }
        }
    }
    if segments.is_empty() {
        return Err(VaultcError::UnsafePath {
            path: path.display().to_string(),
            reason: "empty logical path".into(),
        });
    }
    let logical = segments.join("/");
    if logical.len() > policy.limits.max_path_bytes {
        return Err(VaultcError::UnsafePath {
            path: logical,
            reason: format!("path exceeds {} bytes", policy.limits.max_path_bytes),
        });
    }
    Ok(logical)
}

pub(crate) fn validate_output_logical_path(path: &str, policy: &CompilerPolicy) -> Result<()> {
    if path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(VaultcError::UnsafePath {
            path: path.into(),
            reason: "output path must be a relative slash path".into(),
        });
    }
    let normalized = normalize_logical_path(Path::new(path), policy)?;
    if normalized != path {
        return Err(VaultcError::UnsafePath {
            path: path.into(),
            reason: "output path is not in canonical NFC form".into(),
        });
    }
    Ok(())
}

fn validate_component(component: &str, policy: &CompilerPolicy) -> Result<()> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.ends_with(' ')
        || component.ends_with('.')
        || component.chars().any(|character| {
            character == '\0'
                || character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return Err(VaultcError::UnsafePath {
            path: component.into(),
            reason: "invalid portable path component".into(),
        });
    }
    if component.len() > policy.limits.max_component_bytes {
        return Err(VaultcError::UnsafePath {
            path: component.into(),
            reason: format!(
                "component exceeds {} UTF-8 bytes",
                policy.limits.max_component_bytes
            ),
        });
    }
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        return Err(VaultcError::UnsafePath {
            path: component.into(),
            reason: "Windows-reserved component".into(),
        });
    }
    Ok(())
}

fn build_excludes(policy: &CompilerPolicy) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new("");
    for pattern in &policy.paths.exclude {
        builder.add_line(None, pattern).map_err(|error| {
            VaultcError::InvalidConfig(format!("invalid exclude pattern `{pattern}`: {error}"))
        })?;
    }
    builder.build().map_err(|error| {
        VaultcError::InvalidConfig(format!("invalid exclude pattern set: {error}"))
    })
}

fn is_excluded(logical_path: &str, is_symlink: bool, is_dir: bool, excludes: &Gitignore) -> bool {
    if is_symlink {
        return true;
    }
    let lower = logical_path.to_ascii_lowercase();
    if lower
        .split('/')
        .any(|component| matches!(component, ".obsidian" | ".git"))
        || lower.ends_with("/.ds_store")
        || lower == ".ds_store"
    {
        return true;
    }
    if excludes
        .matched_path_or_any_parents(Path::new(logical_path), is_dir)
        .is_ignore()
    {
        return true;
    }
    let basename = Path::new(&lower)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if basename == ".env"
        || basename.starts_with(".env.")
        || matches!(
            basename,
            "id_rsa"
                | "id_dsa"
                | "id_ecdsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
                | ".npmrc"
                | ".pypirc"
                | ".netrc"
        )
        || basename.starts_with("secrets.")
    {
        return true;
    }
    let extension = Path::new(&lower)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    matches!(
        extension,
        "exe"
            | "com"
            | "scr"
            | "dll"
            | "dylib"
            | "so"
            | "app"
            | "msi"
            | "msp"
            | "bat"
            | "cmd"
            | "ps1"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "command"
            | "vbs"
            | "vbe"
            | "js"
            | "jse"
            | "mjs"
            | "cjs"
            | "wsf"
            | "wsh"
            | "hta"
            | "jar"
            | "class"
            | "wasm"
            | "apk"
            | "deb"
            | "rpm"
            | "dmg"
            | "pkg"
            | "key"
            | "pem"
            | "p12"
            | "pfx"
    )
}

fn read_limited_file(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let link_metadata = fs::symlink_metadata(path).map_err(|error| VaultcError::io(path, error))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.file_type().is_file() {
        return Err(VaultcError::UnsafePath {
            path: path.display().to_string(),
            reason: "source changed to a symlink or special file before it was opened".into(),
        });
    }
    let mut file = File::open(path).map_err(|error| VaultcError::io(path, error))?;
    let before = file
        .metadata()
        .map_err(|error| VaultcError::io(path, error))?;
    if !before.is_file() {
        return Err(VaultcError::UnsafePath {
            path: path.display().to_string(),
            reason: "source is not a regular file".into(),
        });
    }
    if before.len() > maximum {
        return Err(VaultcError::ResourceLimit(format!(
            "`{}` exceeds per-file limit",
            path.display()
        )));
    }
    let before_modified = before.modified().ok();
    let capacity =
        usize::try_from(before.len().min(INITIAL_READ_CAPACITY)).unwrap_or(16 * 1024 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| VaultcError::io(path, error))?;
    if bytes.len() as u64 > maximum {
        return Err(VaultcError::ResourceLimit(format!(
            "`{}` grew beyond the per-file limit while being read",
            path.display()
        )));
    }
    let after = file
        .metadata()
        .map_err(|error| VaultcError::io(path, error))?;
    if after.len() != before.len()
        || bytes.len() as u64 != before.len()
        || (before_modified.is_some() && after.modified().ok() != before_modified)
    {
        return Err(VaultcError::IdentityMismatch(format!(
            "source changed while reading `{}`",
            path.display()
        )));
    }
    Ok(bytes)
}

fn validate_structured_size(path: &str, bytes: &[u8], policy: &CompilerPolicy) -> Result<()> {
    if FileKind::classify(path) != FileKind::Asset
        && bytes.len() as u64 > policy.limits.max_structured_text_bytes
    {
        return Err(VaultcError::ResourceLimit(format!(
            "structured file `{path}` exceeds text limit"
        )));
    }
    Ok(())
}

fn enforce_archive_limits(
    compressed: u64,
    expanded: u64,
    files: usize,
    policy: &CompilerPolicy,
) -> Result<()> {
    if files as u64 > policy.limits.max_files {
        return Err(VaultcError::ResourceLimit(
            "archive file count exceeds configured maximum".into(),
        ));
    }
    if expanded > policy.limits.max_total_bytes {
        return Err(VaultcError::ResourceLimit(
            "archive expanded bytes exceed configured maximum".into(),
        ));
    }
    enforce_expansion_ratio(compressed, expanded, policy)
}

fn enforce_file_limits(files: usize, total_bytes: u64, policy: &CompilerPolicy) -> Result<()> {
    if files as u64 > policy.limits.max_files {
        return Err(VaultcError::ResourceLimit(
            "file count exceeds configured maximum".into(),
        ));
    }
    if total_bytes > policy.limits.max_total_bytes {
        return Err(VaultcError::ResourceLimit(
            "total bytes exceed configured maximum".into(),
        ));
    }
    Ok(())
}

fn enforce_expansion_ratio(compressed: u64, expanded: u64, policy: &CompilerPolicy) -> Result<()> {
    let allowed = compressed
        .max(1)
        .saturating_mul(policy.limits.max_archive_expansion_ratio);
    if expanded > allowed {
        return Err(VaultcError::ResourceLimit(
            "archive expansion ratio exceeds configured maximum".into(),
        ));
    }
    Ok(())
}

fn reject_duplicate_paths(entries: &[RawEntry]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.logical_path.clone()) {
            return Err(VaultcError::UnsafePath {
                path: entry.logical_path.clone(),
                reason: "duplicate archive member".into(),
            });
        }
    }
    Ok(())
}

fn read_exact_limited<R: Read>(
    reader: R,
    expected: u64,
    limit: u64,
    source: &Path,
    logical_path: &str,
) -> Result<Vec<u8>> {
    if expected > limit {
        return Err(VaultcError::ResourceLimit(
            "stream exceeds configured per-file limit".into(),
        ));
    }
    let capacity = usize::try_from(expected.min(INITIAL_READ_CAPACITY)).unwrap_or(16 * 1024 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| VaultcError::io(source, error))?;
    if bytes.len() as u64 > limit {
        return Err(VaultcError::ResourceLimit(
            "stream exceeds configured per-file limit".into(),
        ));
    }
    if bytes.len() as u64 != expected {
        return Err(VaultcError::MalformedInput {
            path: logical_path.into(),
            reason: format!(
                "archive member length {} does not match declared length {expected}",
                bytes.len()
            ),
        });
    }
    Ok(bytes)
}

#[allow(dead_code)]
fn _cursor(bytes: Vec<u8>) -> Cursor<Vec<u8>> {
    Cursor::new(bytes)
}
