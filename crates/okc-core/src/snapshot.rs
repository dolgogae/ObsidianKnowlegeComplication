use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::config::CompilerPolicy;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::error::{OkcError, Result};
use crate::identity::{ContentHash, SnapshotId, SourceFileId};
use crate::ir::{CanonicalWorkspace, FileKind, SourceFile, SourcePathEncoding};
use crate::plan::Inspection;
use crate::source::{SourceId, SourceSpec};

pub(crate) const SOURCE_POLICY_ID: &str = "okc-source-v2";
const INITIAL_READ_CAPACITY: u64 = 16 * 1024 * 1024;
#[cfg(feature = "archives")]
const ARCHIVE_EXPANSION_LIMIT_ERROR: &str = "okc archive expansion limit exceeded";

#[cfg(feature = "archives")]
struct ExpansionBoundedReader<R> {
    inner: R,
    remaining: u64,
    exceeded: Arc<AtomicBool>,
}

#[cfg(feature = "archives")]
impl<R: Read> Read for ExpansionBoundedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let allowance = usize::try_from(
            self.remaining
                .saturating_add(1)
                .min(u64::try_from(buffer.len()).unwrap_or(u64::MAX)),
        )
        .unwrap_or(buffer.len());
        let read = self.inner.read(&mut buffer[..allowance])?;
        let read = u64::try_from(read).unwrap_or(u64::MAX);
        if read > self.remaining {
            self.exceeded.store(true, Ordering::Relaxed);
            return Err(io::Error::other(ARCHIVE_EXPANSION_LIMIT_ERROR));
        }
        self.remaining -= read;
        usize::try_from(read).map_err(|_| io::Error::other("archive read length overflow"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSnapshot {
    pub snapshot_id: SnapshotId,
    pub vault_content_id: ContentHash,
    pub source_id: SourceId,
    pub source: SourceSpec,
    pub policy_id: String,
    pub files: Vec<SourceFile>,
}

#[derive(Debug)]
pub(crate) struct RawEntry {
    pub original_path: String,
    pub logical_path: String,
    pub path_encoding: SourcePathEncoding,
    pub kind: FileKind,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
struct PendingFile {
    original_path: String,
    logical_path: String,
    path_encoding: SourcePathEncoding,
    kind: FileKind,
    byte_len: u64,
    raw_sha256: String,
    content_hash: ContentHash,
    file_id: SourceFileId,
}

#[allow(
    clippy::too_many_lines,
    reason = "source ordering, duplicate-Vault rejection, and optional workspace persistence form one inspection transaction"
)]
pub fn inspect_sources(
    sources: impl IntoIterator<Item = SourceSpec>,
    policy: &CompilerPolicy,
    workspace_path: Option<&Path>,
) -> Result<Inspection> {
    policy.validate()?;
    let sources: Vec<_> = sources.into_iter().collect();
    if sources.is_empty() {
        return Err(OkcError::InvalidConfig(
            "at least one source is required".into(),
        ));
    }
    if sources.len() > policy.limits.max_sources as usize {
        return Err(OkcError::ResourceLimit(format!(
            "{} sources exceeds configured maximum {}",
            sources.len(),
            policy.limits.max_sources
        )));
    }
    let unique: BTreeSet<_> = sources.iter().map(SourceSpec::source_id).collect();
    if unique.len() != sources.len() {
        return Err(OkcError::InvalidConfig(
            "source IDs must be unique within an inspection".into(),
        ));
    }

    let mut snapshots = Vec::with_capacity(sources.len());
    let mut canonical = CanonicalWorkspace::default();
    let mut diagnostics = Vec::new();
    let mut total_files = 0_u64;
    let mut total_bytes = 0_u64;

    for source in sources {
        let mut entries = collect_entries(&source, policy, &mut diagnostics)?;
        if policy.paths.nonstandard_files == crate::config::NonstandardFilePolicy::Exclude {
            entries.retain(|entry| {
                let extension = Path::new(&entry.logical_path)
                    .extension()
                    .and_then(|extension| extension.to_str());
                let explicitly_nonstandard = extension.is_some_and(|extension| {
                    policy
                        .paths
                        .nonstandard_extensions
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(extension))
                });
                if explicitly_nonstandard {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::ExcludedPath,
                            "nonstandard file excluded explicitly by policy",
                        )
                        .for_path(entry.logical_path.clone()),
                    );
                    false
                } else {
                    true
                }
            });
        }
        total_files = total_files.saturating_add(entries.len() as u64);
        total_bytes = total_bytes.saturating_add(
            entries
                .iter()
                .map(|entry| entry.bytes.len() as u64)
                .sum::<u64>(),
        );
        if total_files > policy.limits.max_files {
            return Err(OkcError::ResourceLimit(format!(
                "file count exceeds {}",
                policy.limits.max_files
            )));
        }
        if total_bytes > policy.limits.max_total_bytes {
            return Err(OkcError::ResourceLimit(format!(
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
    let mut vault_content_ids = BTreeSet::new();
    for snapshot in &snapshots {
        if !vault_content_ids.insert(snapshot.vault_content_id) {
            return Err(OkcError::InvalidConfig(format!(
                "Vault bytes were registered more than once; duplicate detected at source `{}`",
                snapshot.source_id
            )));
        }
    }
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
            original_path: entry.original_path.clone(),
            logical_path: entry.logical_path.clone(),
            path_encoding: entry.path_encoding,
            kind: entry.kind.clone(),
            byte_len: entry.bytes.len() as u64,
            raw_sha256: hex::encode(Sha256::digest(&entry.bytes)),
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
    let vault_content_id = crate::canonical::canonical_hash(
        "okc:vault-content:v2\0",
        &pending
            .iter()
            .map(|file| (&file.logical_path, file.file_id))
            .collect::<Vec<_>>(),
    )?;

    let files: Vec<_> = pending
        .into_iter()
        .map(|file| SourceFile {
            source_id: source.source_id().clone(),
            snapshot_id,
            file_id: file.file_id,
            original_path: file.original_path,
            logical_path: file.logical_path,
            path_encoding: file.path_encoding,
            kind: file.kind,
            byte_len: file.byte_len,
            raw_sha256: file.raw_sha256,
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
            vault_content_id,
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
    expected_file: &SourceFile,
    policy: &CompilerPolicy,
) -> Result<Vec<u8>> {
    let mut diagnostics = Vec::new();
    let entries = collect_entries(source, policy, &mut diagnostics)?;
    let entry = entries
        .into_iter()
        .find(|entry| entry.logical_path == expected_file.logical_path)
        .ok_or_else(|| OkcError::IdentityMismatch(expected_file.logical_path.clone()))?;
    if entry.original_path != expected_file.original_path
        || entry.path_encoding != expected_file.path_encoding
    {
        return Err(OkcError::IdentityMismatch(format!(
            "source path spelling changed for `{}`",
            expected_file.logical_path
        )));
    }
    Ok(entry.bytes)
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
    let metadata = fs::metadata(root).map_err(|error| OkcError::io(root, error))?;
    if !metadata.is_dir() {
        return Err(OkcError::UnsupportedSource(root.to_path_buf()));
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| OkcError::io(root, error))?;
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
    let filter_policy = policy.clone();
    builder.filter_entry(move |entry| {
        if entry.path() == filter_root || !entry.file_type().is_some_and(|kind| kind.is_dir()) {
            return true;
        }
        let Some(relative) = entry.path().strip_prefix(&filter_root).ok() else {
            return true;
        };
        let Ok(path_pair) = decode_directory_path(relative, &filter_policy) else {
            // Do not let pruning hide an invalid path. The main traversal reports it.
            return true;
        };
        let should_prune = is_excluded(&path_pair.logical_path, false, true, &filter_excludes);
        if should_prune && let Ok(mut paths) = filter_pruned.lock() {
            paths.push(path_pair.logical_path);
        }
        !should_prune
    });
    let mut entries = Vec::new();
    let mut seen_paths = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for result in builder.build() {
        let entry = result.map_err(|error| OkcError::MalformedInput {
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
            .map_err(|_| OkcError::UnsafePath {
                path: entry.path().display().to_string(),
                reason: "entry escaped source root".into(),
            })?;
        let path_pair = decode_directory_path(relative, policy)?;
        reject_seen_path(&mut seen_paths, &path_pair.logical_path)?;
        if is_excluded(
            &path_pair.logical_path,
            file_type.is_symlink(),
            file_type.is_dir(),
            &excludes,
        ) {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V2 policy")
                    .for_path(path_pair.logical_path),
            );
            continue;
        }
        if file_type.is_dir() {
            continue;
        }
        if file_type.is_symlink() {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "symlink not followed")
                    .for_path(path_pair.logical_path),
            );
            continue;
        }
        if !file_type.is_file() {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "special file excluded")
                    .for_path(path_pair.logical_path),
            );
            continue;
        }
        let canonical_file =
            fs::canonicalize(entry.path()).map_err(|error| OkcError::io(entry.path(), error))?;
        if !canonical_file.starts_with(&canonical_root) {
            return Err(OkcError::UnsafePath {
                path: entry.path().display().to_string(),
                reason: "resolved entry escaped source root".into(),
            });
        }
        let bytes = read_limited_file(entry.path(), policy.limits.max_file_bytes)?;
        let canonical_after =
            fs::canonicalize(entry.path()).map_err(|error| OkcError::io(entry.path(), error))?;
        if !canonical_after.starts_with(&canonical_root) || canonical_after != canonical_file {
            return Err(OkcError::IdentityMismatch(format!(
                "source path changed while reading `{}`",
                entry.path().display()
            )));
        }
        validate_structured_size(&path_pair.logical_path, &bytes, policy)?;
        let next_files = entries.len().saturating_add(1);
        let next_bytes = total_bytes.saturating_add(bytes.len() as u64);
        enforce_file_limits(next_files, next_bytes, policy)?;
        total_bytes = next_bytes;
        entries.push(RawEntry {
            kind: FileKind::classify(&path_pair.logical_path),
            original_path: path_pair.original_path,
            logical_path: path_pair.logical_path,
            path_encoding: SourcePathEncoding::Utf8,
            bytes,
        });
    }
    let mut pruned = pruned_directories
        .lock()
        .map_err(|_| OkcError::Internal("excluded-directory collector was poisoned".into()))?;
    pruned.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    pruned.dedup();
    diagnostics.extend(pruned.drain(..).map(|logical| {
        Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V2 policy")
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
    let mut file = File::open(path).map_err(|error| OkcError::io(path, error))?;
    let compressed_len = file
        .metadata()
        .map_err(|error| OkcError::io(path, error))?
        .len();
    if compressed_len > policy.limits.max_file_bytes {
        return Err(OkcError::ResourceLimit(format!(
            "ZIP container `{}` exceeds per-file limit",
            path.display()
        )));
    }
    let central_entries = scan_zip_central_entries(&mut file, compressed_len, path, policy)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| OkcError::io(path, error))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| OkcError::MalformedInput {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;
    if archive.len() != central_entries.len() {
        return Err(OkcError::UnsafePath {
            path: path.display().to_string(),
            reason: format!(
                "ZIP central directory declares {} entries but the reader exposes {}; duplicate or shadowed members are forbidden",
                central_entries.len(),
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
            .map_err(|error| OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: error.to_string(),
            })?;
        let central_entry = central_entries
            .get(index)
            .ok_or_else(|| OkcError::Internal("ZIP central-entry index was not sealed".into()))?;
        if item.central_header_start() != central_entry.header_offset {
            return Err(OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: "ZIP reader order disagrees with the scanned central directory".into(),
            });
        }
        let unix_mode = item.unix_mode();
        if central_entry.is_directory {
            continue;
        }
        if unix_mode.is_some_and(|mode| mode & 0o170_000 == 0o120_000) {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "archive symlink excluded")
                    .for_path(central_entry.path.logical_path.clone()),
            );
            continue;
        }
        if is_excluded(&central_entry.path.logical_path, false, false, &excludes) {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V2 policy")
                    .for_path(central_entry.path.logical_path.clone()),
            );
            continue;
        }
        if item.size() > policy.limits.max_file_bytes {
            return Err(OkcError::ResourceLimit(format!(
                "ZIP member `{}` exceeds per-file limit",
                central_entry.path.logical_path
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
            &central_entry.path.logical_path,
        )?;
        validate_structured_size(&central_entry.path.logical_path, &bytes, policy)?;
        entries.push(RawEntry {
            kind: FileKind::classify(&central_entry.path.logical_path),
            original_path: central_entry.path.original_path.clone(),
            logical_path: central_entry.path.logical_path.clone(),
            path_encoding: SourcePathEncoding::Utf8,
            bytes,
        });
    }
    reject_duplicate_paths(&entries)?;
    Ok(entries)
}

#[cfg(feature = "archives")]
#[derive(Debug)]
struct ZipCentralDirectory {
    entries: u64,
    offset: u64,
    size: u64,
}

#[cfg(feature = "archives")]
#[derive(Debug)]
struct ZipCentralEntry {
    path: SourcePathPair,
    is_directory: bool,
    header_offset: u64,
}

#[cfg(feature = "archives")]
fn scan_zip_central_entries(
    file: &mut File,
    length: u64,
    path: &Path,
    policy: &CompilerPolicy,
) -> Result<Vec<ZipCentralEntry>> {
    let directory = read_zip_central_directory(file, length, path)?;
    if directory.entries > policy.limits.max_files {
        return Err(OkcError::ResourceLimit(format!(
            "ZIP declared member count {} exceeds configured maximum {}",
            directory.entries, policy.limits.max_files
        )));
    }
    let end = directory
        .offset
        .checked_add(directory.size)
        .filter(|end| *end <= length)
        .ok_or_else(|| OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP central-directory range is outside the container".into(),
        })?;
    file.seek(SeekFrom::Start(directory.offset))
        .map_err(|error| OkcError::io(path, error))?;
    let capacity = usize::try_from(directory.entries).map_err(|_| {
        OkcError::ResourceLimit("ZIP declared member count does not fit this platform".into())
    })?;
    let mut entries = Vec::with_capacity(capacity);
    let mut seen_paths = BTreeSet::new();
    let mut cursor = directory.offset;
    for _ in 0..directory.entries {
        let header_offset = cursor;
        if cursor.checked_add(46).is_none_or(|next| next > end) {
            return Err(OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: "truncated ZIP central-directory header".into(),
            });
        }
        let mut header = [0_u8; 46];
        file.read_exact(&mut header)
            .map_err(|error| OkcError::io(path, error))?;
        if !header.starts_with(b"PK\x01\x02") {
            return Err(OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: "invalid ZIP central-directory header signature".into(),
            });
        }
        let name_length = u64::from(read_u16_le(&header, 28).unwrap_or(0));
        let extra_length = u64::from(read_u16_le(&header, 30).unwrap_or(0));
        let comment_length = u64::from(read_u16_le(&header, 32).unwrap_or(0));
        let variable_length = name_length
            .checked_add(extra_length)
            .and_then(|value| value.checked_add(comment_length))
            .ok_or_else(|| OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: "ZIP central-directory field lengths overflow".into(),
            })?;
        cursor = cursor
            .checked_add(46)
            .and_then(|value| value.checked_add(variable_length))
            .filter(|next| *next <= end)
            .ok_or_else(|| OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: "ZIP central-directory member exceeds declared range".into(),
            })?;
        let name_capacity = usize::try_from(name_length).map_err(|_| {
            OkcError::ResourceLimit("ZIP filename length does not fit this platform".into())
        })?;
        let mut raw_name = vec![0_u8; name_capacity];
        file.read_exact(&mut raw_name)
            .map_err(|error| OkcError::io(path, error))?;
        let raw_name_text =
            std::str::from_utf8(&raw_name).map_err(|error| OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: format!("ZIP central-directory filename is not UTF-8: {error}"),
            })?;
        let made_by = header[5];
        let unix_mode = if made_by == 3 {
            read_u32_le(&header, 38).map(|attributes| attributes >> 16)
        } else {
            None
        };
        let is_directory =
            raw_name.ends_with(b"/") || unix_mode.is_some_and(|mode| mode & 0o170_000 == 0o040_000);
        let path_pair = decode_archive_path(raw_name_text, is_directory, policy)?;
        reject_seen_path(&mut seen_paths, &path_pair.logical_path)?;
        let skip = i64::try_from(extra_length.saturating_add(comment_length)).map_err(|_| {
            OkcError::ResourceLimit("ZIP metadata length does not fit this platform".into())
        })?;
        file.seek(SeekFrom::Current(skip))
            .map_err(|error| OkcError::io(path, error))?;
        entries.push(ZipCentralEntry {
            path: path_pair,
            is_directory,
            header_offset,
        });
    }
    if cursor != end {
        return Err(OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP central-directory size does not match its member headers".into(),
        });
    }
    Ok(entries)
}

#[cfg(feature = "archives")]
#[allow(
    clippy::too_many_lines,
    reason = "EOCD and ZIP64 parsing are one fail-closed central-directory boundary"
)]
fn read_zip_central_directory(
    file: &mut File,
    length: u64,
    path: &Path,
) -> Result<ZipCentralDirectory> {
    const EOCD_MINIMUM: usize = 22;
    const EOCD_SEARCH: u64 = EOCD_MINIMUM as u64 + u16::MAX as u64;
    if length < EOCD_MINIMUM as u64 {
        return Err(OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP is too short to contain an end-of-central-directory record".into(),
        });
    }
    let tail_length = length.min(EOCD_SEARCH) as usize;
    file.seek(SeekFrom::Start(length - tail_length as u64))
        .map_err(|error| OkcError::io(path, error))?;
    let mut tail = vec![0_u8; tail_length];
    file.read_exact(&mut tail)
        .map_err(|error| OkcError::io(path, error))?;

    let eocd = (0..=tail.len() - EOCD_MINIMUM)
        .rev()
        .find(|&offset| {
            tail[offset..].starts_with(b"PK\x05\x06")
                && read_u16_le(&tail, offset + 20).is_some_and(|comment_length| {
                    offset + EOCD_MINIMUM + comment_length as usize == tail.len()
                })
        })
        .ok_or_else(|| OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP end-of-central-directory record is missing or has trailing data".into(),
        })?;
    let disk = read_u16_le(&tail, eocd + 4).unwrap_or(u16::MAX);
    let directory_disk = read_u16_le(&tail, eocd + 6).unwrap_or(u16::MAX);
    let entries_on_disk = read_u16_le(&tail, eocd + 8).unwrap_or(u16::MAX);
    let total_entries = read_u16_le(&tail, eocd + 10).unwrap_or(u16::MAX);
    if disk != 0 || directory_disk != 0 || entries_on_disk != total_entries {
        return Err(OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "multi-disk ZIP archives are unsupported".into(),
        });
    }
    let directory_size = read_u32_le(&tail, eocd + 12).unwrap_or(u32::MAX);
    let directory_offset = read_u32_le(&tail, eocd + 16).unwrap_or(u32::MAX);
    if total_entries != u16::MAX && directory_size != u32::MAX && directory_offset != u32::MAX {
        return Ok(ZipCentralDirectory {
            entries: u64::from(total_entries),
            offset: u64::from(directory_offset),
            size: u64::from(directory_size),
        });
    }

    if eocd < 20 || !tail[eocd - 20..].starts_with(b"PK\x06\x07") {
        return Err(OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "ZIP64 end-of-central-directory locator is missing".into(),
        });
    }
    let locator = eocd - 20;
    let zip64_disk = read_u32_le(&tail, locator + 4).unwrap_or(u32::MAX);
    let zip64_offset = read_u64_le(&tail, locator + 8).ok_or_else(|| OkcError::MalformedInput {
        path: path.display().to_string(),
        reason: "truncated ZIP64 locator".into(),
    })?;
    let total_disks = read_u32_le(&tail, locator + 16).unwrap_or(u32::MAX);
    if zip64_disk != 0 || total_disks != 1 || zip64_offset > length.saturating_sub(40) {
        return Err(OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "invalid or multi-disk ZIP64 locator".into(),
        });
    }
    file.seek(SeekFrom::Start(zip64_offset))
        .map_err(|error| OkcError::io(path, error))?;
    let mut zip64 = [0_u8; 56];
    file.read_exact(&mut zip64)
        .map_err(|error| OkcError::io(path, error))?;
    if !zip64.starts_with(b"PK\x06\x06") {
        return Err(OkcError::MalformedInput {
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
        return Err(OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "multi-disk ZIP64 archives are unsupported".into(),
        });
    }
    let zip64_directory_size = read_u64_le(&zip64, 40).ok_or_else(|| OkcError::MalformedInput {
        path: path.display().to_string(),
        reason: "truncated ZIP64 central-directory size".into(),
    })?;
    let zip64_directory_offset =
        read_u64_le(&zip64, 48).ok_or_else(|| OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: "truncated ZIP64 central-directory offset".into(),
        })?;
    Ok(ZipCentralDirectory {
        entries: zip64_total_entries,
        offset: zip64_directory_offset,
        size: zip64_directory_size,
    })
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
    Err(OkcError::UnsupportedSource(path.to_path_buf()))
}

#[cfg(feature = "archives")]
fn collect_tar_zst(
    path: &Path,
    policy: &CompilerPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    let compressed_len = fs::metadata(path)
        .map_err(|error| OkcError::io(path, error))?
        .len();
    if compressed_len > policy.limits.max_file_bytes {
        return Err(OkcError::ResourceLimit(format!(
            "tar.zst container `{}` exceeds per-file limit",
            path.display()
        )));
    }
    let file = File::open(path).map_err(|error| OkcError::io(path, error))?;
    let decoder = zstd::Decoder::new(file).map_err(|error| OkcError::io(path, error))?;
    let expansion_limit = compressed_len
        .max(1)
        .saturating_mul(policy.limits.max_archive_expansion_ratio);
    let expansion_exceeded = Arc::new(AtomicBool::new(false));
    let bounded = ExpansionBoundedReader {
        inner: decoder,
        remaining: expansion_limit,
        exceeded: Arc::clone(&expansion_exceeded),
    };
    let mut archive = tar::Archive::new(bounded);
    let mut entries = Vec::new();
    let mut expanded = 0_u64;
    let mut visited = 0_usize;
    let mut seen_paths = BTreeSet::new();
    let excludes = build_excludes(policy)?;
    for item in archive
        .entries()
        .map_err(|error| map_tar_io_error(path, &expansion_exceeded, error))?
    {
        let mut item = item.map_err(|error| map_tar_io_error(path, &expansion_exceeded, error))?;
        visited = visited
            .checked_add(1)
            .ok_or_else(|| OkcError::ResourceLimit("tar member count overflow".into()))?;
        let entry_type = item.header().entry_type();
        let raw_path = strict_tar_path_bytes(&mut item, path, &expansion_exceeded)?;
        let raw_path_text =
            std::str::from_utf8(&raw_path).map_err(|error| OkcError::MalformedInput {
                path: path.display().to_string(),
                reason: format!("tar member path is not UTF-8: {error}"),
            })?;
        let path_pair = decode_archive_path(raw_path_text, entry_type.is_dir(), policy)?;
        reject_seen_path(&mut seen_paths, &path_pair.logical_path)?;
        let size = item.size();
        expanded = expanded.saturating_add(size);
        enforce_archive_limits(compressed_len, expanded, visited, policy)?;
        if !entry_type.is_file() {
            if entry_type.is_symlink() || entry_type.is_hard_link() {
                diagnostics.push(
                    Diagnostic::warning(DiagnosticCode::ExcludedPath, "archive link excluded")
                        .for_path(path_pair.logical_path),
                );
            }
            continue;
        }
        if is_excluded(&path_pair.logical_path, false, false, &excludes) {
            diagnostics.push(
                Diagnostic::warning(DiagnosticCode::ExcludedPath, "path excluded by V2 policy")
                    .for_path(path_pair.logical_path),
            );
            continue;
        }
        if size > policy.limits.max_file_bytes {
            return Err(OkcError::ResourceLimit(format!(
                "tar member `{}` exceeds per-file limit",
                path_pair.logical_path
            )));
        }
        let bytes = read_exact_limited(
            &mut item,
            size,
            policy.limits.max_file_bytes,
            path,
            &path_pair.logical_path,
        );
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(_) if expansion_exceeded.load(Ordering::Relaxed) => {
                return Err(OkcError::ResourceLimit(
                    "tar.zst expansion ratio exceeds configured maximum".into(),
                ));
            }
            Err(error) => return Err(error),
        };
        validate_structured_size(&path_pair.logical_path, &bytes, policy)?;
        entries.push(RawEntry {
            kind: FileKind::classify(&path_pair.logical_path),
            original_path: path_pair.original_path,
            logical_path: path_pair.logical_path,
            path_encoding: SourcePathEncoding::Utf8,
            bytes,
        });
    }
    let mut bounded = archive.into_inner();
    io::copy(&mut bounded, &mut io::sink())
        .map_err(|error| map_tar_io_error(path, &expansion_exceeded, error))?;
    reject_duplicate_paths(&entries)?;
    Ok(entries)
}

#[cfg(feature = "archives")]
fn map_tar_io_error(path: &Path, exceeded: &AtomicBool, error: io::Error) -> OkcError {
    if exceeded.load(Ordering::Relaxed) || error.to_string().contains(ARCHIVE_EXPANSION_LIMIT_ERROR)
    {
        OkcError::ResourceLimit("tar.zst expansion ratio exceeds configured maximum".into())
    } else {
        OkcError::io(path, error)
    }
}

#[cfg(feature = "archives")]
fn strict_tar_path_bytes<R: Read>(
    item: &mut tar::Entry<'_, R>,
    source_path: &Path,
    expansion_exceeded: &AtomicBool,
) -> Result<Vec<u8>> {
    let mut pax_path = None;
    if let Some(extensions) = item
        .pax_extensions()
        .map_err(|error| map_tar_io_error(source_path, expansion_exceeded, error))?
    {
        for extension in extensions {
            let extension = extension.map_err(|error| OkcError::MalformedInput {
                path: source_path.display().to_string(),
                reason: format!("malformed PAX extension: {error}"),
            })?;
            if extension.key_bytes() == b"path"
                && pax_path.replace(extension.value_bytes().to_vec()).is_some()
            {
                return Err(OkcError::MalformedInput {
                    path: source_path.display().to_string(),
                    reason: "duplicate PAX path field".into(),
                });
            }
        }
    }
    let effective_path = item.path_bytes().into_owned();
    if pax_path
        .as_deref()
        .is_some_and(|declared| declared != effective_path)
    {
        return Err(OkcError::MalformedInput {
            path: source_path.display().to_string(),
            reason: "ambiguous GNU/PAX tar path carriers disagree".into(),
        });
    }
    Ok(effective_path)
}

#[cfg(not(feature = "archives"))]
fn collect_tar_zst(
    path: &Path,
    _policy: &CompilerPolicy,
    _diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<RawEntry>> {
    Err(OkcError::UnsupportedSource(path.to_path_buf()))
}

#[derive(Debug)]
struct SourcePathPair {
    original_path: String,
    logical_path: String,
}

fn decode_directory_path(path: &Path, policy: &CompilerPolicy) -> Result<SourcePathPair> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or_else(|| OkcError::UnsafePath {
                    path: path.display().to_string(),
                    reason: "non-UTF-8 path is forbidden".into(),
                })?;
                components.push(value.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(OkcError::UnsafePath {
                    path: path.display().to_string(),
                    reason: "absolute and parent-traversing paths are forbidden".into(),
                });
            }
        }
    }
    build_source_path_pair(components, &path.display().to_string(), policy)
}

fn decode_archive_path(
    raw_path: &str,
    is_directory: bool,
    policy: &CompilerPolicy,
) -> Result<SourcePathPair> {
    if raw_path.starts_with('/') || raw_path.contains('\\') || raw_path.contains('\0') {
        return Err(OkcError::UnsafePath {
            path: raw_path.into(),
            reason: "archive path must be a relative slash path".into(),
        });
    }
    let without_directory_marker = if is_directory {
        raw_path.strip_suffix('/').unwrap_or(raw_path)
    } else {
        raw_path
    };
    let mut components = Vec::new();
    for component in without_directory_marker.split('/') {
        match component {
            "." => {}
            "" => {
                return Err(OkcError::UnsafePath {
                    path: raw_path.into(),
                    reason: "archive path contains an empty component".into(),
                });
            }
            ".." => {
                return Err(OkcError::UnsafePath {
                    path: raw_path.into(),
                    reason: "archive path traverses a parent".into(),
                });
            }
            value => components.push(value.to_owned()),
        }
    }
    build_source_path_pair(components, raw_path, policy)
}

fn build_source_path_pair(
    components: Vec<String>,
    display_path: &str,
    policy: &CompilerPolicy,
) -> Result<SourcePathPair> {
    if components.is_empty() {
        return Err(OkcError::UnsafePath {
            path: display_path.into(),
            reason: "empty source path".into(),
        });
    }
    let mut original_components = Vec::with_capacity(components.len());
    let mut logical_components = Vec::with_capacity(components.len());
    for component in components {
        validate_component(&component, policy)?;
        let normalized: String = component.nfc().collect();
        validate_component(&normalized, policy)?;
        original_components.push(component);
        logical_components.push(normalized);
    }
    let original_path = original_components.join("/");
    let logical_path = logical_components.join("/");
    validate_path_length(&original_path, policy)?;
    validate_path_length(&logical_path, policy)?;
    Ok(SourcePathPair {
        original_path,
        logical_path,
    })
}

fn validate_path_length(path: &str, policy: &CompilerPolicy) -> Result<()> {
    if path.len() > policy.limits.max_path_bytes {
        return Err(OkcError::UnsafePath {
            path: path.into(),
            reason: format!("path exceeds {} bytes", policy.limits.max_path_bytes),
        });
    }
    Ok(())
}

pub(crate) fn validate_source_path_pair(
    original_path: &str,
    logical_path: &str,
    path_encoding: SourcePathEncoding,
    policy: &CompilerPolicy,
) -> Result<()> {
    if !matches!(path_encoding, SourcePathEncoding::Utf8) {
        return Err(OkcError::UnsafePath {
            path: original_path.into(),
            reason: "unsupported source path encoding".into(),
        });
    }
    let decoded = decode_archive_path(original_path, false, policy)?;
    if decoded.original_path != original_path || decoded.logical_path != logical_path {
        return Err(OkcError::IdentityMismatch(format!(
            "source path pair is inconsistent: `{original_path}` / `{logical_path}`"
        )));
    }
    Ok(())
}

pub(crate) fn validate_output_logical_path(path: &str, policy: &CompilerPolicy) -> Result<()> {
    let decoded = decode_archive_path(path, false, policy)?;
    if decoded.original_path != path || decoded.logical_path != path {
        return Err(OkcError::UnsafePath {
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
        return Err(OkcError::UnsafePath {
            path: component.into(),
            reason: "invalid portable path component".into(),
        });
    }
    if component.len() > policy.limits.max_component_bytes {
        return Err(OkcError::UnsafePath {
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
        return Err(OkcError::UnsafePath {
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
            OkcError::InvalidConfig(format!("invalid exclude pattern `{pattern}`: {error}"))
        })?;
    }
    builder
        .build()
        .map_err(|error| OkcError::InvalidConfig(format!("invalid exclude pattern set: {error}")))
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
    let link_metadata = fs::symlink_metadata(path).map_err(|error| OkcError::io(path, error))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.file_type().is_file() {
        return Err(OkcError::UnsafePath {
            path: path.display().to_string(),
            reason: "source changed to a symlink or special file before it was opened".into(),
        });
    }
    let mut file = File::open(path).map_err(|error| OkcError::io(path, error))?;
    let before = file.metadata().map_err(|error| OkcError::io(path, error))?;
    if !before.is_file() {
        return Err(OkcError::UnsafePath {
            path: path.display().to_string(),
            reason: "source is not a regular file".into(),
        });
    }
    if before.len() > maximum {
        return Err(OkcError::ResourceLimit(format!(
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
        .map_err(|error| OkcError::io(path, error))?;
    if bytes.len() as u64 > maximum {
        return Err(OkcError::ResourceLimit(format!(
            "`{}` grew beyond the per-file limit while being read",
            path.display()
        )));
    }
    let after = file.metadata().map_err(|error| OkcError::io(path, error))?;
    if after.len() != before.len()
        || bytes.len() as u64 != before.len()
        || (before_modified.is_some() && after.modified().ok() != before_modified)
    {
        return Err(OkcError::IdentityMismatch(format!(
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
        return Err(OkcError::ResourceLimit(format!(
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
        return Err(OkcError::ResourceLimit(
            "archive file count exceeds configured maximum".into(),
        ));
    }
    if expanded > policy.limits.max_total_bytes {
        return Err(OkcError::ResourceLimit(
            "archive expanded bytes exceed configured maximum".into(),
        ));
    }
    enforce_expansion_ratio(compressed, expanded, policy)
}

fn enforce_file_limits(files: usize, total_bytes: u64, policy: &CompilerPolicy) -> Result<()> {
    if files as u64 > policy.limits.max_files {
        return Err(OkcError::ResourceLimit(
            "file count exceeds configured maximum".into(),
        ));
    }
    if total_bytes > policy.limits.max_total_bytes {
        return Err(OkcError::ResourceLimit(
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
        return Err(OkcError::ResourceLimit(
            "archive expansion ratio exceeds configured maximum".into(),
        ));
    }
    Ok(())
}

fn reject_duplicate_paths(entries: &[RawEntry]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for entry in entries {
        reject_seen_path(&mut seen, &entry.logical_path)?;
    }
    Ok(())
}

fn reject_seen_path(seen: &mut BTreeSet<String>, logical_path: &str) -> Result<()> {
    if !seen.insert(logical_path.to_owned()) {
        return Err(OkcError::UnsafePath {
            path: logical_path.into(),
            reason: "duplicate source path after NFC normalization".into(),
        });
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
        return Err(OkcError::ResourceLimit(
            "stream exceeds configured per-file limit".into(),
        ));
    }
    let capacity = usize::try_from(expected.min(INITIAL_READ_CAPACITY)).unwrap_or(16 * 1024 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| OkcError::io(source, error))?;
    if bytes.len() as u64 > limit {
        return Err(OkcError::ResourceLimit(
            "stream exceeds configured per-file limit".into(),
        ));
    }
    if bytes.len() as u64 != expected {
        return Err(OkcError::MalformedInput {
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
