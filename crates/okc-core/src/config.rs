use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{OkcError, Result};

pub const POLICY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompilerPolicy {
    pub schema_version: u32,
    pub limits: SafetyLimits,
    pub paths: PathPolicy,
    pub dedup: DedupPolicy,
    pub output: OutputPolicy,
    pub augmentation: AugmentationPolicy,
}

impl Default for CompilerPolicy {
    fn default() -> Self {
        Self {
            schema_version: POLICY_SCHEMA_VERSION,
            limits: SafetyLimits::default(),
            paths: PathPolicy::default(),
            dedup: DedupPolicy::default(),
            output: OutputPolicy::default(),
            augmentation: AugmentationPolicy::default(),
        }
    }
}

impl CompilerPolicy {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|error| OkcError::io(path, error))?;
        let text = std::str::from_utf8(&bytes).map_err(|error| OkcError::MalformedInput {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        let policy: Self = toml::from_str(text)?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != POLICY_SCHEMA_VERSION {
            return Err(OkcError::InvalidConfig(format!(
                "unsupported policy schema version {}",
                self.schema_version
            )));
        }
        if self.limits.max_sources == 0
            || self.limits.max_files == 0
            || self.limits.max_total_bytes == 0
            || self.limits.max_file_bytes == 0
            || self.limits.max_structured_text_bytes == 0
            || self.limits.max_archive_expansion_ratio == 0
            || self.limits.max_path_bytes == 0
            || self.limits.max_component_bytes == 0
        {
            return Err(OkcError::InvalidConfig(
                "all safety limits must be positive".into(),
            ));
        }
        if self.limits.max_file_bytes > self.limits.max_total_bytes {
            return Err(OkcError::InvalidConfig(
                "per-file byte limit cannot exceed the total byte limit".into(),
            ));
        }
        if self.limits.max_structured_text_bytes > self.limits.max_file_bytes {
            return Err(OkcError::InvalidConfig(
                "structured-text byte limit cannot exceed the per-file byte limit".into(),
            ));
        }
        if self.limits.max_component_bytes < 80
            || self.limits.max_path_bytes < self.limits.max_component_bytes
        {
            return Err(OkcError::InvalidConfig(
                "path limit must cover a component and component limit must be at least 80 bytes"
                    .into(),
            ));
        }
        if self.paths.follow_symlinks {
            return Err(OkcError::InvalidConfig(
                "following source symlinks is not supported by the V2 safety policy".into(),
            ));
        }
        if !self.output.reject_existing_destination {
            return Err(OkcError::InvalidConfig(
                "V2 always rejects an existing destination; an update workflow requires a new ADR"
                    .into(),
            ));
        }
        let mut excludes = ignore::gitignore::GitignoreBuilder::new("");
        for pattern in &self.paths.exclude {
            if pattern.starts_with('!') {
                return Err(OkcError::InvalidConfig(
                    "exclude patterns cannot contain gitignore negations".into(),
                ));
            }
            excludes.add_line(None, pattern).map_err(|error| {
                OkcError::InvalidConfig(format!("invalid exclude pattern `{pattern}`: {error}"))
            })?;
        }
        excludes.build().map_err(|error| {
            OkcError::InvalidConfig(format!("invalid exclude pattern set: {error}"))
        })?;
        if self.paths.nonstandard_extensions.iter().any(|extension| {
            extension.is_empty()
                || extension.len() > 32
                || !extension
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        }) {
            return Err(OkcError::InvalidConfig(
                "nonstandard extensions must be bounded ASCII names without a leading dot".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.dedup.near_threshold) {
            return Err(OkcError::InvalidConfig(
                "near duplicate threshold must be between 0 and 1".into(),
            ));
        }
        if self.dedup.minhash_components != 128
            || self.dedup.lsh_bands != 32
            || self.dedup.rows_per_band != 4
        {
            return Err(OkcError::InvalidConfig(
                "V2 identity policy requires 128 MinHash components in 32x4 bands".into(),
            ));
        }
        if self.dedup.max_candidates_per_document == 0 {
            return Err(OkcError::InvalidConfig(
                "near-duplicate candidate limit must be positive".into(),
            ));
        }
        if self.augmentation.max_proposals == 0 || self.augmentation.max_generated_bytes == 0 {
            return Err(OkcError::InvalidConfig(
                "augmentation limits must be positive".into(),
            ));
        }
        Ok(())
    }

    pub fn semantic_hash(&self) -> Result<crate::identity::ContentHash> {
        crate::canonical::canonical_hash("okc:policy:v2\0", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SafetyLimits {
    pub max_sources: u32,
    pub max_files: u64,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_structured_text_bytes: u64,
    pub max_archive_expansion_ratio: u64,
    pub max_path_bytes: usize,
    pub max_component_bytes: usize,
}

impl Default for SafetyLimits {
    fn default() -> Self {
        Self {
            max_sources: 10,
            max_files: 100_000,
            max_total_bytes: 20 * 1024 * 1024 * 1024,
            max_file_bytes: 2 * 1024 * 1024 * 1024,
            max_structured_text_bytes: 16 * 1024 * 1024,
            max_archive_expansion_ratio: 100,
            max_path_bytes: 1024,
            max_component_bytes: 240,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathPolicy {
    pub follow_symlinks: bool,
    pub exclude: Vec<String>,
    pub nonstandard_files: NonstandardFilePolicy,
    pub nonstandard_extensions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NonstandardFilePolicy {
    #[default]
    PreserveOpaque,
    Exclude,
}

impl Default for PathPolicy {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            nonstandard_files: NonstandardFilePolicy::PreserveOpaque,
            nonstandard_extensions: Vec::new(),
            exclude: vec![
                ".obsidian/**".into(),
                ".git/**".into(),
                "**/.DS_Store".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DedupPolicy {
    pub near_threshold: f64,
    pub minhash_components: usize,
    pub lsh_bands: usize,
    pub rows_per_band: usize,
    pub max_candidates_per_document: usize,
}

impl Default for DedupPolicy {
    fn default() -> Self {
        Self {
            near_threshold: 0.85,
            minhash_components: 128,
            lsh_bands: 32,
            rows_per_band: 4,
            max_candidates_per_document: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputPolicy {
    pub reject_existing_destination: bool,
    pub deny_warnings: bool,
    pub retain_failed_staging: bool,
    pub zstd_level: i32,
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self {
            reject_existing_destination: true,
            deny_warnings: false,
            retain_failed_staging: false,
            zstd_level: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AugmentationPolicy {
    pub allow_remote_providers: bool,
    pub max_proposals: u32,
    pub max_generated_bytes: u64,
}

impl Default for AugmentationPolicy {
    fn default() -> Self {
        Self {
            allow_remote_providers: false,
            max_proposals: 1000,
            max_generated_bytes: 16 * 1024 * 1024,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_cannot_disable_existing_destination_rejection() {
        let mut policy = CompilerPolicy::default();
        policy.output.reject_existing_destination = false;
        assert!(matches!(policy.validate(), Err(OkcError::InvalidConfig(_))));
    }
}
