use std::path::Path;

#[cfg(feature = "archives")]
use sha2::{Digest as _, Sha256};
#[cfg(feature = "archives")]
use std::collections::BTreeSet;
#[cfg(feature = "archives")]
use std::fs::{self, File};
#[cfg(feature = "archives")]
use std::io::{BufReader, Read, Write};

use crate::error::{Result, VaultcError};

pub(crate) const PACK_PROFILE: &str = "vaultc-tar-zstd-deterministic-v1";

#[cfg(feature = "archives")]
pub(crate) struct PackObservation {
    pub raw_sha256: String,
    pub content_hash: crate::identity::ContentHash,
    pub byte_len: u64,
}

#[cfg(feature = "archives")]
pub fn create_pack(compiled_vault: &Path, destination: &Path, zstd_level: i32) -> Result<()> {
    if destination.exists() {
        return Err(VaultcError::OutputExists(destination.to_path_buf()));
    }
    let sealed_level = sealed_zstd_level(compiled_vault)?;
    if zstd_level != sealed_level {
        return Err(VaultcError::InvalidConfig(format!(
            "VaultPack zstd level {zstd_level} disagrees with sealed plan level {sealed_level}"
        )));
    }
    let files = crate::compile::inventory(compiled_vault, &[])?;
    let output = File::create(destination).map_err(|error| VaultcError::io(destination, error))?;
    let encoder = zstd::Encoder::new(output, zstd_level)
        .map_err(|error| VaultcError::io(destination, error))?;
    let mut archive = tar::Builder::new(encoder);
    archive.mode(tar::HeaderMode::Deterministic);
    for file in files {
        let path = compiled_vault.join(&file.path);
        let mut input = File::open(&path).map_err(|error| VaultcError::io(&path, error))?;
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
            .map_err(|error| VaultcError::io(destination, error))?;
    }
    let encoder = archive
        .into_inner()
        .map_err(|error| VaultcError::io(destination, error))?;
    let mut output = encoder
        .finish()
        .map_err(|error| VaultcError::io(destination, error))?;
    output
        .flush()
        .map_err(|error| VaultcError::io(destination, error))?;
    output
        .sync_all()
        .map_err(|error| VaultcError::io(destination, error))?;
    Ok(())
}

#[cfg(not(feature = "archives"))]
pub fn create_pack(_compiled_vault: &Path, destination: &Path, _zstd_level: i32) -> Result<()> {
    Err(VaultcError::UnsupportedSource(destination.to_path_buf()))
}

#[cfg(feature = "archives")]
fn sealed_zstd_level(compiled_vault: &Path) -> Result<i32> {
    let plan_path = compiled_vault.join(".vaultc/plan.json");
    let bytes = fs::read(&plan_path).map_err(|error| VaultcError::io(&plan_path, error))?;
    let approved: crate::approval::ApprovedPlan =
        serde_json::from_slice(&bytes).map_err(|error| {
            VaultcError::VerificationFailed(format!(
                "sealed approved plan is malformed while checking VaultPack profile: {error}"
            ))
        })?;
    Ok(approved.plan.policy.output.zstd_level)
}

#[cfg(feature = "archives")]
fn compare_pack_bytes(actual_path: &Path, expected_path: &Path) -> Result<PackObservation> {
    let actual = File::open(actual_path).map_err(|error| VaultcError::io(actual_path, error))?;
    let expected =
        File::open(expected_path).map_err(|error| VaultcError::io(expected_path, error))?;
    let mut actual = BufReader::new(actual);
    let mut expected = BufReader::new(expected);
    let mut actual_buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut expected_buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut raw_hasher = Sha256::new();
    let mut content_hasher = Sha256::new();
    content_hasher.update(b"vaultc:content:v1\0");
    let mut byte_len = 0_u64;

    loop {
        let actual_len = actual
            .read(&mut actual_buffer)
            .map_err(|error| VaultcError::io(actual_path, error))?;
        let expected_len = expected
            .read(&mut expected_buffer)
            .map_err(|error| VaultcError::io(expected_path, error))?;
        if actual_len != expected_len
            || actual_buffer[..actual_len] != expected_buffer[..expected_len]
        {
            return Err(VaultcError::VerificationFailed(format!(
                "VaultPack bytes do not match deterministic profile `{PACK_PROFILE}`"
            )));
        }
        if actual_len == 0 {
            break;
        }
        let chunk = &actual_buffer[..actual_len];
        raw_hasher.update(chunk);
        content_hasher.update(chunk);
        byte_len = byte_len
            .checked_add(u64::try_from(actual_len).map_err(|_| {
                VaultcError::ResourceLimit("VaultPack chunk length overflow".into())
            })?)
            .ok_or_else(|| VaultcError::ResourceLimit("VaultPack length overflow".into()))?;
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
    let temporary = tempfile::tempdir().map_err(|error| VaultcError::io(pack, error))?;
    let expected = temporary.path().join("canonical.vaultpack");
    create_pack(compiled_vault, &expected, zstd_level)?;
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
        .map_err(|error| VaultcError::io(output, error))?;
    let mut member_bytes = 0_u64;
    loop {
        let read = reader
            .read(buffer)
            .map_err(|error| VaultcError::io(pack, error))?;
        if read == 0 {
            break;
        }
        member_bytes = member_bytes
            .checked_add(u64::try_from(read).map_err(|_| {
                VaultcError::ResourceLimit("VaultPack member read length overflow".into())
            })?)
            .ok_or_else(|| VaultcError::ResourceLimit("VaultPack member length overflow".into()))?;
        let streamed_total = budget
            .expanded_before
            .checked_add(member_bytes)
            .ok_or_else(|| {
                VaultcError::ResourceLimit("VaultPack expanded byte length overflow".into())
            })?;
        if member_bytes > budget.expected_bytes
            || streamed_total > budget.max_total_bytes
            || streamed_total > budget.expansion_limit
        {
            return Err(VaultcError::ResourceLimit(
                "VaultPack streamed content exceeds declared safety limits".into(),
            ));
        }
        file.write_all(&buffer[..read])
            .map_err(|error| VaultcError::io(output, error))?;
    }
    if member_bytes != budget.expected_bytes {
        return Err(VaultcError::MalformedInput {
            path: logical.into(),
            reason: format!(
                "VaultPack member length {member_bytes} does not match declared length {}",
                budget.expected_bytes
            ),
        });
    }
    Ok(())
}

#[cfg(feature = "archives")]
pub(crate) fn extract_pack_safely(pack: &Path, destination: &Path) -> Result<()> {
    let policy = crate::config::CompilerPolicy::default();
    let metadata = fs::symlink_metadata(pack).map_err(|error| VaultcError::io(pack, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(VaultcError::VerificationFailed(
            "VaultPack input must be a regular non-symlink file".into(),
        ));
    }
    let compressed_bytes = metadata.len();
    if compressed_bytes > policy.limits.max_file_bytes {
        return Err(VaultcError::ResourceLimit(format!(
            "VaultPack exceeds outer compressed-file limit of {} bytes",
            policy.limits.max_file_bytes
        )));
    }
    let expansion_limit = compressed_bytes
        .max(1)
        .saturating_mul(policy.limits.max_archive_expansion_ratio);
    let file = File::open(pack).map_err(|error| VaultcError::io(pack, error))?;
    let decoder = zstd::Decoder::new(file).map_err(|error| VaultcError::io(pack, error))?;
    let mut archive = tar::Archive::new(decoder);
    let mut total = 0_u64;
    let mut count = 0_u64;
    let mut seen = BTreeSet::new();
    let mut copy_buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    for item in archive
        .entries()
        .map_err(|error| VaultcError::io(pack, error))?
    {
        let mut item = item.map_err(|error| VaultcError::io(pack, error))?;
        if !item.header().entry_type().is_file() {
            return Err(VaultcError::VerificationFailed(
                "VaultPack may contain regular files only".into(),
            ));
        }
        let member = item.path().map_err(|error| VaultcError::MalformedInput {
            path: pack.display().to_string(),
            reason: error.to_string(),
        })?;
        let logical = member
            .to_str()
            .ok_or_else(|| VaultcError::MalformedInput {
                path: pack.display().to_string(),
                reason: "VaultPack member path is not valid UTF-8".into(),
            })?
            .to_owned();
        crate::snapshot::validate_output_logical_path(&logical, &policy)?;
        if !seen.insert(logical.clone()) {
            return Err(VaultcError::VerificationFailed(format!(
                "VaultPack contains duplicate member `{logical}`"
            )));
        }
        let size = item.header().size().unwrap_or(u64::MAX);
        if size > policy.limits.max_file_bytes {
            return Err(VaultcError::ResourceLimit(format!(
                "VaultPack member `{logical}` exceeds per-file limit"
            )));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| VaultcError::ResourceLimit("VaultPack file count overflow".into()))?;
        let next_total = total.checked_add(size).ok_or_else(|| {
            VaultcError::ResourceLimit("VaultPack expanded byte length overflow".into())
        })?;
        if count > policy.limits.max_files || next_total > policy.limits.max_total_bytes {
            return Err(VaultcError::ResourceLimit(
                "VaultPack exceeds extraction limits".into(),
            ));
        }
        if next_total > expansion_limit {
            return Err(VaultcError::ResourceLimit(
                "VaultPack expansion ratio exceeds configured maximum".into(),
            ));
        }
        let output = destination.join(&logical);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| VaultcError::io(parent, error))?;
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
