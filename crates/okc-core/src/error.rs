use std::path::PathBuf;

use thiserror::Error;

pub type Result<T, E = OkcError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OkcError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("unsafe path `{path}`: {reason}")]
    UnsafePath { path: String, reason: String },
    #[error("unsupported source `{0}`")]
    UnsupportedSource(PathBuf),
    #[error("resource limit exceeded: {0}")]
    ResourceLimit(String),
    #[error("malformed input `{path}`: {reason}")]
    MalformedInput { path: String, reason: String },
    #[error("identity mismatch for `{0}`")]
    IdentityMismatch(String),
    #[error("plan is stale: {0}")]
    PlanStale(String),
    #[error("proposal is invalid: {0}")]
    ProposalInvalid(String),
    #[error("approval is stale or missing: {0}")]
    ApprovalStale(String),
    #[error("destination already exists: {0}")]
    OutputExists(PathBuf),
    #[error(
        "`{path}` was published completely, but synchronizing its parent directory failed; durability is uncertain: {source}"
    )]
    PublishedButDurabilityUncertain {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "sqlite")]
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("internal invariant failed: {0}")]
    Internal(String),
}

impl OkcError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
