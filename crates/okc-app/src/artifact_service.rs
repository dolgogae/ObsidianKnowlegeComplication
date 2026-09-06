//! Version-detecting, read-only artifact application service.
//!
//! This module is the single dispatch boundary used by the CLI and language
//! bindings. Detection happens before a version-specific decoder is selected;
//! mixed or unknown families fail closed.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::Path;

use okc_core::integration::{
    V3Manifest, V3ProvenanceRecord, explain_v3_directory, verify_v3_directory,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, Result};

const MANIFEST_HEADER_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFamily {
    V1,
    V2,
    V3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactVerification {
    V1(okc_legacy_v1::VerificationReport),
    V2(okc_core::VerificationReport),
    V3(V3Manifest),
}

impl ArtifactVerification {
    pub const fn family(&self) -> ArtifactFamily {
        match self {
            Self::V1(_) => ArtifactFamily::V1,
            Self::V2(_) => ArtifactFamily::V2,
            Self::V3(_) => ArtifactFamily::V3,
        }
    }

    pub fn payload_json(&self) -> Result<serde_json::Value> {
        match self {
            Self::V1(value) => Ok(serde_json::to_value(value)?),
            Self::V2(value) => Ok(serde_json::to_value(value)?),
            Self::V3(value) => Ok(serde_json::to_value(value)?),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactExplanation {
    V1(okc_legacy_v1::ProvenancePage),
    V2(okc_core::ProvenancePage),
    V3(V3ProvenanceRecord),
}

impl ArtifactExplanation {
    pub const fn family(&self) -> ArtifactFamily {
        match self {
            Self::V1(_) => ArtifactFamily::V1,
            Self::V2(_) => ArtifactFamily::V2,
            Self::V3(_) => ArtifactFamily::V3,
        }
    }

    pub fn payload_json(&self) -> Result<serde_json::Value> {
        match self {
            Self::V1(value) => Ok(serde_json::to_value(value)?),
            Self::V2(value) => Ok(serde_json::to_value(value)?),
            Self::V3(value) => Ok(serde_json::to_value(value)?),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ArtifactService;

impl ArtifactService {
    pub fn detect(&self, artifact: impl AsRef<Path>) -> Result<ArtifactFamily> {
        detect_family(artifact.as_ref())
    }

    pub fn verify(&self, artifact: impl AsRef<Path>) -> Result<ArtifactVerification> {
        let artifact = artifact.as_ref();
        match detect_family(artifact)? {
            ArtifactFamily::V1 => {
                let compiler =
                    okc_legacy_v1::VaultCompiler::builder()
                        .build()
                        .map_err(|error| {
                            AppError::Artifact(format!("could not initialize V1 reader: {error}"))
                        })?;
                let mut report = compiler
                    .verify(artifact)
                    .map_err(|error| AppError::Artifact(error.to_string()))?;
                // Pack verification uses a private extraction path. Never leak it.
                report.artifact_path = artifact.to_path_buf();
                Ok(ArtifactVerification::V1(report))
            }
            ArtifactFamily::V2 => {
                let reader = okc_legacy_v2::LegacyV2Reader::new()?;
                let mut report = reader
                    .verify(artifact)
                    .map_err(|error| AppError::Artifact(error.to_string()))?;
                report.artifact_path = artifact.to_path_buf();
                Ok(ArtifactVerification::V2(report))
            }
            ArtifactFamily::V3 => Ok(ArtifactVerification::V3(
                verify_v3_directory(artifact)
                    .map_err(|error| AppError::Artifact(error.to_string()))?,
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn explain(
        &self,
        artifact: impl AsRef<Path>,
        output_path: Option<&str>,
        package: bool,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<ArtifactExplanation> {
        let artifact = artifact.as_ref();
        match detect_family(artifact)? {
            ArtifactFamily::V1 => {
                let compiler =
                    okc_legacy_v1::VaultCompiler::builder()
                        .build()
                        .map_err(|error| {
                            AppError::Artifact(format!("could not initialize V1 reader: {error}"))
                        })?;
                let mut query = if package {
                    okc_legacy_v1::ProvenanceQuery::package()
                } else {
                    okc_legacy_v1::ProvenanceQuery::artifact_path(required_output(output_path)?)
                };
                query = query
                    .with_limit(limit)
                    .map_err(|error| AppError::Artifact(error.to_string()))?;
                if let Some(cursor) = cursor {
                    query = query.with_cursor(cursor.to_owned());
                }
                Ok(ArtifactExplanation::V1(
                    compiler
                        .explain_provenance_page(artifact, &query)
                        .map_err(|error| AppError::Artifact(error.to_string()))?,
                ))
            }
            ArtifactFamily::V2 => {
                let reader = okc_legacy_v2::LegacyV2Reader::new()?;
                let mut query = if package {
                    okc_core::ProvenanceQuery::package()
                } else {
                    okc_core::ProvenanceQuery::artifact_path(required_output(output_path)?)
                };
                query = query.with_limit(limit)?;
                if let Some(cursor) = cursor {
                    query = query.with_cursor(cursor.to_owned());
                }
                Ok(ArtifactExplanation::V2(
                    reader
                        .explain(artifact, &query)
                        .map_err(|error| AppError::Artifact(error.to_string()))?,
                ))
            }
            ArtifactFamily::V3 => {
                if package || cursor.is_some() {
                    return Err(AppError::Artifact(
                        "V3 directory explanation requires one output path and has no Pack cursor"
                            .into(),
                    ));
                }
                Ok(ArtifactExplanation::V3(
                    explain_v3_directory(artifact, required_output(output_path)?.as_str())
                        .map_err(|error| AppError::Artifact(error.to_string()))?,
                ))
            }
        }
    }
}

fn required_output(output_path: Option<&str>) -> Result<String> {
    output_path
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::Artifact("provenance explanation requires an output path".into()))
}

#[derive(Deserialize)]
struct ManifestHeader {
    format_family: String,
    schema_version: u32,
}

fn detect_family(path: &Path) -> Result<ArtifactFamily> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::Artifact(format!(
            "artifact path must not be a symlink: {}",
            path.display()
        )));
    }
    if metadata.is_file() {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("vaultpack") {
            return Ok(ArtifactFamily::V1);
        }
        if extension.eq_ignore_ascii_case("okcpack") {
            // V3 Pack publication is not implemented; every supported
            // `.okcpack` is therefore the frozen schema-2 profile.
            return Ok(ArtifactFamily::V2);
        }
        return Err(AppError::Artifact(
            "artifact file must end in .vaultpack or .okcpack".into(),
        ));
    }
    if !metadata.is_dir() {
        return Err(AppError::Artifact(
            "artifact must be a regular file or directory".into(),
        ));
    }
    let okc_manifest = path.join(".okc/manifest.json");
    let has_v1 = has_safe_manifest_marker(path, ".vaultc")?;
    let has_okc = has_safe_manifest_marker(path, ".okc")?;
    if has_v1 && has_okc {
        return Err(AppError::Artifact(
            "artifact contains mixed V1 and V2/V3 manifest families".into(),
        ));
    }
    if has_v1 {
        return Ok(ArtifactFamily::V1);
    }
    if !has_okc {
        return Err(AppError::Artifact(
            "artifact directory has no recognized manifest".into(),
        ));
    }
    let header: ManifestHeader = read_manifest_header(&okc_manifest)?;
    if header.format_family != "okc" {
        return Err(AppError::Artifact(format!(
            "unsupported artifact format family `{}`",
            header.format_family
        )));
    }
    match header.schema_version {
        2 => Ok(ArtifactFamily::V2),
        3 => Ok(ArtifactFamily::V3),
        version => Err(AppError::Artifact(format!(
            "unsupported OKC artifact schema {version}"
        ))),
    }
}

fn has_safe_manifest_marker(root: &Path, directory_name: &str) -> Result<bool> {
    let directory = root.join(directory_name);
    let directory_metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(AppError::Artifact(format!(
            "artifact marker must be a regular non-symlink directory: {}",
            directory.display()
        )));
    }
    let manifest = directory.join("manifest.json");
    let manifest_metadata = match fs::symlink_metadata(&manifest) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
        return Err(AppError::Artifact(format!(
            "artifact manifest must be a regular non-symlink file: {}",
            manifest.display()
        )));
    }
    Ok(true)
}

fn read_manifest_header(path: &Path) -> Result<ManifestHeader> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::Artifact(format!(
            "artifact manifest must be a regular non-symlink file: {}",
            path.display()
        )));
    }
    if metadata.len() > MANIFEST_HEADER_LIMIT {
        return Err(AppError::Artifact(
            "artifact manifest exceeds the detection size limit".into(),
        ));
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| {
        AppError::Artifact("artifact manifest size is unsupported on this platform".into())
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    File::open(path)?
        .take(MANIFEST_HEADER_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MANIFEST_HEADER_LIMIT {
        return Err(AppError::Artifact(
            "artifact manifest exceeds the detection size limit".into(),
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        AppError::Artifact(format!(
            "artifact manifest is invalid JSON ({:?})",
            error.classify()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_is_explicit_and_rejects_mixed_or_unknown_families() {
        let temporary = tempfile::tempdir().expect("temporary");
        let root = temporary.path().join("artifact");
        fs::create_dir_all(root.join(".okc")).expect("OKC metadata");
        fs::write(
            root.join(".okc/manifest.json"),
            br#"{"format_family":"okc","schema_version":3}"#,
        )
        .expect("manifest");
        assert_eq!(
            ArtifactService.detect(&root).expect("V3 detection"),
            ArtifactFamily::V3
        );
        fs::create_dir_all(root.join(".vaultc")).expect("legacy metadata");
        fs::write(root.join(".vaultc/manifest.json"), b"{}").expect("legacy manifest");
        assert!(ArtifactService.detect(&root).is_err());

        fs::remove_dir_all(root.join(".vaultc")).expect("remove legacy marker");
        fs::remove_file(root.join(".okc/manifest.json")).expect("remove manifest");
        fs::create_dir(root.join(".okc/manifest.json")).expect("directory manifest");
        assert!(ArtifactService.detect(&root).is_err());

        let unknown = temporary.path().join("unknown.okcpack.bad");
        fs::write(&unknown, b"not an artifact").expect("unknown");
        assert!(ArtifactService.detect(&unknown).is_err());
    }

    #[test]
    fn invalid_manifest_json_is_an_artifact_failure() {
        let temporary = tempfile::tempdir().expect("temporary");
        let root = temporary.path().join("artifact");
        fs::create_dir_all(root.join(".okc")).expect("OKC metadata");
        fs::write(root.join(".okc/manifest.json"), b"{not-json").expect("manifest");

        let error = ArtifactService.detect(&root).expect_err("invalid JSON");
        assert!(matches!(
            error,
            AppError::Artifact(message) if message.starts_with("artifact manifest is invalid JSON")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn detection_rejects_symlinked_marker_directories() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary");
        let artifact = temporary.path().join("artifact");
        let external = temporary.path().join("external");
        fs::create_dir(&artifact).expect("artifact");
        fs::create_dir(&external).expect("external");
        fs::write(
            external.join("manifest.json"),
            br#"{"format_family":"okc","schema_version":3}"#,
        )
        .expect("manifest");
        symlink(&external, artifact.join(".okc")).expect("marker symlink");
        assert!(ArtifactService.detect(&artifact).is_err());
    }
}
