use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

#[cfg(feature = "archives")]
use std::collections::BTreeSet;

use crate::error::{Result, VaultcError};

#[cfg(feature = "archives")]
pub fn create_pack(compiled_vault: &Path, destination: &Path, zstd_level: i32) -> Result<()> {
    if destination.exists() {
        return Err(VaultcError::OutputExists(destination.to_path_buf()));
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
pub(crate) fn extract_pack_safely(pack: &Path, destination: &Path) -> Result<()> {
    let file = File::open(pack).map_err(|error| VaultcError::io(pack, error))?;
    let decoder = zstd::Decoder::new(file).map_err(|error| VaultcError::io(pack, error))?;
    let mut archive = tar::Archive::new(decoder);
    let policy = crate::config::CompilerPolicy::default();
    let mut total = 0_u64;
    let mut count = 0_u64;
    let mut seen = BTreeSet::new();
    for item in archive
        .entries()
        .map_err(|error| VaultcError::io(pack, error))?
    {
        let item = item.map_err(|error| VaultcError::io(pack, error))?;
        if !item.header().entry_type().is_file() {
            return Err(VaultcError::VerificationFailed(
                "VaultPack may contain regular files only".into(),
            ));
        }
        let member = item.path().map_err(|error| VaultcError::MalformedInput {
            path: pack.display().to_string(),
            reason: error.to_string(),
        })?;
        let logical = member.to_str().ok_or_else(|| VaultcError::MalformedInput {
            path: pack.display().to_string(),
            reason: "VaultPack member path is not valid UTF-8".into(),
        })?;
        crate::snapshot::validate_output_logical_path(logical, &policy)?;
        if !seen.insert(logical.to_owned()) {
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
        count = count.saturating_add(1);
        total = total.saturating_add(size);
        if count > policy.limits.max_files || total > policy.limits.max_total_bytes {
            return Err(VaultcError::ResourceLimit(
                "VaultPack exceeds extraction limits".into(),
            ));
        }
        let output = destination.join(logical);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| VaultcError::io(parent, error))?;
        }
        let mut bytes = Vec::new();
        item.take(policy.limits.max_file_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| VaultcError::io(pack, error))?;
        if bytes.len() as u64 > policy.limits.max_file_bytes {
            return Err(VaultcError::ResourceLimit(
                "VaultPack member exceeds per-file limit".into(),
            ));
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options
            .open(&output)
            .map_err(|error| VaultcError::io(&output, error))?;
        file.write_all(&bytes)
            .map_err(|error| VaultcError::io(&output, error))?;
    }
    Ok(())
}
