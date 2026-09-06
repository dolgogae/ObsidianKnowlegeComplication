//! Shared schema-3 orchestration and review service used by CLI and TUI.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use okc_ai::{AiRole, DataBoundary, ProviderClient};
use okc_core::integration::{
    ApprovalBinding, ApprovedClusterRevision, ClusterApproval, CompiledArtifact,
    CompiledVaultManifest, CriticSeverity, DispositionKind, FindingWaiver, OmissionApproval,
    TaxonomyCluster, TaxonomyProposal, verify,
};
use okc_core::{CorpusBuilder, DocumentId, PreparedCorpus, SourceSpec, to_canonical_json};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::project_state::{
    ApprovedTaxonomy, ClusterRegenerationRequest, ClusterTaskOutput, RunRecord, SensitiveScan,
    TaskStage, TaskStatus, TaxonomyTaskOutput, cluster_feedback_hash,
};
use crate::provider_service::ProviderService;
use crate::{
    AppError, OperationControl, OperationKind, OperationPhase, ProgressEvent, ProjectStore, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationCheckpoint {
    NeedsProvider,
    NeedsSources,
    NeedsDisclosure,
    NeedsTaxonomy,
    NeedsClusters,
    ReadyToCompile,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleBoundary {
    pub role: AiRole,
    pub profile_name: String,
    pub boundary: DataBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightSummary {
    pub run_id: String,
    pub documents: usize,
    pub blocks: usize,
    pub input_bytes: u64,
    pub estimated_tokens_min: u64,
    pub estimated_tokens_max: u64,
    pub estimated_requests_min: u64,
    pub estimated_requests_max: u64,
    pub sensitive_findings: usize,
    pub routes: Vec<RoleBoundary>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClusterReviewDecision {
    /// Keys use `document-id:target-id` so every omission is individually
    /// acknowledged even when target labels repeat between source documents.
    pub omission_rationales: BTreeMap<String, String>,
    pub minor_waivers: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct IntegrationService {
    project_path: PathBuf,
    providers: ProviderService,
}

impl IntegrationService {
    pub fn new(project_path: impl Into<PathBuf>, providers: ProviderService) -> Self {
        Self {
            project_path: project_path.into(),
            providers,
        }
    }

    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    pub(crate) fn provider_service(&self) -> &ProviderService {
        &self.providers
    }

    pub fn checkpoint(&self) -> Result<IntegrationCheckpoint> {
        let project = ProjectStore::open(&self.project_path)?;
        if project.manifest().sources.is_empty() {
            return Ok(IntegrationCheckpoint::NeedsSources);
        }
        if let Some(path) = project.latest_verified_output()?
            && path.is_dir()
            && verify(&path).is_ok()
        {
            return Ok(IntegrationCheckpoint::Verified);
        }
        if project.latest_approved_integration_plan()?.is_some() {
            return Ok(IntegrationCheckpoint::ReadyToCompile);
        }
        if !self.required_profiles_exist(&project)? {
            return Ok(IntegrationCheckpoint::NeedsProvider);
        }
        let Some(run) = project.latest_run()? else {
            return Ok(IntegrationCheckpoint::NeedsDisclosure);
        };
        let preflight_complete = project
            .tasks_for_stage(&run.run_id, TaskStage::SensitivePreflight)?
            .iter()
            .any(|task| task.status == TaskStatus::Complete);
        let organizer_complete = project
            .tasks_for_stage(&run.run_id, TaskStage::Organizer)?
            .iter()
            .any(|task| task.status == TaskStatus::Complete);
        if !preflight_complete || !organizer_complete {
            return Ok(IntegrationCheckpoint::NeedsDisclosure);
        }
        let Some(taxonomy) =
            project.latest_approval::<ApprovedTaxonomy>(&run.run_id, "taxonomy", "taxonomy")?
        else {
            return Ok(IntegrationCheckpoint::NeedsTaxonomy);
        };
        if taxonomy.target_hash != taxonomy.value.taxonomy.taxonomy_hash.hex() {
            return Ok(IntegrationCheckpoint::NeedsTaxonomy);
        }
        for cluster in &taxonomy.value.taxonomy.clusters {
            let current = project.latest_approval::<ApprovedClusterRevision>(
                &run.run_id,
                "cluster",
                &cluster.cluster_id,
            )?;
            if current.is_none_or(|approval| {
                approval.target_hash != approval.value.critic.critic_hash.hex()
                    || approval.value.proposal.taxonomy_hash
                        != taxonomy.value.taxonomy.taxonomy_hash
            }) {
                return Ok(IntegrationCheckpoint::NeedsClusters);
            }
        }
        let _lock = project.acquire_writer_lock()?;
        Ok(if project.seal_latest_integration_plan()?.is_some() {
            IntegrationCheckpoint::ReadyToCompile
        } else {
            IntegrationCheckpoint::NeedsClusters
        })
    }

    pub fn preflight(&self, control: &OperationControl) -> Result<PreflightSummary> {
        ensure_not_cancelled(control)?;
        let project = ProjectStore::open(&self.project_path)?;
        let _lock = project.acquire_writer_lock()?;
        observe(control, OperationPhase::Reading, 0, None);
        let builder =
            CorpusBuilder::new().workspace(project.root().join("workspace/build.sqlite3"));
        let sources = project
            .manifest()
            .sources
            .iter()
            .map(source_spec)
            .collect::<Result<Vec<_>>>()?;
        let PreparedCorpus {
            corpus,
            block_texts: block_text,
            source_count,
        } = builder.build(sources)?;
        ensure_not_cancelled(control)?;
        observe(
            control,
            OperationPhase::Processing,
            0,
            Some(source_count as u64),
        );
        let config_bytes = to_canonical_json(&project.manifest().ai_routes)?;
        let config_hash = raw_hash(b"okc:integration-config:v3\0", &config_bytes);
        let run = project.begin_or_resume_run(&corpus.corpus_hash.hex(), &config_hash)?;
        let findings = SensitiveScan::scan(&corpus)?;
        crate::integration_execution::record_preflight(
            &project,
            &run,
            &corpus.corpus_hash.hex(),
            &findings,
        )?;
        let routes = self.role_boundaries(&project)?;
        let input_bytes = block_text
            .values()
            .try_fold(0_u64, |total, text| total.checked_add(text.len() as u64))
            .ok_or_else(|| AppError::InvalidProject("preflight byte count overflow".into()))?;
        let documents = corpus.documents.len();
        observe(
            control,
            OperationPhase::Complete,
            documents as u64,
            Some(documents as u64),
        );
        Ok(PreflightSummary {
            run_id: run.run_id,
            documents,
            blocks: block_text.len(),
            input_bytes,
            estimated_tokens_min: input_bytes.div_ceil(6),
            estimated_tokens_max: input_bytes,
            estimated_requests_min: 2,
            estimated_requests_max: 2_u64.saturating_add((documents as u64).saturating_mul(2)),
            sensitive_findings: findings.len(),
            routes,
        })
    }

    pub fn latest_taxonomy(&self) -> Result<TaxonomyTaskOutput> {
        let project = ProjectStore::open(&self.project_path)?;
        let run = require_run(&project)?;
        let task = project
            .tasks_for_stage(&run.run_id, TaskStage::Organizer)?
            .into_iter()
            .rev()
            .find(|task| task.status == TaskStatus::Complete)
            .ok_or_else(|| AppError::InvalidProject("no complete taxonomy proposal".into()))?;
        project.complete_task_response(&task)
    }

    pub fn approve_taxonomy(
        &self,
        edited_clusters: Option<Vec<TaxonomyCluster>>,
        rationale: Option<String>,
    ) -> Result<ApprovedTaxonomy> {
        let project = ProjectStore::open(&self.project_path)?;
        let _lock = project.acquire_writer_lock()?;
        let run = require_run(&project)?;
        let proposed = self.latest_taxonomy()?;
        let edited = edited_clusters.is_some();
        if edited && rationale.as_deref().is_none_or(str::is_empty) {
            return Err(AppError::InvalidProject(
                "edited taxonomy approval requires a rationale".into(),
            ));
        }
        let taxonomy = if let Some(clusters) = edited_clusters {
            TaxonomyProposal::seal(
                &proposed.corpus,
                clusters,
                proposed.taxonomy.organizer_recording_hash,
            )?
        } else {
            proposed.taxonomy
        };
        let approval = ApprovalBinding {
            target_hash: taxonomy.taxonomy_hash,
            approved: true,
            curator_id: project.manifest().curator_id.clone(),
            policy_version: project.manifest().policy_version.clone(),
            rationale,
        };
        let approved = ApprovedTaxonomy {
            corpus: proposed.corpus,
            taxonomy,
            approval,
        };
        let bytes = to_canonical_json(&approved)?;
        project.append_approval(
            &run.run_id,
            "taxonomy",
            "taxonomy",
            &approved.taxonomy.taxonomy_hash.hex(),
            &bytes,
        )?;
        Ok(approved)
    }

    pub fn completed_clusters(&self) -> Result<Vec<ClusterTaskOutput>> {
        let project = ProjectStore::open(&self.project_path)?;
        let run = require_run(&project)?;
        let Some(taxonomy) =
            project.latest_approval::<ApprovedTaxonomy>(&run.run_id, "taxonomy", "taxonomy")?
        else {
            return Ok(Vec::new());
        };
        let mut latest = BTreeMap::<String, ClusterTaskOutput>::new();
        for task in project
            .tasks_for_stage(&run.run_id, TaskStage::Critic)?
            .into_iter()
            .filter(|task| task.status == TaskStatus::Complete)
        {
            let output: ClusterTaskOutput = project.complete_task_response(&task)?;
            if output.proposal.taxonomy_hash != taxonomy.value.taxonomy.taxonomy_hash
                || project
                    .latest_cluster_regeneration(&run.run_id, &output.proposal.cluster_id)?
                    .is_some_and(|request| request.revision > output.proposal.revision)
            {
                continue;
            }
            let replace = latest
                .get(&output.proposal.cluster_id)
                .is_none_or(|old| old.proposal.revision <= output.proposal.revision);
            if replace {
                latest.insert(output.proposal.cluster_id.clone(), output);
            }
        }
        Ok(latest.into_values().collect())
    }

    pub fn request_cluster_regeneration(
        &self,
        cluster_id: &str,
        feedback: String,
    ) -> Result<ClusterRegenerationRequest> {
        if feedback.trim().is_empty() {
            return Err(AppError::InvalidProject(
                "regeneration feedback must not be empty".into(),
            ));
        }
        let project = ProjectStore::open(&self.project_path)?;
        let _lock = project.acquire_writer_lock()?;
        let run = require_run(&project)?;
        let output = self
            .completed_clusters()?
            .into_iter()
            .find(|output| output.proposal.cluster_id == cluster_id)
            .ok_or_else(|| AppError::InvalidProject("cluster has no complete revision".into()))?;
        let revision =
            output.proposal.revision.checked_add(1).ok_or_else(|| {
                AppError::InvalidProject("cluster revision counter overflow".into())
            })?;
        let feedback_hash = cluster_feedback_hash(
            cluster_id,
            revision,
            &feedback,
            output.proposal.proposal_hash,
            output.critic.critic_hash,
        )?;
        let request = ClusterRegenerationRequest {
            cluster_id: cluster_id.to_owned(),
            revision,
            feedback,
            feedback_hash,
            previous_proposal_hash: output.proposal.proposal_hash,
            previous_critic_hash: output.critic.critic_hash,
        };
        project.append_cluster_regeneration(&request)?;
        if let Some(old) = project.latest_approval::<ApprovedClusterRevision>(
            &run.run_id,
            "cluster",
            cluster_id,
        )? {
            project.append_approval(
                &run.run_id,
                "cluster",
                cluster_id,
                &request.feedback_hash,
                &to_canonical_json(&old.value)?,
            )?;
        }
        Ok(request)
    }

    pub fn approve_cluster(
        &self,
        cluster_id: &str,
        decision: &ClusterReviewDecision,
    ) -> Result<ApprovedClusterRevision> {
        let project = ProjectStore::open(&self.project_path)?;
        let _lock = project.acquire_writer_lock()?;
        let run = require_run(&project)?;
        let output = self
            .completed_clusters()?
            .into_iter()
            .find(|output| output.proposal.cluster_id == cluster_id)
            .ok_or_else(|| {
                AppError::InvalidProject("cluster has no complete critic report".into())
            })?;
        if output.critic.findings.iter().any(|finding| {
            matches!(
                finding.severity,
                CriticSeverity::Major | CriticSeverity::Critical
            )
        }) {
            return Err(AppError::InvalidProject(
                "major or critical critic findings require regeneration".into(),
            ));
        }
        let curator = &project.manifest().curator_id;
        let mut omissions = Vec::new();
        for disposition in output
            .proposal
            .dispositions
            .iter()
            .filter(|item| item.disposition == DispositionKind::OmissionProposed)
        {
            let key = omission_key(
                disposition.target.document_id,
                &disposition.target.target_id,
            );
            let rationale = decision
                .omission_rationales
                .get(&key)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AppError::InvalidProject(format!(
                        "omission `{key}` requires an individual rationale"
                    ))
                })?;
            omissions.push(OmissionApproval {
                target: disposition.target.clone(),
                curator_id: curator.clone(),
                rationale: rationale.clone(),
            });
        }
        let mut waivers = Vec::new();
        for finding in output
            .critic
            .findings
            .iter()
            .filter(|finding| finding.severity == CriticSeverity::Minor)
        {
            let rationale = decision
                .minor_waivers
                .get(&finding.finding_id)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AppError::InvalidProject(format!(
                        "minor finding `{}` requires an individual rationale",
                        finding.finding_id
                    ))
                })?;
            waivers.push(FindingWaiver {
                finding_id: finding.finding_id.clone(),
                curator_id: curator.clone(),
                rationale: rationale.clone(),
            });
        }
        let approval = ClusterApproval::approve(
            &output.proposal,
            &output.critic,
            curator,
            &project.manifest().policy_version,
            omissions,
            waivers,
        )?;
        let approved = ApprovedClusterRevision {
            proposal: output.proposal,
            critic: output.critic,
            approval,
        };
        let bytes = to_canonical_json(&approved)?;
        project.append_cluster_revision(
            &run.run_id,
            cluster_id,
            &approved.approval.revision_hash.hex(),
            &bytes,
        )?;
        project.append_approval(
            &run.run_id,
            "cluster",
            cluster_id,
            &approved.critic.critic_hash.hex(),
            &bytes,
        )?;
        let _ = project.seal_latest_integration_plan()?;
        Ok(approved)
    }

    pub fn compile_latest(
        &self,
        destination: impl AsRef<Path>,
        control: &OperationControl,
    ) -> Result<(CompiledArtifact, CompiledVaultManifest)> {
        ensure_not_cancelled(control)?;
        let project = ProjectStore::open(&self.project_path)?;
        let _lock = project.acquire_writer_lock()?;
        let plan = project.latest_approved_integration_plan()?.ok_or_else(|| {
            AppError::InvalidProject("no current approved integration plan".into())
        })?;
        let destination = destination.as_ref();
        let absolute = absolute_output(destination)?;
        if project
            .manifest()
            .sources
            .iter()
            .any(|source| crate::workspace_bootstrap::paths_overlap(&absolute, &source.path))
        {
            return Err(AppError::InvalidProject(
                "compiled output must be outside every source".into(),
            ));
        }
        observe(control, OperationPhase::Staging, 0, Some(1));
        ensure_not_cancelled(control)?;
        // The core owns the atomic publication barrier. Once called, callers
        // must report publication as non-cancellable until it returns.
        observe(control, OperationPhase::Publishing, 0, Some(1));
        let artifact = project.compile_locked(&plan, &absolute)?;
        observe(control, OperationPhase::Verifying, 0, Some(1));
        let manifest = verify(&artifact.path)?;
        project.record_verified_output(&artifact.path, &manifest)?;
        observe(control, OperationPhase::Complete, 1, Some(1));
        Ok((artifact, manifest))
    }

    pub fn verify(
        &self,
        artifact: impl AsRef<Path>,
        control: &OperationControl,
    ) -> Result<CompiledVaultManifest> {
        ensure_not_cancelled(control)?;
        observe(control, OperationPhase::Verifying, 0, Some(1));
        let project = ProjectStore::open(&self.project_path)?;
        let _lock = project.acquire_writer_lock()?;
        let manifest = verify(artifact.as_ref())?;
        project.record_verified_output(artifact.as_ref(), &manifest)?;
        observe(control, OperationPhase::Complete, 1, Some(1));
        Ok(manifest)
    }

    fn required_profiles_exist(&self, project: &ProjectStore) -> Result<bool> {
        let config = self.providers.load_config()?;
        for role in [
            AiRole::Embedding,
            AiRole::Organizer,
            AiRole::Synthesis,
            AiRole::Critic,
        ] {
            let Some(profile) = config
                .profiles
                .get(project.manifest().ai_routes.profile_for(role))
            else {
                return Ok(false);
            };
            let client = ProviderClient::new(profile.clone())?;
            let capabilities = okc_ai::StructuredGenerator::capabilities(&client);
            if !capabilities.structured_generation
                || (role == AiRole::Embedding && !capabilities.embeddings)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn role_boundaries(&self, project: &ProjectStore) -> Result<Vec<RoleBoundary>> {
        let config = self.providers.load_config()?;
        [
            AiRole::Embedding,
            AiRole::Organizer,
            AiRole::Synthesis,
            AiRole::Critic,
        ]
        .into_iter()
        .map(|role| {
            let name = project.manifest().ai_routes.profile_for(role);
            let profile = config.profiles.get(name).ok_or_else(|| {
                AppError::InvalidProject(format!("AI role references missing profile `{name}`"))
            })?;
            Ok(RoleBoundary {
                role,
                profile_name: name.into(),
                boundary: profile.data_boundary(),
            })
        })
        .collect()
    }
}

pub fn omission_key(document_id: DocumentId, target_id: &str) -> String {
    format!("{document_id}:{target_id}")
}

fn require_run(project: &ProjectStore) -> Result<RunRecord> {
    project
        .latest_run()?
        .ok_or_else(|| AppError::InvalidProject("project has no integration run".into()))
}

fn ensure_not_cancelled(control: &OperationControl) -> Result<()> {
    if control.cancellation.is_cancelled() {
        return Err(AppError::InvalidProject("operation was cancelled".into()));
    }
    Ok(())
}

fn observe(control: &OperationControl, phase: OperationPhase, completed: u64, total: Option<u64>) {
    control.observer.observe(&ProgressEvent {
        operation: OperationKind::Integrate,
        phase,
        completed,
        total,
        current_item: None,
    });
}

pub(crate) fn source_spec(binding: &crate::SourceBinding) -> Result<SourceSpec> {
    let extension = binding
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut source = if matches!(extension.as_str(), "zip" | "zst" | "tzst") {
        SourceSpec::archive(binding.source_id.as_str(), &binding.path)
    } else {
        SourceSpec::directory(binding.source_id.as_str(), &binding.path)
    }?;
    if let Some(owner) = &binding.owner_display_name {
        source = source.with_owner_display_name(owner.clone())?;
    }
    Ok(source)
}

pub(crate) fn raw_hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn absolute_output(path: &Path) -> Result<PathBuf> {
    let mut ancestor = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut missing = Vec::<OsString>::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => {
                ancestor = fs::canonicalize(&ancestor)?;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = ancestor.file_name().ok_or_else(|| {
                    AppError::InvalidProject("output path has no existing ancestor".into())
                })?;
                missing.push(component.to_os_string());
                if !ancestor.pop() {
                    return Err(AppError::InvalidProject(
                        "output path has no existing ancestor".into(),
                    ));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    for component in missing.into_iter().rev() {
        ancestor.push(component);
    }
    Ok(ancestor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn output_resolution_closes_existing_symlink_parent_aliases() {
        let temporary = tempfile::tempdir().expect("temporary");
        let source = temporary.path().join("source");
        fs::create_dir(&source).expect("source");
        let alias = temporary.path().join("alias");
        std::os::unix::fs::symlink(&source, &alias).expect("alias");

        assert_eq!(
            absolute_output(&alias.join("IntegratedVault")).expect("resolved output"),
            fs::canonicalize(source)
                .expect("canonical source")
                .join("IntegratedVault")
        );
    }
}
