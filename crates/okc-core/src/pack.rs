use std::path::{Component, Path, PathBuf};

#[cfg(feature = "archives")]
use sha2::{Digest as _, Sha256};
#[cfg(feature = "archives")]
use std::collections::BTreeSet;
#[cfg(feature = "archives")]
use std::fs::{self, File};
#[cfg(feature = "archives")]
use std::io::{BufReader, Read, Write};

use crate::error::{OkcError, Result};

pub(crate) const PACK_PROFILE: &str = "okc-tar-zstd-deterministic-v2";

#[cfg(feature = "archives")]
pub(crate) struct PackObservation {
    pub raw_sha256: String,
    pub content_hash: crate::identity::ContentHash,
    pub byte_len: u64,
}

#[cfg(feature = "archives")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackPublicationStep {
    Write,
    Finish,
    Flush,
    FileSync,
    StagedVerify,
    Publish,
    ParentSync,
}

#[cfg(feature = "archives")]
trait PackPublicationHook {
    fn checkpoint(&self, _step: PackPublicationStep, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "archives")]
struct ProductionPackPublicationHook;

#[cfg(feature = "archives")]
impl PackPublicationHook for ProductionPackPublicationHook {}

#[cfg(feature = "archives")]
fn write_pack_stream<W: Write>(
    compiled_vault: &Path,
    output: W,
    error_path: &Path,
    zstd_level: i32,
    hook: &impl PackPublicationHook,
) -> Result<W> {
    let files = crate::compile::inventory(compiled_vault, &[])?;
    let encoder =
        zstd::Encoder::new(output, zstd_level).map_err(|error| OkcError::io(error_path, error))?;
    let mut archive = tar::Builder::new(encoder);
    archive.mode(tar::HeaderMode::Deterministic);
    for file in files {
        let path = compiled_vault.join(&file.path);
        let mut input = File::open(&path).map_err(|error| OkcError::io(&path, error))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(file.byte_len);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        archive
            .append_data(&mut header, &file.path, &mut input)
            .map_err(|error| OkcError::io(error_path, error))?;
    }
    hook.checkpoint(PackPublicationStep::Write, error_path)
        .map_err(|error| OkcError::io(error_path, error))?;
    let encoder = archive
        .into_inner()
        .map_err(|error| OkcError::io(error_path, error))?;
    let mut output = encoder
        .finish()
        .map_err(|error| OkcError::io(error_path, error))?;
    hook.checkpoint(PackPublicationStep::Finish, error_path)
        .map_err(|error| OkcError::io(error_path, error))?;
    output
        .flush()
        .map_err(|error| OkcError::io(error_path, error))?;
    hook.checkpoint(PackPublicationStep::Flush, error_path)
        .map_err(|error| OkcError::io(error_path, error))?;
    Ok(output)
}

#[cfg(feature = "archives")]
fn write_pack_path_raw(compiled_vault: &Path, destination: &Path, zstd_level: i32) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let output = options
        .open(destination)
        .map_err(|error| OkcError::io(destination, error))?;
    let output = write_pack_stream(
        compiled_vault,
        output,
        destination,
        zstd_level,
        &ProductionPackPublicationHook,
    )?;
    output
        .sync_all()
        .map_err(|error| OkcError::io(destination, error))
}

#[cfg(feature = "archives")]
pub fn create_pack(compiled_vault: &Path, destination: &Path, zstd_level: i32) -> Result<()> {
    create_pack_with_hook(
        compiled_vault,
        destination,
        zstd_level,
        &ProductionPackPublicationHook,
    )
}

#[cfg(feature = "archives")]
fn create_pack_with_hook(
    compiled_vault: &Path,
    destination: &Path,
    zstd_level: i32,
    hook: &impl PackPublicationHook,
) -> Result<()> {
    let sealed_level = sealed_zstd_level(compiled_vault)?;
    if zstd_level != sealed_level {
        return Err(OkcError::InvalidConfig(format!(
            "OKCPack zstd level {zstd_level} disagrees with sealed plan level {sealed_level}"
        )));
    }
    require_compiled_vault_directory(compiled_vault)?;
    crate::verify::verify_directory(compiled_vault)?;
    preflight_pack_destination(compiled_vault, destination)?;

    let parent = destination_parent(destination);
    let mut staged = tempfile::Builder::new()
        .prefix(".okc-pack-")
        .suffix(".okcpack")
        .tempfile_in(parent)
        .map_err(|error| OkcError::io(parent, error))?;
    let staged_path = staged.path().to_path_buf();
    write_pack_stream(
        compiled_vault,
        staged.as_file_mut(),
        &staged_path,
        zstd_level,
        hook,
    )?;
    staged
        .as_file()
        .sync_all()
        .map_err(|error| OkcError::io(&staged_path, error))?;
    hook.checkpoint(PackPublicationStep::FileSync, &staged_path)
        .map_err(|error| OkcError::io(&staged_path, error))?;
    crate::verify::verify_artifact(&staged_path)?;
    hook.checkpoint(PackPublicationStep::StagedVerify, &staged_path)
        .map_err(|error| OkcError::io(&staged_path, error))?;

    let published = match staged.persist_noclobber(destination) {
        Ok(file) => file,
        Err(error) => {
            let source = error.error;
            if source.kind() == std::io::ErrorKind::AlreadyExists
                || fs::symlink_metadata(destination).is_ok()
            {
                return Err(OkcError::OutputExists(destination.to_path_buf()));
            }
            return Err(OkcError::io(destination, source));
        }
    };
    if let Err(source) = hook.checkpoint(PackPublicationStep::Publish, destination) {
        return Err(OkcError::PublishedButDurabilityUncertain {
            path: destination.to_path_buf(),
            source,
        });
    }
    drop(published);
    if let Err(error) = sync_parent_after_publication(parent, destination, hook) {
        return Err(OkcError::PublishedButDurabilityUncertain {
            path: destination.to_path_buf(),
            source: error,
        });
    }
    Ok(())
}

#[cfg(not(feature = "archives"))]
pub fn create_pack(_compiled_vault: &Path, destination: &Path, _zstd_level: i32) -> Result<()> {
    Err(OkcError::UnsupportedSource(destination.to_path_buf()))
}

#[cfg(feature = "archives")]
pub(crate) fn preflight_pack_destination(compiled_vault: &Path, destination: &Path) -> Result<()> {
    validate_pack_extension(destination)?;
    reject_existing_pack_leaf(destination)?;
    validate_disjoint_paths(compiled_vault, destination)?;

    let parent = destination_parent(destination);
    fs::create_dir_all(parent).map_err(|error| OkcError::io(parent, error))?;

    // Creating a previously absent parent can expose a different canonical
    // ancestor than the first pass. Repeat both security checks before any
    // staging file is created.
    reject_existing_pack_leaf(destination)?;
    validate_disjoint_paths(compiled_vault, destination)
}

#[cfg(not(feature = "archives"))]
pub(crate) fn preflight_pack_destination(_compiled_vault: &Path, destination: &Path) -> Result<()> {
    Err(OkcError::UnsupportedSource(destination.to_path_buf()))
}

#[cfg(feature = "archives")]
fn validate_pack_extension(destination: &Path) -> Result<()> {
    if destination
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("okcpack"))
    {
        return Ok(());
    }
    Err(OkcError::InvalidConfig(format!(
        "OKCPack destination `{}` must use the .okcpack extension",
        destination.display()
    )))
}

#[cfg(feature = "archives")]
fn reject_existing_pack_leaf(destination: &Path) -> Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(OkcError::OutputExists(destination.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(OkcError::io(destination, error)),
    }
}

#[cfg(feature = "archives")]
fn require_compiled_vault_directory(compiled_vault: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(compiled_vault)
        .map_err(|error| OkcError::io(compiled_vault, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(OkcError::VerificationFailed(format!(
            "OKCPack input `{}` must be a non-symlink Compiled Vault directory",
            compiled_vault.display()
        )));
    }
    Ok(())
}

#[cfg(feature = "archives")]
fn validate_disjoint_paths(compiled_vault: &Path, destination: &Path) -> Result<()> {
    let compiled = resolve_existing_ancestor(compiled_vault)?;
    let pack = resolve_existing_ancestor(destination)?;
    let compiled_key = portable_host_path_key(&compiled)?;
    let pack_key = portable_host_path_key(&pack)?;
    if compiled_key == pack_key
        || compiled_key.starts_with(&pack_key)
        || pack_key.starts_with(&compiled_key)
    {
        return Err(OkcError::UnsafePath {
            path: destination.display().to_string(),
            reason: "OKCPack destination and Compiled Vault must be disjoint in both containment directions"
                .into(),
        });
    }
    Ok(())
}

#[cfg(feature = "archives")]
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
            Ok(_) => {
                let resolved =
                    fs::canonicalize(ancestor).map_err(|error| OkcError::io(ancestor, error))?;
                let suffix = absolute.strip_prefix(ancestor).map_err(|_| {
                    OkcError::Internal(
                        "existing destination ancestor was not a lexical prefix".into(),
                    )
                })?;
                return lexical_normalize(&resolved.join(suffix));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(OkcError::io(ancestor, error)),
        }
    }
    Err(OkcError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "destination has no resolvable existing ancestor",
        ),
    })
}

#[cfg(feature = "archives")]
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
                        reason: "host destination path traverses above its root".into(),
                    });
                }
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    Ok(normalized)
}

#[cfg(feature = "archives")]
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

#[cfg(feature = "archives")]
fn portable_component_key(component: &std::ffi::OsStr, path: &Path) -> Result<String> {
    let component = component.to_str().ok_or_else(|| OkcError::UnsafePath {
        path: path.display().to_string(),
        reason: "pack publication paths must be valid UTF-8 for portable comparison".into(),
    })?;
    Ok(crate::parse::full_casefold_nfc(component))
}

#[cfg(feature = "archives")]
fn destination_parent(destination: &Path) -> &Path {
    destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(feature = "archives")]
fn sync_parent_after_publication(
    parent: &Path,
    destination: &Path,
    hook: &impl PackPublicationHook,
) -> std::io::Result<()> {
    hook.checkpoint(PackPublicationStep::ParentSync, destination)?;
    #[cfg(unix)]
    {
        let directory = File::open(parent)?;
        directory.sync_all()?;
    }
    Ok(())
}

#[cfg(feature = "archives")]
fn sealed_zstd_level(compiled_vault: &Path) -> Result<i32> {
    let plan_path = compiled_vault.join(".okc/plan.json");
    let bytes = fs::read(&plan_path).map_err(|error| OkcError::io(&plan_path, error))?;
    let approved: crate::approval::ApprovedPlan =
        serde_json::from_slice(&bytes).map_err(|error| {
            OkcError::VerificationFailed(format!(
                "sealed approved plan is malformed while checking OKCPack profile: {error}"
            ))
        })?;
    Ok(approved.plan.policy.output.zstd_level)
}

#[cfg(feature = "archives")]
fn compare_pack_bytes(actual_path: &Path, expected_path: &Path) -> Result<PackObservation> {
    let actual = File::open(actual_path).map_err(|error| OkcError::io(actual_path, error))?;
    let expected = File::open(expected_path).map_err(|error| OkcError::io(expected_path, error))?;
    let mut actual = BufReader::new(actual);
    let mut expected = BufReader::new(expected);
    let mut actual_buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut expected_buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut raw_hasher = Sha256::new();
    let mut content_hasher = Sha256::new();
    content_hasher.update(b"okc:content:v2\0");
    let mut byte_len = 0_u64;

    loop {
        let actual_len = actual
            .read(&mut actual_buffer)
            .map_err(|error| OkcError::io(actual_path, error))?;
        let expected_len = expected
            .read(&mut expected_buffer)
            .map_err(|error| OkcError::io(expected_path, error))?;
        if actual_len != expected_len
            || actual_buffer[..actual_len] != expected_buffer[..expected_len]
        {
            return Err(OkcError::VerificationFailed(format!(
                "OKCPack bytes do not match deterministic profile `{PACK_PROFILE}`"
            )));
        }
        if actual_len == 0 {
            break;
        }
        let chunk = &actual_buffer[..actual_len];
        raw_hasher.update(chunk);
        content_hasher.update(chunk);
        byte_len = byte_len
            .checked_add(
                u64::try_from(actual_len)
                    .map_err(|_| OkcError::ResourceLimit("OKCPack chunk length overflow".into()))?,
            )
            .ok_or_else(|| OkcError::ResourceLimit("OKCPack length overflow".into()))?;
    }

    Ok(PackObservation {
        raw_sha256: hex::encode(raw_hasher.finalize()),
        content_hash: crate::identity::ContentHash::parse_hex(&hex::encode(
            content_hasher.finalize(),
        ))?,
        byte_len,
    })
}

/// Rebuild the pack with the sealed compiler profile and require byte-for-byte
/// equality before any provenance record may name that deterministic profile.
/// The caller must verify `compiled_vault` first; this function deliberately
/// avoids recursively invoking artifact verification.
#[cfg(feature = "archives")]
pub(crate) fn verify_canonical_pack(pack: &Path, compiled_vault: &Path) -> Result<PackObservation> {
    let zstd_level = sealed_zstd_level(compiled_vault)?;
    let temporary = tempfile::tempdir().map_err(|error| OkcError::io(pack, error))?;
    let expected = temporary.path().join("canonical.okcpack");
    write_pack_path_raw(compiled_vault, &expected, zstd_level)?;
    compare_pack_bytes(pack, &expected)
}

#[cfg(feature = "archives")]
#[derive(Clone, Copy)]
struct MemberStreamBudget {
    expected_bytes: u64,
    expanded_before: u64,
    max_total_bytes: u64,
    expansion_limit: u64,
}

#[cfg(feature = "archives")]
fn write_pack_member<R: Read>(
    reader: &mut R,
    output: &Path,
    pack: &Path,
    logical: &str,
    budget: MemberStreamBudget,
    buffer: &mut [u8],
) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(output)
        .map_err(|error| OkcError::io(output, error))?;
    let mut member_bytes = 0_u64;
    loop {
        let read = reader
            .read(buffer)
            .map_err(|error| OkcError::io(pack, error))?;
        if read == 0 {
            break;
        }
        member_bytes = member_bytes
            .checked_add(u64::try_from(read).map_err(|_| {
                OkcError::ResourceLimit("OKCPack member read length overflow".into())
            })?)
            .ok_or_else(|| OkcError::ResourceLimit("OKCPack member length overflow".into()))?;
        let streamed_total = budget
            .expanded_before
            .checked_add(member_bytes)
            .ok_or_else(|| {
                OkcError::ResourceLimit("OKCPack expanded byte length overflow".into())
            })?;
        if member_bytes > budget.expected_bytes
            || streamed_total > budget.max_total_bytes
            || streamed_total > budget.expansion_limit
        {
            return Err(OkcError::ResourceLimit(
                "OKCPack streamed content exceeds declared safety limits".into(),
            ));
        }
        file.write_all(&buffer[..read])
            .map_err(|error| OkcError::io(output, error))?;
    }
    if member_bytes != budget.expected_bytes {
        return Err(OkcError::MalformedInput {
            path: logical.into(),
            reason: format!(
                "OKCPack member length {member_bytes} does not match declared length {}",
                budget.expected_bytes
            ),
        });
    }
    Ok(())
}

#[cfg(feature = "archives")]
pub(crate) fn extract_pack_safely(pack: &Path, destination: &Path) -> Result<()> {
    let policy = crate::config::CompilerPolicy::default();
    let metadata = fs::symlink_metadata(pack).map_err(|error| OkcError::io(pack, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(OkcError::VerificationFailed(
            "OKCPack input must be a regular non-symlink file".into(),
        ));
    }
    let compressed_bytes = metadata.len();
    if compressed_bytes > policy.limits.max_file_bytes {
        return Err(OkcError::ResourceLimit(format!(
            "OKCPack exceeds outer compressed-file limit of {} bytes",
            policy.limits.max_file_bytes
        )));
    }
    let expansion_limit = compressed_bytes
        .max(1)
        .saturating_mul(policy.limits.max_archive_expansion_ratio);
    let file = File::open(pack).map_err(|error| OkcError::io(pack, error))?;
    let decoder = zstd::Decoder::new(file).map_err(|error| OkcError::io(pack, error))?;
    let mut archive = tar::Archive::new(decoder);
    let mut total = 0_u64;
    let mut count = 0_u64;
    let mut seen = BTreeSet::new();
    let mut copy_buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    for item in archive
        .entries()
        .map_err(|error| OkcError::io(pack, error))?
    {
        let mut item = item.map_err(|error| OkcError::io(pack, error))?;
        if !item.header().entry_type().is_file() {
            return Err(OkcError::VerificationFailed(
                "OKCPack may contain regular files only".into(),
            ));
        }
        let member = item.path().map_err(|error| OkcError::MalformedInput {
            path: pack.display().to_string(),
            reason: error.to_string(),
        })?;
        let logical = member
            .to_str()
            .ok_or_else(|| OkcError::MalformedInput {
                path: pack.display().to_string(),
                reason: "OKCPack member path is not valid UTF-8".into(),
            })?
            .to_owned();
        crate::snapshot::validate_output_logical_path(&logical, &policy)?;
        if !seen.insert(logical.clone()) {
            return Err(OkcError::VerificationFailed(format!(
                "OKCPack contains duplicate member `{logical}`"
            )));
        }
        let size = item.header().size().unwrap_or(u64::MAX);
        if size > policy.limits.max_file_bytes {
            return Err(OkcError::ResourceLimit(format!(
                "OKCPack member `{logical}` exceeds per-file limit"
            )));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| OkcError::ResourceLimit("OKCPack file count overflow".into()))?;
        let next_total = total.checked_add(size).ok_or_else(|| {
            OkcError::ResourceLimit("OKCPack expanded byte length overflow".into())
        })?;
        if count > policy.limits.max_files || next_total > policy.limits.max_total_bytes {
            return Err(OkcError::ResourceLimit(
                "OKCPack exceeds extraction limits".into(),
            ));
        }
        if next_total > expansion_limit {
            return Err(OkcError::ResourceLimit(
                "OKCPack expansion ratio exceeds configured maximum".into(),
            ));
        }
        let output = destination.join(&logical);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| OkcError::io(parent, error))?;
        }
        write_pack_member(
            &mut item,
            &output,
            pack,
            &logical,
            MemberStreamBudget {
                expected_bytes: size,
                expanded_before: total,
                max_total_bytes: policy.limits.max_total_bytes,
                expansion_limit,
            },
            &mut copy_buffer,
        )?;
        total = next_total;
    }
    Ok(())
}

#[cfg(all(test, feature = "archives"))]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{PackPublicationHook, PackPublicationStep, create_pack_with_hook};
    use crate::{OkcCompiler, OkcError, SourceSpec};

    struct RecordingHook {
        steps: RefCell<Vec<PackPublicationStep>>,
        fail_at: Option<PackPublicationStep>,
    }

    impl RecordingHook {
        fn new(fail_at: Option<PackPublicationStep>) -> Self {
            Self {
                steps: RefCell::new(Vec::new()),
                fail_at,
            }
        }

        fn observed(&self) -> Vec<PackPublicationStep> {
            self.steps.borrow().clone()
        }
    }

    impl PackPublicationHook for RecordingHook {
        fn checkpoint(&self, step: PackPublicationStep, _path: &Path) -> std::io::Result<()> {
            self.steps.borrow_mut().push(step);
            if self.fail_at == Some(step) {
                return Err(std::io::Error::other(format!(
                    "injected OKCPack publication failure at {step:?}"
                )));
            }
            Ok(())
        }
    }

    fn compile_fixture(parent: &Path) -> (OkcCompiler, PathBuf) {
        let sdk = OkcCompiler::builder()
            .build()
            .expect("build pack publication test compiler");
        let source =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/basic_vault");
        let inspection = sdk
            .inspect([SourceSpec::directory("pack-publication-unit", source)
                .expect("pack publication unit source")])
            .expect("inspect pack publication unit source");
        let plan = sdk
            .plan(&inspection)
            .expect("plan pack publication unit source");
        let approved = sdk
            .approve_without_augmentation(plan)
            .expect("approve pack publication unit source");
        let compiled_vault = parent.join("compiled");
        sdk.compile(&approved, &compiled_vault)
            .expect("compile pack publication unit source");
        (sdk, compiled_vault)
    }

    fn entry_names(parent: &Path) -> BTreeSet<OsString> {
        fs::read_dir(parent)
            .expect("read pack publication test parent")
            .map(|entry| entry.expect("pack publication test entry").file_name())
            .collect()
    }

    #[test]
    fn pack_publication_checkpoints_follow_the_normative_order() {
        let temporary = tempfile::tempdir().expect("pack checkpoint temporary parent");
        let (sdk, compiled_vault) = compile_fixture(temporary.path());
        let destination = temporary.path().join("ordered.okcpack");
        let hook = RecordingHook::new(None);

        create_pack_with_hook(
            &compiled_vault,
            &destination,
            sdk.policy().output.zstd_level,
            &hook,
        )
        .expect("publish pack through every checkpoint");

        assert_eq!(
            hook.observed(),
            vec![
                PackPublicationStep::Write,
                PackPublicationStep::Finish,
                PackPublicationStep::Flush,
                PackPublicationStep::FileSync,
                PackPublicationStep::StagedVerify,
                PackPublicationStep::Publish,
                PackPublicationStep::ParentSync,
            ]
        );
        assert!(
            sdk.verify(&destination)
                .expect("verify checkpoint pack")
                .valid
        );
    }

    #[test]
    fn precommit_pack_faults_leave_no_destination_or_sibling_stage() {
        let temporary = tempfile::tempdir().expect("precommit fault temporary parent");
        let (sdk, compiled_vault) = compile_fixture(temporary.path());
        let normative_order = [
            PackPublicationStep::Write,
            PackPublicationStep::Finish,
            PackPublicationStep::Flush,
            PackPublicationStep::FileSync,
            PackPublicationStep::StagedVerify,
        ];

        for (index, fail_at) in normative_order.iter().copied().enumerate() {
            let destination = temporary.path().join(format!("precommit-{index}.okcpack"));
            let entries_before = entry_names(temporary.path());
            let hook = RecordingHook::new(Some(fail_at));
            let error = create_pack_with_hook(
                &compiled_vault,
                &destination,
                sdk.policy().output.zstd_level,
                &hook,
            )
            .expect_err("injected precommit fault must fail");

            assert!(matches!(error, OkcError::Io { .. }));
            assert!(fs::symlink_metadata(&destination).is_err());
            assert_eq!(entry_names(temporary.path()), entries_before);
            assert_eq!(hook.observed(), normative_order[..=index]);
        }
    }

    #[test]
    fn postcommit_pack_faults_retain_a_complete_published_pack() {
        let temporary = tempfile::tempdir().expect("postcommit fault temporary parent");
        let (sdk, compiled_vault) = compile_fixture(temporary.path());

        for (index, fail_at) in [
            PackPublicationStep::Publish,
            PackPublicationStep::ParentSync,
        ]
        .into_iter()
        .enumerate()
        {
            let destination = temporary.path().join(format!("postcommit-{index}.okcpack"));
            let entries_before = entry_names(temporary.path());
            let hook = RecordingHook::new(Some(fail_at));
            let error = create_pack_with_hook(
                &compiled_vault,
                &destination,
                sdk.policy().output.zstd_level,
                &hook,
            )
            .expect_err("injected postcommit fault must report uncertain durability");

            assert!(matches!(
                error,
                OkcError::PublishedButDurabilityUncertain { ref path, .. }
                    if path == &destination
            ));
            assert!(
                sdk.verify(&destination)
                    .expect("verify retained postcommit pack")
                    .valid
            );
            let mut expected_entries = entries_before;
            expected_entries.insert(
                destination
                    .file_name()
                    .expect("postcommit destination file name")
                    .to_os_string(),
            );
            assert_eq!(entry_names(temporary.path()), expected_entries);
        }
    }
}
