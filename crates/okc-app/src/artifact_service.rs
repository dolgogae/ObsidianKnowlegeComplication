//! Current-schema artifact verification and provenance explanation.
//!
//! Detection happens before decoding. Recognizable retired artifacts receive
//! an explicit unsupported-schema error; mixed, malformed, symlinked, or
//! unknown inputs continue to fail closed as verification errors.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::Path;

use okc_core::integration::{
    CompiledVaultManifest, MAX_MANIFEST_BYTES, ProvenanceRecord, explain, verify,
};
use serde::Deserialize;

use crate::{AppError, Result};

const MANIFEST_HEADER_LIMIT: u64 = MAX_MANIFEST_BYTES;
const SUPPORTED_ARTIFACT_SCHEMA: u32 = 3;

#[derive(Debug, Clone, Copy, Default)]
pub struct ArtifactService;

impl ArtifactService {
    pub fn verify(&self, artifact: impl AsRef<Path>) -> Result<CompiledVaultManifest> {
        let artifact = artifact.as_ref();
        require_current_schema(artifact)?;
        verify(artifact).map_err(|error| AppError::Artifact(error.to_string()))
    }

    pub fn explain(
        &self,
        artifact: impl AsRef<Path>,
        output_path: &str,
    ) -> Result<ProvenanceRecord> {
        let artifact = artifact.as_ref();
        require_current_schema(artifact)?;
        if output_path.is_empty() {
            return Err(AppError::Artifact(
                "provenance explanation requires an output path".into(),
            ));
        }
        explain(artifact, output_path).map_err(|error| AppError::Artifact(error.to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct ManifestHeader {
    format_family: String,
    schema_version: u32,
}

fn require_current_schema(path: &Path) -> Result<()> {
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
            return Err(unsupported_schema("vaultc", 1));
        }
        if extension.eq_ignore_ascii_case("okcpack") {
            return Err(unsupported_schema("okc", 2));
        }
        return Err(AppError::Artifact(
            "artifact must be a Schema 3 Compiled Vault directory".into(),
        ));
    }
    if !metadata.is_dir() {
        return Err(AppError::Artifact(
            "artifact must be a regular directory".into(),
        ));
    }

    let has_retired_marker = has_safe_manifest_marker(path, ".vaultc")?;
    let has_current_marker = has_safe_manifest_marker(path, ".okc")?;
    if has_retired_marker && has_current_marker {
        return Err(AppError::Artifact(
            "artifact contains mixed manifest families".into(),
        ));
    }
    if has_retired_marker {
        return Err(unsupported_schema("vaultc", 1));
    }
    if !has_current_marker {
        return Err(AppError::Artifact(
            "artifact directory has no recognized manifest".into(),
        ));
    }

    let header = read_manifest_header(&path.join(".okc/manifest.json"))?;
    if header.format_family != "okc" {
        return Err(AppError::Artifact(format!(
            "unsupported artifact format family `{}`",
            header.format_family
        )));
    }
    match header.schema_version {
        SUPPORTED_ARTIFACT_SCHEMA => Ok(()),
        1 | 2 => Err(unsupported_schema("okc", header.schema_version)),
        schema => Err(AppError::Artifact(format!(
            "unsupported OKC artifact schema {schema}"
        ))),
    }
}

fn unsupported_schema(format_family: &str, detected_schema: u32) -> AppError {
    AppError::ArtifactSchemaUnsupported {
        supported_schema: SUPPORTED_ARTIFACT_SCHEMA,
        detected_schema,
        detected_format_family: format_family.into(),
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

    fn write_marker(root: &Path, directory: &str, manifest: &[u8]) {
        fs::create_dir_all(root.join(directory)).expect("artifact marker");
        fs::write(root.join(directory).join("manifest.json"), manifest).expect("manifest");
    }

    #[test]
    fn current_schema_is_accepted_and_mixed_or_unknown_inputs_fail_closed() {
        let temporary = tempfile::tempdir().expect("temporary");
        let root = temporary.path().join("artifact");
        write_marker(
            &root,
            ".okc",
            br#"{"format_family":"okc","schema_version":3,"artifact_id":"artifact_test"}"#,
        );
        require_current_schema(&root).expect("current-schema marker");

        write_marker(&root, ".vaultc", b"{}");
        assert!(matches!(
            require_current_schema(&root),
            Err(AppError::Artifact(message)) if message.contains("mixed")
        ));

        let unknown = temporary.path().join("unknown.bin");
        fs::write(&unknown, b"not an artifact").expect("unknown");
        assert!(matches!(
            require_current_schema(&unknown),
            Err(AppError::Artifact(_))
        ));
    }

    #[test]
    fn recognizable_retired_schemas_have_explicit_unsupported_details() {
        let temporary = tempfile::tempdir().expect("temporary");
        let schema_one = temporary.path().join("schema-one");
        write_marker(&schema_one, ".vaultc", b"{}");
        assert!(matches!(
            require_current_schema(&schema_one),
            Err(AppError::ArtifactSchemaUnsupported {
                supported_schema: 3,
                detected_schema: 1,
                ref detected_format_family,
            }) if detected_format_family == "vaultc"
        ));

        let schema_two = temporary.path().join("schema-two");
        write_marker(
            &schema_two,
            ".okc",
            br#"{"format_family":"okc","schema_version":2}"#,
        );
        assert!(matches!(
            require_current_schema(&schema_two),
            Err(AppError::ArtifactSchemaUnsupported {
                supported_schema: 3,
                detected_schema: 2,
                ref detected_format_family,
            }) if detected_format_family == "okc"
        ));

        let schema_one_pack = temporary.path().join("retired.vaultpack");
        fs::write(&schema_one_pack, b"marker").expect("schema one pack");
        assert!(matches!(
            require_current_schema(&schema_one_pack),
            Err(AppError::ArtifactSchemaUnsupported {
                supported_schema: 3,
                detected_schema: 1,
                ..
            })
        ));

        let schema_two_pack = temporary.path().join("retired.okcpack");
        fs::write(&schema_two_pack, b"marker").expect("schema two pack");
        assert!(matches!(
            require_current_schema(&schema_two_pack),
            Err(AppError::ArtifactSchemaUnsupported {
                supported_schema: 3,
                detected_schema: 2,
                ..
            })
        ));
    }

    #[test]
    fn malformed_and_oversized_manifests_fail_closed() {
        let temporary = tempfile::tempdir().expect("temporary");
        let malformed = temporary.path().join("malformed");
        write_marker(&malformed, ".okc", b"{not-json");
        assert!(matches!(
            require_current_schema(&malformed),
            Err(AppError::Artifact(message)) if message.starts_with("artifact manifest is invalid JSON")
        ));

        let oversized = temporary.path().join("oversized");
        write_marker(
            &oversized,
            ".okc",
            &vec![b' '; usize::try_from(MANIFEST_HEADER_LIMIT + 1).expect("limit")],
        );
        assert!(matches!(
            require_current_schema(&oversized),
            Err(AppError::Artifact(message)) if message.contains("size limit")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_roots_markers_and_manifests_are_rejected() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary");
        let external = temporary.path().join("external");
        write_marker(
            &external,
            ".okc",
            br#"{"format_family":"okc","schema_version":3}"#,
        );
        let root_alias = temporary.path().join("root-alias");
        symlink(&external, &root_alias).expect("root symlink");
        assert!(require_current_schema(&root_alias).is_err());

        let marker_alias = temporary.path().join("marker-alias");
        fs::create_dir(&marker_alias).expect("artifact");
        symlink(external.join(".okc"), marker_alias.join(".okc")).expect("marker symlink");
        assert!(require_current_schema(&marker_alias).is_err());

        let manifest_alias = temporary.path().join("manifest-alias");
        fs::create_dir_all(manifest_alias.join(".okc")).expect("marker");
        symlink(
            external.join(".okc/manifest.json"),
            manifest_alias.join(".okc/manifest.json"),
        )
        .expect("manifest symlink");
        assert!(require_current_schema(&manifest_alias).is_err());
    }
}
