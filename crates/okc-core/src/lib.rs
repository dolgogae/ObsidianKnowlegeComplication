//! Current-schema Obsidian Vault compiler primitives.

#![allow(
    clippy::struct_field_names,
    reason = "serialized contract field names must remain byte-compatible"
)]

mod cancellation;
mod canonical;
mod config;
mod corpus;
mod dedup;
mod diagnostic;
mod error;
mod identity;
pub mod integration;
mod ir;
mod json;
mod parse;
mod plan;
mod snapshot;
mod source;
mod source_io;
#[cfg(feature = "sqlite")]
mod workspace;

pub use cancellation::CancellationToken;
pub use canonical::to_canonical_json;
pub use corpus::{BlockTextMap, CorpusBuilder, PreparedCorpus};
pub use error::{OkcError, Result};
pub use identity::{BlockId, ContentHash, DocumentId};
pub use json::parse_json_strict;
pub use source::{SourceId, SourceSpec};
