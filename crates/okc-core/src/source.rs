use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{OkcError, Result};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SourceId(String);

impl<'de> Deserialize<'de> for SourceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl SourceId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 || matches!(value.as_str(), "." | "..") {
            return Err(OkcError::InvalidConfig(
                "source ID must contain 1..=128 UTF-8 bytes".into(),
            ));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(OkcError::InvalidConfig(format!(
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceSpec {
    Directory {
        source_id: SourceId,
        path: PathBuf,
        owner_display_name: Option<String>,
    },
    Zip {
        source_id: SourceId,
        path: PathBuf,
        owner_display_name: Option<String>,
    },
    TarZst {
        source_id: SourceId,
        path: PathBuf,
        owner_display_name: Option<String>,
    },
}

impl SourceSpec {
    pub fn directory(source_id: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self::Directory {
            source_id: SourceId::new(source_id)?,
            path: path.into(),
            owner_display_name: None,
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
                owner_display_name: None,
            })
        } else if is_tar_zst || is_tzst {
            Ok(Self::TarZst {
                source_id: SourceId::new(source_id)?,
                path,
                owner_display_name: None,
            })
        } else {
            Err(OkcError::UnsupportedSource(path))
        }
    }

    pub fn with_owner_display_name(mut self, owner: impl Into<String>) -> Result<Self> {
        let owner = owner.into();
        if owner.trim().is_empty() || owner.len() > 256 || owner.chars().any(char::is_control) {
            return Err(OkcError::InvalidConfig(
                "owner display name is empty, too long, or contains controls".into(),
            ));
        }
        match &mut self {
            Self::Directory {
                owner_display_name, ..
            }
            | Self::Zip {
                owner_display_name, ..
            }
            | Self::TarZst {
                owner_display_name, ..
            } => *owner_display_name = Some(owner),
        }
        Ok(self)
    }

    pub fn owner_display_name(&self) -> Option<&str> {
        match self {
            Self::Directory {
                owner_display_name, ..
            }
            | Self::Zip {
                owner_display_name, ..
            }
            | Self::TarZst {
                owner_display_name, ..
            } => owner_display_name.as_deref(),
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
