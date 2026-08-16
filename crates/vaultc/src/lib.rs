//! Deterministic Obsidian Vault compiler.

pub mod approval;
pub mod canonical;
pub mod compile;
pub mod config;
pub mod dedup;
pub mod diagnostic;
pub mod error;
pub mod identity;
pub mod ir;
pub mod pack;
pub mod parse;
pub mod plan;
pub mod provenance;
pub mod provider;
pub mod snapshot;
pub mod source;
pub mod verify;
pub mod workspace;

pub use approval::{
    ApprovalDecision, ApprovalLog, ApprovedPlan, ConflictDecision, ConflictDecisionLog,
};
pub use compile::{CompileOptions, CompiledArtifact};
pub use config::{CompilerPolicy, SafetyLimits};
pub use error::{Result, VaultcError};
pub use plan::{DraftPlan, Inspection};
pub use provider::{KnowledgeAugmentor, ProposalValidation, ValidatedProposals};
pub use source::{SourceId, SourceSpec};
pub use verify::VerificationReport;

use std::path::{Path, PathBuf};

/// Stateful façade over the explicitly phased compiler operations.
#[derive(Debug, Clone)]
pub struct VaultCompiler {
    policy: CompilerPolicy,
    workspace: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct VaultCompilerBuilder {
    policy: Option<CompilerPolicy>,
    workspace: Option<PathBuf>,
}

impl VaultCompiler {
    pub fn builder() -> VaultCompilerBuilder {
        VaultCompilerBuilder::default()
    }

    pub fn policy(&self) -> &CompilerPolicy {
        &self.policy
    }

    pub fn inspect(&self, sources: impl IntoIterator<Item = SourceSpec>) -> Result<Inspection> {
        snapshot::inspect_sources(sources, &self.policy, self.workspace.as_deref())
    }

    pub fn plan(&self, inspection: &Inspection) -> Result<DraftPlan> {
        plan::build_plan(inspection, &self.policy)
    }

    pub fn validate_proposals(
        &self,
        plan: &DraftPlan,
        proposals: Vec<vaultc_protocol::KnowledgeProposal>,
    ) -> Result<ValidatedProposals> {
        provider::validate_proposals(plan, proposals, &self.policy)
    }

    pub fn approve(
        &self,
        plan: DraftPlan,
        validated: ValidatedProposals,
        approvals: ApprovalLog,
    ) -> Result<ApprovedPlan> {
        approval::approve_plan(plan, validated, approvals)
    }

    pub fn approve_with_conflicts(
        &self,
        plan: DraftPlan,
        validated: ValidatedProposals,
        approvals: ApprovalLog,
        conflicts: ConflictDecisionLog,
    ) -> Result<ApprovedPlan> {
        approval::approve_plan_with_conflicts(plan, validated, approvals, conflicts)
    }

    pub fn approve_without_augmentation(&self, plan: DraftPlan) -> Result<ApprovedPlan> {
        approval::approve_plan(plan, ValidatedProposals::default(), ApprovalLog::default())
    }

    pub fn compile(
        &self,
        approved: &ApprovedPlan,
        destination: impl AsRef<Path>,
    ) -> Result<CompiledArtifact> {
        compile::compile_plan(approved, destination.as_ref(), &self.policy)
    }

    pub fn verify(&self, artifact: impl AsRef<Path>) -> Result<VerificationReport> {
        verify::verify_artifact(artifact.as_ref())
    }

    pub fn explain_provenance(
        &self,
        artifact: impl AsRef<Path>,
        output_path: &str,
    ) -> Result<provenance::ProvenanceExplanation> {
        provenance::explain(artifact.as_ref(), output_path)
    }
}

impl VaultCompilerBuilder {
    #[must_use]
    pub fn policy(mut self, policy: CompilerPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    #[must_use]
    pub fn workspace(mut self, path: impl Into<PathBuf>) -> Self {
        self.workspace = Some(path.into());
        self
    }

    pub fn build(self) -> Result<VaultCompiler> {
        let policy = self.policy.unwrap_or_default();
        policy.validate()?;
        Ok(VaultCompiler {
            policy,
            workspace: self.workspace,
        })
    }
}
