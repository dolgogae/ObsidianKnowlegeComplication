#![deprecated(
    since = "0.2.0",
    note = "use the okc-core crate; the vaultc facade will be removed after the 0.2 minor line"
)]
//! Read-only source compatibility facade for applications migrating to OKC V2.
//!
//! This package intentionally has no binary target. New integrations should
//! depend on `okc-core` and invoke the single `okc` executable.

pub use okc_core::*;

#[deprecated(since = "0.2.0", note = "use okc_core::OkcCompiler")]
pub type VaultCompiler = okc_core::OkcCompiler;
#[deprecated(since = "0.2.0", note = "use okc_core::OkcCompilerBuilder")]
pub type VaultCompilerBuilder = okc_core::OkcCompilerBuilder;
#[deprecated(since = "0.2.0", note = "use okc_core::OkcError")]
pub type VaultcError = okc_core::OkcError;
#[deprecated(since = "0.2.0", note = "use okc_core::DecisionOverlay")]
pub type ConflictDecision = okc_core::DecisionOverlay;
#[deprecated(since = "0.2.0", note = "use okc_core::DecisionOverlayLog")]
pub type ConflictDecisionLog = okc_core::DecisionOverlayLog;
