//! Frozen read-only V2 compatibility boundary used by the 0.3 application.
//!
//! No inspection, planning, approval, compilation, or pack-writing method is
//! exposed from this package.

use std::path::Path;

use okc_core::provenance::{ProvenancePage, ProvenanceQuery};
use okc_core::{OkcCompiler, Result, VerificationReport};

#[derive(Debug, Clone)]
pub struct LegacyV2Reader {
    compiler: OkcCompiler,
}

impl LegacyV2Reader {
    pub fn new() -> Result<Self> {
        Ok(OkcCompiler::builder().build()?.into())
    }

    pub fn verify(&self, artifact: impl AsRef<Path>) -> Result<VerificationReport> {
        self.compiler.verify(artifact)
    }

    pub fn explain(
        &self,
        artifact: impl AsRef<Path>,
        query: &ProvenanceQuery,
    ) -> Result<ProvenancePage> {
        self.compiler.explain_provenance_page(artifact, query)
    }
}

impl From<OkcCompiler> for LegacyV2Reader {
    fn from(compiler: OkcCompiler) -> Self {
        Self { compiler }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_constructs_without_exposing_a_writer() {
        let reader = LegacyV2Reader::new().expect("legacy reader");
        assert!(format!("{reader:?}").contains("LegacyV2Reader"));
    }
}
