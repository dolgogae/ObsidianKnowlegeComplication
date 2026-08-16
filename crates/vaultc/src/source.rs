use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, VaultcError};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(VaultcError::InvalidConfig(
                "source ID must contain 1..=128 UTF-8 bytes".into(),
            ));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(VaultcError::InvalidConfig(format!(
                "source ID `{value}` contains unsupported characters"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SourceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSpec {
    Directory { source_id: SourceId, path: PathBuf },
    Zip { source_id: SourceId, path: PathBuf },
    TarZst { source_id: SourceId, path: PathBuf },
}

impl SourceSpec {
    pub fn directory(source_id: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self::Directory {
            source_id: SourceId::new(source_id)?,
            path: path.into(),
        })
    }

    pub fn archive(source_id: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let extension = path.extension().and_then(|value| value.to_str());
        let is_zip = extension.is_some_and(|value| value.eq_ignore_ascii_case("zip"));
        let is_tzst = extension.is_some_and(|value| value.eq_ignore_ascii_case("tzst"));
        let is_tar_zst = extension.is_some_and(|value| value.eq_ignore_ascii_case("zst"))
            && path
                .file_stem()
                .and_then(|value| Path::new(value).extension())
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("tar"));
        if is_zip {
            Ok(Self::Zip {
                source_id: SourceId::new(source_id)?,
                path,
            })
        } else if is_tar_zst || is_tzst {
            Ok(Self::TarZst {
                source_id: SourceId::new(source_id)?,
                path,
            })
        } else {
            Err(VaultcError::UnsupportedSource(path))
        }
    }

    pub fn source_id(&self) -> &SourceId {
        match self {
            Self::Directory { source_id, .. }
            | Self::Zip { source_id, .. }
            | Self::TarZst { source_id, .. } => source_id,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Directory { path, .. } | Self::Zip { path, .. } | Self::TarZst { path, .. } => {
                path
            }
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Directory { .. } => "directory",
            Self::Zip { .. } => "zip",
            Self::TarZst { .. } => "tar_zst",
        }
    }
}
