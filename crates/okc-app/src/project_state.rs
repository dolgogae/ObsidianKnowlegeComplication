//! Current-schema application services, append-only run journal, and disclosure gate.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use okc_ai::{AiRole, DataBoundary, ProviderCapabilities};
use okc_core::integration::{
    ApprovalBinding, ApprovedClusterRevision, ApprovedIntegrationPlan, CompiledArtifact,
    CompiledVaultManifest, CriticReport, IntegrationCorpus, SynthesisProposal, TaxonomyProposal,
    compile,
};
use okc_core::{BlockId, ContentHash, DocumentId};
use rusqlite::{Connection, OptionalExtension as _, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{AppError, PROJECT_SCHEMA_VERSION, ProjectStore, Result, hex_digest};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiRouteConfig {
    pub default: String,
    pub embedding: Option<String>,
    pub organizer: Option<String>,
    pub synthesis: Option<String>,
    pub critic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterRegenerationRequest {
    pub cluster_id: String,
    pub revision: u32,
    pub feedback: String,
    pub feedback_hash: String,
    pub previous_proposal_hash: ContentHash,
    pub previous_critic_hash: ContentHash,
}

pub fn cluster_feedback_hash(
    cluster_id: &str,
    revision: u32,
    feedback: &str,
    previous_proposal_hash: ContentHash,
    previous_critic_hash: ContentHash,
) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"okc:cluster-feedback:v3\0");
    hasher.update(okc_core::to_canonical_json(&(
        cluster_id,
        revision,
        feedback,
        previous_proposal_hash,
        previous_critic_hash,
    ))?);
    Ok(format!("{:x}", hasher.finalize()))
}

impl Default for AiRouteConfig {
    fn default() -> Self {
        Self {
            default: "default".into(),
            embedding: None,
            organizer: None,
            synthesis: None,
            critic: None,
        }
    }
}

impl AiRouteConfig {
    pub fn validate(&self) -> Result<()> {
        for profile in std::iter::once(Some(&self.default))
            .chain([
                self.embedding.as_ref(),
                self.organizer.as_ref(),
                self.synthesis.as_ref(),
                self.critic.as_ref(),
            ])
            .flatten()
        {
            if profile.is_empty()
                || profile.len() > 128
                || !profile
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(AppError::InvalidProject(format!(
                    "invalid AI profile name `{profile}`"
                )));
            }
        }
        Ok(())
    }

    pub fn profile_for(&self, role: AiRole) -> &str {
        match role {
            AiRole::Embedding => self.embedding.as_deref(),
            AiRole::Organizer => self.organizer.as_deref(),
            AiRole::Synthesis => self.synthesis.as_deref(),
            AiRole::Critic => self.critic.as_deref(),
        }
        .unwrap_or(&self.default)
    }

    pub fn set(&mut self, role: Option<AiRole>, profile: String) -> Result<()> {
        let mut next = self.clone();
        match role {
            None => next.default = profile,
            Some(AiRole::Embedding) => next.embedding = Some(profile),
            Some(AiRole::Organizer) => next.organizer = Some(profile),
            Some(AiRole::Synthesis) => next.synthesis = Some(profile),
            Some(AiRole::Critic) => next.critic = Some(profile),
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}

pub fn validate_bcp47(language: &str) -> Result<()> {
    if language.is_empty()
        || language.len() > 63
        || language.starts_with('-')
        || language.ends_with('-')
        || language.split('-').any(|part| {
            part.is_empty()
                || part.len() > 8
                || !part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
    {
        return Err(AppError::InvalidProject(format!(
            "invalid BCP-47 language tag `{language}`"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub run_id: String,
    pub input_hash: String,
    pub config_hash: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStage {
    SensitivePreflight,
    Embedding,
    SemanticCandidates,
    Organizer,
    Synthesis,
    Critic,
    Materialization,
}

impl TaskStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SensitivePreflight => "sensitive_preflight",
            Self::Embedding => "embedding",
            Self::SemanticCandidates => "semantic_candidates",
            Self::Organizer => "organizer",
            Self::Synthesis => "synthesis",
            Self::Critic => "critic",
            Self::Materialization => "materialization",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Cancelled,
}

impl TaskStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDefinition {
    pub task_id: String,
    pub run_id: String,
    pub stage: TaskStage,
    pub cache_key: String,
    pub request_object: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskState {
    pub task: TaskDefinition,
    pub status: TaskStatus,
    pub response_object: Option<String>,
    pub error_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationStatus {
    pub schema_version: u32,
    pub run: Option<RunRecord>,
    pub tasks: Vec<TaskState>,
    pub completed: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRecord<T> {
    pub target_hash: String,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyTaskOutput {
    pub corpus: IntegrationCorpus,
    pub taxonomy: TaxonomyProposal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedTaxonomy {
    pub corpus: IntegrationCorpus,
    pub taxonomy: TaxonomyProposal,
    pub approval: ApprovalBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterTaskOutput {
    pub proposal: SynthesisProposal,
    pub critic: CriticReport,
}

impl ProjectStore {
    pub fn append_cluster_regeneration(&self, request: &ClusterRegenerationRequest) -> Result<()> {
        if request.feedback.trim().is_empty() || request.revision < 2 {
            return Err(AppError::InvalidProject(
                "cluster regeneration requires feedback and revision >= 2".into(),
            ));
        }
        let run = self.latest_run()?.ok_or_else(|| {
            AppError::InvalidProject("cluster regeneration requires a current run".into())
        })?;
        let expected = cluster_feedback_hash(
            &request.cluster_id,
            request.revision,
            &request.feedback,
            request.previous_proposal_hash,
            request.previous_critic_hash,
        )?;
        if request.feedback_hash != expected {
            return Err(AppError::InvalidProject(
                "cluster feedback hash does not match its binding".into(),
            ));
        }
        let object = self.put_object(&okc_core::to_canonical_json(request)?)?;
        self.connection()?.execute(
            "INSERT INTO cluster_feedback_v4(\
                run_id,cluster_id,revision,feedback_hash,feedback_object,\
                previous_proposal_hash,previous_critic_hash\
             ) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                run.run_id,
                request.cluster_id,
                request.revision,
                request.feedback_hash,
                object,
                request.previous_proposal_hash.hex(),
                request.previous_critic_hash.hex(),
            ],
        )?;
        Ok(())
    }

    pub fn latest_cluster_regeneration(
        &self,
        run_id: &str,
        cluster_id: &str,
    ) -> Result<Option<ClusterRegenerationRequest>> {
        let object = self
            .connection()?
            .query_row(
                "SELECT feedback_object FROM cluster_feedback_v4 \
                 WHERE run_id=?1 AND cluster_id=?2 ORDER BY sequence DESC LIMIT 1",
                params![run_id, cluster_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        object
            .map(|object| {
                Ok(serde_json::from_slice::<ClusterRegenerationRequest>(
                    &self.read_object(&object)?,
                )?)
            })
            .transpose()
    }
    pub fn set_language(&mut self, language: Option<String>) -> Result<()> {
        let _lock = self.acquire_writer_lock()?;
        if let Some(language) = &language {
            validate_bcp47(language)?;
        }
        if self.manifest.language == language {
            return Ok(());
        }
        let mut next_manifest = self.manifest.clone();
        next_manifest.language = language;
        self.commit_configuration(next_manifest, "project language changed")
    }

    pub fn set_ai_route(&mut self, role: Option<AiRole>, profile: String) -> Result<()> {
        let _lock = self.acquire_writer_lock()?;
        let mut next_manifest = self.manifest.clone();
        next_manifest.ai_routes.set(role, profile)?;
        if next_manifest.ai_routes == self.manifest.ai_routes {
            return Ok(());
        }
        self.commit_configuration(next_manifest, "AI route changed")
    }

    pub fn begin_or_resume_run(&self, input_hash: &str, config_hash: &str) -> Result<RunRecord> {
        validate_hex_hash("input hash", input_hash)?;
        validate_hex_hash("configuration hash", config_hash)?;
        let connection = self.connection()?;
        if let Some(existing) = connection
            .query_row(
                "SELECT run_id, input_hash, config_hash, sequence FROM runs \
                 ORDER BY sequence DESC LIMIT 1",
                [],
                |row| {
                    Ok(RunRecord {
                        run_id: row.get(0)?,
                        input_hash: row.get(1)?,
                        config_hash: row.get(2)?,
                        sequence: row.get(3)?,
                    })
                },
            )
            .optional()?
            && existing.input_hash == input_hash
            && existing.config_hash == config_hash
        {
            return Ok(existing);
        }
        let sequence: u64 = connection.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM runs",
            [],
            |row| row.get(0),
        )?;
        let run_id = format!(
            "run_{}",
            digest(
                b"okc:run:v3\0",
                &[
                    input_hash.as_bytes(),
                    config_hash.as_bytes(),
                    &sequence.to_be_bytes()
                ]
            )
        );
        connection.execute(
            "INSERT INTO runs(run_id,input_hash,config_hash,sequence) VALUES (?1,?2,?3,?4)",
            params![run_id, input_hash, config_hash, sequence],
        )?;
        Ok(RunRecord {
            run_id,
            input_hash: input_hash.into(),
            config_hash: config_hash.into(),
            sequence,
        })
    }

    pub fn register_task(
        &self,
        run: &RunRecord,
        stage: TaskStage,
        cache_key: &TaskCacheKey,
        request_bytes: &[u8],
    ) -> Result<TaskState> {
        if cache_key.stage != stage {
            return Err(AppError::InvalidProject(
                "task stage does not match its cache key".into(),
            ));
        }
        let cache_key = cache_key.hash()?;
        let request_object = self.put_object(request_bytes)?;
        let connection = self.connection()?;
        if let Some(task_id) = connection
            .query_row(
                "SELECT task_id FROM task_definitions WHERE run_id=?1 AND cache_key=?2",
                params![run.run_id, cache_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            let task = self.task_state(&task_id)?;
            if task.task.request_object != request_object {
                return Err(AppError::InvalidProject(
                    "task cache key is stale for its request bytes".into(),
                ));
            }
            return Ok(task);
        }
        let task_id = format!(
            "task_{}",
            digest(
                b"okc:task:v3\0",
                &[run.run_id.as_bytes(), cache_key.as_bytes()]
            )
        );
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO task_definitions(task_id,run_id,stage,cache_key,request_object) \
             VALUES (?1,?2,?3,?4,?5)",
            params![
                task_id,
                run.run_id,
                stage.as_str(),
                cache_key,
                request_object
            ],
        )?;
        transaction.execute(
            "INSERT INTO task_events(task_id,status) VALUES (?1,?2)",
            params![task_id, TaskStatus::Queued.as_str()],
        )?;
        transaction.commit()?;
        self.task_state(&task_id)
    }

    pub fn append_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
        response: Option<&[u8]>,
        error_kind: Option<&str>,
    ) -> Result<TaskState> {
        if status == TaskStatus::Complete && (response.is_none() || error_kind.is_some()) {
            return Err(AppError::InvalidProject(
                "a complete task requires one response and no error".into(),
            ));
        }
        if matches!(status, TaskStatus::Failed | TaskStatus::Cancelled) && error_kind.is_none() {
            return Err(AppError::InvalidProject(
                "failed/cancelled task events require an error kind".into(),
            ));
        }
        let response_object = response.map(|bytes| self.put_object(bytes)).transpose()?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO task_events(task_id,status,response_object,error_kind) \
             VALUES (?1,?2,?3,?4)",
            params![task_id, status.as_str(), response_object, error_kind],
        )?;
        self.task_state(task_id)
    }

    pub fn record_exchange(
        &self,
        run_id: &str,
        task_id: &str,
        canonical_recording: &[u8],
    ) -> Result<String> {
        let recording_hash = digest(b"okc:provider-recording:v3\0", &[canonical_recording]);
        let recording_object = self.put_object(canonical_recording)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO exchanges_v3(run_id,task_id,recording_object,recording_hash) \
             VALUES (?1,?2,?3,?4)",
            params![run_id, task_id, recording_object, recording_hash],
        )?;
        Ok(recording_hash)
    }

    pub fn append_cluster_revision(
        &self,
        run_id: &str,
        cluster_id: &str,
        revision_hash: &str,
        bytes: &[u8],
    ) -> Result<()> {
        validate_identifier("cluster ID", cluster_id)?;
        validate_hex_hash("cluster revision hash", revision_hash)?;
        let object_id = self.put_object(bytes)?;
        self.connection()?.execute(
            "INSERT INTO cluster_revisions_v3(run_id,cluster_id,revision_hash,object_id) \
             VALUES (?1,?2,?3,?4)",
            params![run_id, cluster_id, revision_hash, object_id],
        )?;
        Ok(())
    }

    pub fn append_approval(
        &self,
        run_id: &str,
        approval_kind: &str,
        target_id: &str,
        target_hash: &str,
        bytes: &[u8],
    ) -> Result<()> {
        validate_identifier("approval kind", approval_kind)?;
        validate_identifier("approval target", target_id)?;
        validate_hex_hash("approval target hash", target_hash)?;
        let object_id = self.put_object(bytes)?;
        self.connection()?.execute(
            "INSERT INTO approvals_v3(run_id,approval_kind,target_id,target_hash,object_id) \
             VALUES (?1,?2,?3,?4,?5)",
            params![run_id, approval_kind, target_id, target_hash, object_id],
        )?;
        Ok(())
    }

    pub fn integration_status(&self) -> Result<IntegrationStatus> {
        let connection = self.connection()?;
        let run = connection
            .query_row(
                "SELECT run_id,input_hash,config_hash,sequence FROM runs ORDER BY sequence DESC LIMIT 1",
                [],
                |row| {
                    Ok(RunRecord {
                        run_id: row.get(0)?,
                        input_hash: row.get(1)?,
                        config_hash: row.get(2)?,
                        sequence: row.get(3)?,
                    })
                },
            )
            .optional()?;
        let tasks = if let Some(run) = &run {
            let mut statement = connection
                .prepare("SELECT task_id FROM task_definitions WHERE run_id=?1 ORDER BY task_id")?;
            let ids = statement
                .query_map(params![run.run_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ids.into_iter()
                .map(|task_id| self.task_state(&task_id))
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(IntegrationStatus {
            schema_version: PROJECT_SCHEMA_VERSION,
            completed: tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Complete)
                .count(),
            failed: tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Failed)
                .count(),
            run,
            tasks,
        })
    }

    pub fn latest_run(&self) -> Result<Option<RunRecord>> {
        Ok(self.integration_status()?.run)
    }

    pub fn tasks_for_stage(&self, run_id: &str, stage: TaskStage) -> Result<Vec<TaskState>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT d.task_id FROM task_definitions d \
             JOIN (SELECT task_id, MAX(sequence) AS sequence FROM task_events GROUP BY task_id) e \
             ON e.task_id=d.task_id WHERE d.run_id=?1 AND d.stage=?2 ORDER BY e.sequence",
        )?;
        let ids = statement
            .query_map(params![run_id, stage.as_str()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|task_id| self.task_state(&task_id))
            .collect()
    }

    pub fn read_object(&self, object_id: &str) -> Result<Vec<u8>> {
        validate_hex_hash("object ID", object_id)?;
        self.validate_object_directory()?;
        let path = self.root.join("objects").join(object_id);
        super::validate_managed_path(&path, false, true)?;
        let bytes = fs::read(&path)?;
        if hex_digest(&bytes) != object_id {
            return Err(AppError::InvalidProject(format!(
                "object `{object_id}` does not match its content address"
            )));
        }
        Ok(bytes)
    }

    pub fn complete_task_response<T: DeserializeOwned>(&self, task: &TaskState) -> Result<T> {
        if task.status != TaskStatus::Complete {
            return Err(AppError::InvalidProject(format!(
                "task `{}` is not complete",
                task.task.task_id
            )));
        }
        let object = task.response_object.as_deref().ok_or_else(|| {
            AppError::InvalidProject(format!(
                "complete task `{}` has no response object",
                task.task.task_id
            ))
        })?;
        Ok(serde_json::from_slice(&self.read_object(object)?)?)
    }

    pub fn latest_approval<T: DeserializeOwned>(
        &self,
        run_id: &str,
        approval_kind: &str,
        target_id: &str,
    ) -> Result<Option<ApprovalRecord<T>>> {
        validate_identifier("approval kind", approval_kind)?;
        validate_identifier("approval target", target_id)?;
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT target_hash,object_id FROM approvals_v3 \
                 WHERE run_id=?1 AND approval_kind=?2 AND target_id=?3 \
                 ORDER BY sequence DESC LIMIT 1",
                params![run_id, approval_kind, target_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(target_hash, object_id)| {
            Ok(ApprovalRecord {
                target_hash,
                value: serde_json::from_slice(&self.read_object(&object_id)?)?,
            })
        })
        .transpose()
    }

    pub fn recording_hashes(&self, run_id: &str) -> Result<Vec<ContentHash>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT recording_hash FROM exchanges_v3 WHERE run_id=?1 ORDER BY sequence")?;
        let hashes = statement
            .query_map(params![run_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        hashes
            .into_iter()
            .map(|hash| Ok(ContentHash::parse_hex(&hash)?))
            .collect()
    }

    pub fn store_approved_integration_plan(
        &self,
        run_id: &str,
        plan: &ApprovedIntegrationPlan,
    ) -> Result<String> {
        plan.validate()?;
        let bytes = okc_core::to_canonical_json(plan)?;
        let object_id = self.put_object(&bytes)?;
        let plan_hash = digest(b"okc:integration-plan-object:v3\0", &[&bytes]);
        self.connection()?.execute(
            "INSERT INTO integration_plans_v4(run_id,plan_id,plan_hash,object_id) \
             VALUES (?1,?2,?3,?4)",
            params![run_id, plan.integration_plan_id, plan_hash, object_id],
        )?;
        Ok(object_id)
    }

    pub fn latest_approved_integration_plan(&self) -> Result<Option<ApprovedIntegrationPlan>> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT p.plan_id,p.plan_hash,p.object_id,p.run_id FROM integration_plans_v4 p \
                 JOIN runs r ON r.run_id=p.run_id \
                 WHERE r.sequence=(SELECT MAX(sequence) FROM runs) \
                 ORDER BY p.sequence DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((plan_id, plan_hash, object_id, run_id)) = row else {
            return Ok(None);
        };
        {
            let bytes = self.read_object(&object_id)?;
            if digest(b"okc:integration-plan-object:v3\0", &[&bytes]) != plan_hash {
                return Err(AppError::InvalidProject(
                    "approved integration plan object hash is stale".into(),
                ));
            }
            let plan: ApprovedIntegrationPlan = serde_json::from_slice(&bytes)?;
            plan.validate()?;
            if plan.integration_plan_id != plan_id {
                return Err(AppError::InvalidProject(
                    "approved integration plan pointer does not match its object".into(),
                ));
            }
            if !self.integration_plan_is_current(&run_id, &plan)? {
                return Ok(None);
            }
            Ok(Some(plan))
        }
    }

    fn integration_plan_is_current(
        &self,
        run_id: &str,
        plan: &ApprovedIntegrationPlan,
    ) -> Result<bool> {
        let Some(taxonomy) =
            self.latest_approval::<ApprovedTaxonomy>(run_id, "taxonomy", "taxonomy")?
        else {
            return Ok(false);
        };
        if taxonomy.target_hash != plan.taxonomy.taxonomy_hash.hex()
            || taxonomy.value.taxonomy != plan.taxonomy
            || taxonomy.value.approval != plan.taxonomy_approval
            || taxonomy.value.corpus != plan.corpus
        {
            return Ok(false);
        }
        for revision in &plan.clusters {
            let cluster_id = &revision.proposal.cluster_id;
            let Some(approval) =
                self.latest_approval::<ApprovedClusterRevision>(run_id, "cluster", cluster_id)?
            else {
                return Ok(false);
            };
            if approval.target_hash != revision.critic.critic_hash.hex()
                || approval.value != *revision
            {
                return Ok(false);
            }
            if self
                .latest_cluster_regeneration(run_id, cluster_id)?
                .is_some_and(|request| request.revision > revision.proposal.revision)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn seal_latest_integration_plan(&self) -> Result<Option<ApprovedIntegrationPlan>> {
        let Some(run) = self.latest_run()? else {
            return Ok(None);
        };
        let Some(taxonomy) =
            self.latest_approval::<ApprovedTaxonomy>(&run.run_id, "taxonomy", "taxonomy")?
        else {
            return Ok(None);
        };
        if taxonomy.target_hash != taxonomy.value.taxonomy.taxonomy_hash.hex() {
            return Ok(None);
        }
        let mut clusters = Vec::new();
        for cluster in &taxonomy.value.taxonomy.clusters {
            let Some(approved) = self.latest_approval::<ApprovedClusterRevision>(
                &run.run_id,
                "cluster",
                &cluster.cluster_id,
            )?
            else {
                return Ok(None);
            };
            if approved.target_hash != approved.value.critic.critic_hash.hex()
                || approved.value.proposal.taxonomy_hash != taxonomy.value.taxonomy.taxonomy_hash
                || self
                    .latest_cluster_regeneration(&run.run_id, &cluster.cluster_id)?
                    .is_some_and(|request| request.revision > approved.value.proposal.revision)
            {
                return Ok(None);
            }
            clusters.push(approved.value);
        }
        let plan = ApprovedIntegrationPlan::seal(
            taxonomy.value.corpus,
            taxonomy.value.taxonomy,
            taxonomy.value.approval,
            clusters,
            self.recording_hashes(&run.run_id)?,
        )?;
        self.store_approved_integration_plan(&run.run_id, &plan)?;
        Ok(Some(plan))
    }

    pub fn record_verified_output(
        &self,
        path: &Path,
        manifest: &CompiledVaultManifest,
    ) -> Result<()> {
        let run = self.latest_run()?.ok_or_else(|| {
            AppError::InvalidProject("cannot record verification without a current run".into())
        })?;
        let approved = self.latest_approved_integration_plan()?;
        if approved.as_ref().is_none_or(|plan| {
            manifest.integration_plan_id != plan.integration_plan_id
                || manifest.corpus_hash != plan.corpus.corpus_hash
                || manifest.taxonomy_hash != plan.taxonomy.taxonomy_hash
        }) {
            return Err(AppError::InvalidProject(
                "verified artifact is stale for the current approved integration plan".into(),
            ));
        }
        let path = fs::canonicalize(path)?;
        let output_path = path.to_str().ok_or_else(|| {
            AppError::InvalidProject("verified output path must be valid UTF-8".into())
        })?;
        let bytes = okc_core::to_canonical_json(manifest)?;
        let object = self.put_object(&bytes)?;
        self.connection()?.execute(
            "INSERT INTO verified_outputs_v4(run_id,plan_id,output_path,manifest_object) \
             VALUES (?1,?2,?3,?4)",
            params![
                run.run_id,
                manifest.integration_plan_id,
                output_path,
                object
            ],
        )?;
        Ok(())
    }

    pub fn latest_verified_output(&self) -> Result<Option<PathBuf>> {
        let Some(plan) = self.latest_approved_integration_plan()? else {
            return Ok(None);
        };
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT v.output_path FROM verified_outputs_v4 v \
                 JOIN runs r ON r.run_id=v.run_id \
                 WHERE r.sequence=(SELECT MAX(sequence) FROM runs) AND v.plan_id=?1 \
                 ORDER BY v.sequence DESC LIMIT 1",
                [plan.integration_plan_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(PathBuf::from))
    }

    pub fn compile(
        &self,
        plan: &ApprovedIntegrationPlan,
        destination: impl AsRef<Path>,
    ) -> Result<CompiledArtifact> {
        let _lock = self.acquire_writer_lock()?;
        self.compile_locked(plan, destination.as_ref())
    }

    pub(crate) fn compile_locked(
        &self,
        plan: &ApprovedIntegrationPlan,
        destination: &Path,
    ) -> Result<CompiledArtifact> {
        let destination = crate::integration_service::absolute_output(destination)?;
        if self
            .manifest()
            .sources
            .iter()
            .any(|source| crate::workspace_bootstrap::paths_overlap(&destination, &source.path))
        {
            return Err(AppError::InvalidProject(
                "compiled output must be outside every source".into(),
            ));
        }
        Ok(compile(plan, &destination)?)
    }

    fn task_state(&self, task_id: &str) -> Result<TaskState> {
        let connection = self.connection()?;
        let task = connection.query_row(
            "SELECT run_id,stage,cache_key,request_object FROM task_definitions WHERE task_id=?1",
            params![task_id],
            |row| {
                let stage: String = row.get(1)?;
                Ok(TaskDefinition {
                    task_id: task_id.into(),
                    run_id: row.get(0)?,
                    stage: parse_stage(&stage)?,
                    cache_key: row.get(2)?,
                    request_object: row.get(3)?,
                })
            },
        )?;
        let (status, response_object, error_kind) = connection.query_row(
            "SELECT status,response_object,error_kind FROM task_events \
             WHERE task_id=?1 ORDER BY sequence DESC LIMIT 1",
            params![task_id],
            |row| {
                let status: String = row.get(0)?;
                Ok((parse_status(&status)?, row.get(1)?, row.get(2)?))
            },
        )?;
        Ok(TaskState {
            task,
            status,
            response_object,
            error_kind,
        })
    }

    pub(crate) fn connection(&self) -> Result<Connection> {
        super::validate_managed_path(&self.root, true, true)?;
        super::validate_database_paths(&self.root.join("state.sqlite3"), true)?;
        let connection = Connection::open(self.root.join("state.sqlite3"))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(connection)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCacheKey {
    pub stage: TaskStage,
    pub prompt_hash: String,
    pub schema_hash: String,
    pub source_hash: String,
    pub provider: String,
    pub model: String,
    pub adapter: String,
    pub options_hash: String,
}

impl TaskCacheKey {
    pub fn hash(&self) -> Result<String> {
        for (label, value) in [
            ("prompt hash", self.prompt_hash.as_str()),
            ("schema hash", self.schema_hash.as_str()),
            ("source hash", self.source_hash.as_str()),
            ("options hash", self.options_hash.as_str()),
        ] {
            validate_hex_hash(label, value)?;
        }
        for (label, value) in [
            ("provider", self.provider.as_str()),
            ("model", self.model.as_str()),
            ("adapter", self.adapter.as_str()),
        ] {
            validate_identifier(label, value)?;
        }
        let bytes = serde_json::to_vec(self)?;
        Ok(digest(b"okc:ai-task:v3\0", &[&bytes]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveCategory {
    PrivateKey,
    ApiCredential,
    CloudCredential,
    ConnectionString,
    EmailAddress,
    PhoneNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitiveFinding {
    pub scanner_version: String,
    pub category: SensitiveCategory,
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub byte_start: u64,
    pub byte_end: u64,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitiveException {
    pub category: SensitiveCategory,
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub byte_start: u64,
    pub byte_end: u64,
    pub content_hash: ContentHash,
    pub curator_id: String,
    pub rationale: String,
}

pub const SENSITIVE_SCANNER_VERSION: &str = "okc-sensitive-v3-2";

#[derive(Debug, Serialize)]
pub(crate) struct SensitiveScan {
    blocks: Vec<SensitiveFinding>,
    metadata: Vec<SensitiveMetadataFinding>,
}

#[derive(Debug, Serialize)]
struct SensitiveMetadataFinding {
    scanner_version: String,
    category: SensitiveCategory,
    document_id: DocumentId,
    metadata_id: String,
    byte_start: u64,
    byte_end: u64,
    content_hash: ContentHash,
}

impl SensitiveScan {
    pub(crate) fn scan(corpus: &IntegrationCorpus) -> Result<Self> {
        let mut result = Self {
            blocks: Vec::new(),
            metadata: Vec::new(),
        };
        for document in &corpus.documents {
            for block in &document.blocks {
                result.blocks.extend(scan_sensitive_block(
                    document.document_id,
                    block.block_id,
                    &block.text,
                ));
            }
            for metadata in &document.metadata {
                let text = serde_json::to_string(
                    &serde_json::json!({"key": metadata.key, "value": metadata.value}),
                )?;
                for (category, start, end) in scan_sensitive_ranges(&text) {
                    result.metadata.push(SensitiveMetadataFinding {
                        scanner_version: SENSITIVE_SCANNER_VERSION.into(),
                        category,
                        document_id: document.document_id,
                        metadata_id: metadata.metadata_id.clone(),
                        byte_start: start as u64,
                        byte_end: end as u64,
                        content_hash: ContentHash::from_domain_bytes(
                            "okc:sensitive-span:v3\0",
                            &text.as_bytes()[start..end],
                        ),
                    });
                }
            }
        }
        Ok(result)
    }

    pub(crate) fn len(&self) -> usize {
        self.blocks.len() + self.metadata.len()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn authorize(
        &self,
        role: AiRole,
        profile_name: &str,
        capabilities: &ProviderCapabilities,
        disclosed_documents: &BTreeSet<DocumentId>,
        allow_remote_provider: bool,
        yes: bool,
        non_interactive: bool,
    ) -> Result<DisclosureAuthorization> {
        let global = matches!(role, AiRole::Embedding | AiRole::Organizer);
        if capabilities.data_boundary == DataBoundary::Remote
            && self
                .metadata
                .iter()
                .any(|finding| global || disclosed_documents.contains(&finding.document_id))
        {
            return Err(AppError::InvalidProject(
                "sensitive content requires a local provider for this role".into(),
            ));
        }
        let blocks = self
            .blocks
            .iter()
            .filter(|finding| disclosed_documents.contains(&finding.document_id))
            .map(|finding| finding.block_id)
            .collect();
        authorize_disclosure(
            role,
            profile_name,
            capabilities,
            &self.blocks,
            &blocks,
            allow_remote_provider,
            yes,
            non_interactive,
        )
    }
}

/// Deterministic preflight. Findings retain only category, location, and a
/// domain-separated content hash; matched secret text is never returned.
pub fn scan_sensitive_block(
    document_id: DocumentId,
    block_id: BlockId,
    text: &str,
) -> Vec<SensitiveFinding> {
    scan_sensitive_ranges(text)
        .into_iter()
        .map(|(category, start, end)| SensitiveFinding {
            scanner_version: SENSITIVE_SCANNER_VERSION.into(),
            category,
            document_id,
            block_id,
            byte_start: start as u64,
            byte_end: end as u64,
            content_hash: ContentHash::from_domain_bytes(
                "okc:sensitive-span:v3\0",
                &text.as_bytes()[start..end],
            ),
        })
        .collect()
}

fn scan_sensitive_ranges(text: &str) -> Vec<(SensitiveCategory, usize, usize)> {
    let mut candidates = Vec::new();
    for (category, marker) in [
        (SensitiveCategory::PrivateKey, "-----BEGIN PRIVATE KEY-----"),
        (
            SensitiveCategory::PrivateKey,
            "-----BEGIN OPENSSH PRIVATE KEY-----",
        ),
        (SensitiveCategory::ApiCredential, "sk-"),
        (SensitiveCategory::ApiCredential, "ghp_"),
        (SensitiveCategory::CloudCredential, "AKIA"),
        (SensitiveCategory::ConnectionString, "postgres://"),
        (SensitiveCategory::ConnectionString, "mongodb://"),
    ] {
        for (start, _) in text.match_indices(marker) {
            let end = token_end(text, start, marker.len(), category);
            if end > start + marker.len() || category == SensitiveCategory::PrivateKey {
                candidates.push((category, start, end));
            }
        }
    }
    candidates.extend(scan_email_like(text));
    candidates.extend(scan_phone_like(text));
    candidates.sort();
    candidates.dedup();
    candidates
}

fn token_end(text: &str, start: usize, marker_len: usize, category: SensitiveCategory) -> usize {
    if category == SensitiveCategory::PrivateKey {
        return (start + marker_len).min(text.len());
    }
    text[start + marker_len..]
        .char_indices()
        .find(|(_, character)| {
            character.is_whitespace() || matches!(character, '"' | '\'' | '`' | '<' | '>')
        })
        .map_or(text.len(), |(offset, _)| start + marker_len + offset)
}

fn scan_email_like(text: &str) -> Vec<(SensitiveCategory, usize, usize)> {
    text.split_inclusive(char::is_whitespace)
        .scan(0_usize, |offset, token| {
            let start = *offset;
            *offset += token.len();
            Some((start, token.trim()))
        })
        .filter_map(|(start, token)| {
            let clean = token.trim_matches(|character: char| {
                matches!(character, ',' | '.' | ';' | ':' | '(' | ')' | '<' | '>')
            });
            let at = clean.find('@')?;
            let domain = &clean[at + 1..];
            let start = start + token.find(clean)?;
            (at > 0 && domain.contains('.') && !domain.ends_with('.')).then_some((
                SensitiveCategory::EmailAddress,
                start,
                start + clean.len(),
            ))
        })
        .collect()
}

fn scan_phone_like(text: &str) -> Vec<(SensitiveCategory, usize, usize)> {
    let mut results = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() && bytes[start] != b'+' {
            start += 1;
            continue;
        }
        let mut end = start;
        let mut digits = 0;
        while end < bytes.len()
            && (bytes[end].is_ascii_digit()
                || matches!(bytes[end], b'+' | b'-' | b' ' | b'(' | b')'))
        {
            if bytes[end].is_ascii_digit() {
                digits += 1;
            }
            end += 1;
        }
        if digits >= 10 {
            results.push((SensitiveCategory::PhoneNumber, start, end));
        }
        start = end.max(start + 1);
    }
    results
}

pub fn effective_sensitive_findings(
    findings: &[SensitiveFinding],
    exceptions: &[SensitiveException],
) -> Result<Vec<SensitiveFinding>> {
    let exception_keys = exceptions
        .iter()
        .map(|exception| {
            if exception.curator_id.trim().is_empty() || exception.rationale.trim().is_empty() {
                return Err(AppError::InvalidProject(
                    "sensitive-data exceptions require curator and rationale".into(),
                ));
            }
            Ok((
                exception.category,
                exception.document_id,
                exception.block_id,
                exception.byte_start,
                exception.byte_end,
                exception.content_hash,
            ))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    Ok(findings
        .iter()
        .filter(|finding| {
            !exception_keys.contains(&(
                finding.category,
                finding.document_id,
                finding.block_id,
                finding.byte_start,
                finding.byte_end,
                finding.content_hash,
            ))
        })
        .cloned()
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisclosureAuthorization {
    pub profile_name: String,
    pub role: AiRole,
    pub boundary: DataBoundary,
}

#[allow(clippy::too_many_arguments)]
pub fn authorize_disclosure(
    role: AiRole,
    profile_name: &str,
    capabilities: &ProviderCapabilities,
    findings: &[SensitiveFinding],
    disclosed_blocks: &BTreeSet<BlockId>,
    allow_remote_provider: bool,
    yes: bool,
    non_interactive: bool,
) -> Result<DisclosureAuthorization> {
    if capabilities.schema_version != 3 {
        return Err(AppError::InvalidProject(
            "provider capabilities are not schema 3".into(),
        ));
    }
    let sensitive_disclosure = findings
        .iter()
        .any(|finding| disclosed_blocks.contains(&finding.block_id));
    let all_semantics_sensitive =
        !findings.is_empty() && matches!(role, AiRole::Embedding | AiRole::Organizer);
    if capabilities.data_boundary == DataBoundary::Remote
        && (sensitive_disclosure || all_semantics_sensitive)
    {
        return Err(AppError::InvalidProject(
            "sensitive content requires a local provider for this role".into(),
        ));
    }
    if capabilities.data_boundary == DataBoundary::Remote
        && (!allow_remote_provider || (non_interactive && !yes))
    {
        return Err(AppError::InvalidProject(
            "remote disclosure requires --allow-remote-provider and non-interactive runs also require --yes"
                .into(),
        ));
    }
    Ok(DisclosureAuthorization {
        profile_name: profile_name.into(),
        role,
        boundary: capabilities.data_boundary,
    })
}

pub fn write_approved_plan_new(
    path: impl AsRef<Path>,
    plan: &ApprovedIntegrationPlan,
) -> Result<()> {
    plan.validate()?;
    let path = path.as_ref();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, plan)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn parse_stage(value: &str) -> rusqlite::Result<TaskStage> {
    match value {
        "sensitive_preflight" => Ok(TaskStage::SensitivePreflight),
        "embedding" => Ok(TaskStage::Embedding),
        "semantic_candidates" => Ok(TaskStage::SemanticCandidates),
        "organizer" => Ok(TaskStage::Organizer),
        "synthesis" => Ok(TaskStage::Synthesis),
        "critic" => Ok(TaskStage::Critic),
        "materialization" => Ok(TaskStage::Materialization),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn parse_status(value: &str) -> rusqlite::Result<TaskStatus> {
    match value {
        "queued" => Ok(TaskStatus::Queued),
        "running" => Ok(TaskStatus::Running),
        "complete" => Ok(TaskStatus::Complete),
        "failed" => Ok(TaskStatus::Failed),
        "cancelled" => Ok(TaskStatus::Cancelled),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn validate_hex_hash(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(AppError::InvalidProject(format!(
            "{label} must be a lowercase SHA-256 value"
        )));
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 512
        || value.chars().any(char::is_control)
        || value.contains(['\n', '\r'])
    {
        return Err(AppError::InvalidProject(format!("invalid {label}")));
    }
    Ok(())
}

fn digest(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((*part).len().to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use okc_ai::ProviderIdentity;

    use super::*;

    fn hash(label: &str) -> String {
        digest(b"okc:test:v3\0", &[label.as_bytes()])
    }

    fn approved_project() -> (
        tempfile::TempDir,
        ProjectStore,
        RunRecord,
        ApprovedIntegrationPlan,
    ) {
        let temporary = tempfile::tempdir().expect("temporary");
        let project = ProjectStore::create(
            temporary.path().join("Approved.okc-project"),
            "Approved",
            "sdk-test",
            "policy-v3",
        )
        .expect("project");
        let plan: ApprovedIntegrationPlan = serde_json::from_slice(include_bytes!(
            "../../okc-core/tests/fixtures/sdk-integration-plan.json"
        ))
        .expect("approved fixture");
        let run = project
            .begin_or_resume_run(&plan.corpus.corpus_hash.hex(), &hash("config"))
            .expect("run");
        let taxonomy = ApprovedTaxonomy {
            corpus: plan.corpus.clone(),
            taxonomy: plan.taxonomy.clone(),
            approval: plan.taxonomy_approval.clone(),
        };
        project
            .append_approval(
                &run.run_id,
                "taxonomy",
                "taxonomy",
                &plan.taxonomy.taxonomy_hash.hex(),
                &okc_core::to_canonical_json(&taxonomy).expect("taxonomy JSON"),
            )
            .expect("taxonomy approval");
        for cluster in &plan.clusters {
            project
                .append_approval(
                    &run.run_id,
                    "cluster",
                    &cluster.proposal.cluster_id,
                    &cluster.critic.critic_hash.hex(),
                    &okc_core::to_canonical_json(cluster).expect("cluster JSON"),
                )
                .expect("cluster approval");
        }
        project
            .store_approved_integration_plan(&run.run_id, &plan)
            .expect("stored plan");
        (temporary, project, run, plan)
    }

    #[test]
    fn verified_outputs_are_bound_to_the_current_approved_plan() {
        let (temporary, mut project, _run, plan) = approved_project();
        let output = temporary.path().join("compiled");
        project.compile(&plan, &output).expect("compile");
        let manifest = okc_core::integration::verify(&output).expect("verify");
        project
            .record_verified_output(&output, &manifest)
            .expect("record");
        assert_eq!(
            project.latest_verified_output().expect("latest"),
            Some(fs::canonicalize(&output).expect("canonical output"))
        );
        let mut foreign = manifest.clone();
        foreign.integration_plan_id = "plan_foreign".into();
        assert!(project.record_verified_output(&output, &foreign).is_err());
        project
            .set_language(Some("ko-KR".into()))
            .expect("language");
        assert!(
            project
                .latest_verified_output()
                .expect("stale output")
                .is_none()
        );
        assert!(project.record_verified_output(&output, &manifest).is_err());
    }

    #[test]
    fn changed_configuration_invalidates_approved_authority_immediately() {
        for change_language in [true, false] {
            let (_temporary, mut project, run, _plan) = approved_project();
            assert!(
                project
                    .latest_approved_integration_plan()
                    .expect("plan")
                    .is_some()
            );
            if change_language {
                project
                    .set_language(Some("ko-KR".into()))
                    .expect("language");
            } else {
                project
                    .set_ai_route(None, "different-provider".into())
                    .expect("route");
            }
            assert!(
                project
                    .latest_approved_integration_plan()
                    .expect("stale plan")
                    .is_none()
            );
            assert_ne!(
                project
                    .latest_run()
                    .expect("run")
                    .expect("current run")
                    .run_id,
                run.run_id
            );
            let preserved: u64 = project
                .connection()
                .expect("connection")
                .query_row(
                    "SELECT COUNT(*) FROM integration_plans_v4 WHERE run_id=?1",
                    [run.run_id],
                    |row| row.get(0),
                )
                .expect("historical plans");
            assert_eq!(preserved, 1);
        }
    }

    #[test]
    fn failed_invalidation_keeps_manifest_and_in_memory_configuration_unchanged() {
        for change_language in [true, false] {
            let (_temporary, mut project, _run, _plan) = approved_project();
            let before = project.manifest().clone();
            let before_bytes = fs::read(project.root().join("manifest.json")).unwrap();
            project
                .connection()
                .unwrap()
                .execute_batch(
                    "CREATE TRIGGER reject_invalidation BEFORE INSERT ON project_events \
                 WHEN NEW.kind='source_invalidation' \
                 BEGIN SELECT RAISE(ABORT, 'injected journal failure'); END;",
                )
                .unwrap();
            let result = if change_language {
                project.set_language(Some("ko-KR".into()))
            } else {
                project.set_ai_route(None, "different-provider".into())
            };
            assert!(result.is_err());
            assert_eq!(project.manifest(), &before);
            assert_eq!(
                fs::read(project.root().join("manifest.json")).unwrap(),
                before_bytes
            );
            assert!(
                project
                    .latest_approved_integration_plan()
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[test]
    fn failed_manifest_replacement_leaves_previous_approvals_invalidated() {
        for change_language in [true, false] {
            let (_temporary, mut project, run, _plan) = approved_project();
            let before = project.manifest().clone();
            let manifest = project.root().join("manifest.json");
            let saved = project.root().join("manifest.saved");
            fs::rename(&manifest, &saved).unwrap();
            fs::create_dir(&manifest).unwrap();
            let result = if change_language {
                project.set_language(Some("ko-KR".into()))
            } else {
                project.set_ai_route(None, "different-provider".into())
            };
            assert!(result.is_err());
            assert_eq!(project.manifest(), &before);
            assert_ne!(project.latest_run().unwrap().unwrap().run_id, run.run_id);
            assert!(
                project
                    .latest_approved_integration_plan()
                    .unwrap()
                    .is_none()
            );
            fs::remove_dir(&manifest).unwrap();
            fs::rename(&saved, &manifest).unwrap();
            let reopened = ProjectStore::open(project.root()).unwrap();
            assert_eq!(reopened.manifest(), &before);
            assert!(
                reopened
                    .latest_approved_integration_plan()
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn failed_source_manifest_replacement_cannot_resurrect_previous_plan() {
        let (temporary, mut project, run, _plan) = approved_project();
        let before = project.manifest().clone();
        let source = temporary.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("new.md"), "# New source\n").unwrap();
        let manifest = project.root().join("manifest.json");
        let saved = project.root().join("manifest.saved");
        fs::rename(&manifest, &saved).unwrap();
        fs::create_dir(&manifest).unwrap();
        let result = project.replace_sources_explicit(vec![crate::SourceBinding {
            source_id: okc_core::SourceId::new("new").unwrap(),
            owner_display_name: None,
            path: source,
            snapshot_id: None,
        }]);
        assert!(result.is_err());
        assert_eq!(project.manifest(), &before);
        assert_ne!(project.latest_run().unwrap().unwrap().run_id, run.run_id);
        assert!(
            project
                .latest_approved_integration_plan()
                .unwrap()
                .is_none()
        );
        fs::remove_dir(&manifest).unwrap();
        fs::rename(&saved, &manifest).unwrap();
        let reopened = ProjectStore::open(project.root()).unwrap();
        assert_eq!(reopened.manifest(), &before);
        assert!(
            reopened
                .latest_approved_integration_plan()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn changed_taxonomy_and_pending_regeneration_invalidate_stored_plans() {
        let (_temporary, project, run, plan) = approved_project();
        let cluster = &plan.clusters[0];
        let request = ClusterRegenerationRequest {
            cluster_id: cluster.proposal.cluster_id.clone(),
            revision: 2,
            feedback: "retain more context".into(),
            feedback_hash: cluster_feedback_hash(
                &cluster.proposal.cluster_id,
                2,
                "retain more context",
                cluster.proposal.proposal_hash,
                cluster.critic.critic_hash,
            )
            .expect("feedback hash"),
            previous_proposal_hash: cluster.proposal.proposal_hash,
            previous_critic_hash: cluster.critic.critic_hash,
        };
        project
            .append_cluster_regeneration(&request)
            .expect("regeneration");
        assert!(
            project
                .latest_approved_integration_plan()
                .expect("stale plan")
                .is_none()
        );
        assert!(
            project
                .seal_latest_integration_plan()
                .expect("pending regeneration")
                .is_none()
        );
        assert!(
            project
                .latest_approval::<ApprovedClusterRevision>(
                    &run.run_id,
                    "cluster",
                    &cluster.proposal.cluster_id,
                )
                .expect("historical approval")
                .is_some()
        );

        let (_temporary, project, run, plan) = approved_project();
        let mut clusters = plan.taxonomy.clusters.clone();
        clusters[0].title = "New taxonomy title".into();
        let taxonomy = TaxonomyProposal::seal(
            &plan.corpus,
            clusters,
            plan.taxonomy.organizer_recording_hash,
        )
        .expect("new taxonomy");
        let approved = ApprovedTaxonomy {
            corpus: plan.corpus.clone(),
            approval: ApprovalBinding {
                target_hash: taxonomy.taxonomy_hash,
                ..plan.taxonomy_approval
            },
            taxonomy,
        };
        project
            .append_approval(
                &run.run_id,
                "taxonomy",
                "taxonomy",
                &approved.taxonomy.taxonomy_hash.hex(),
                &okc_core::to_canonical_json(&approved).expect("taxonomy JSON"),
            )
            .expect("changed taxonomy");
        assert!(
            project
                .latest_approved_integration_plan()
                .expect("stale plan")
                .is_none()
        );
    }

    #[test]
    fn scanner_detects_pem_keys_and_preserves_exact_email_byte_ranges() {
        let document = DocumentId::from_hash(ContentHash::from_domain_bytes("test", b"doc"));
        let block = BlockId::from_hash(ContentHash::from_domain_bytes("test", b"block"));
        for text in [
            "-----BEGIN PRIVATE KEY-----\nABC\n-----END PRIVATE KEY-----",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nABC",
        ] {
            assert!(
                scan_sensitive_block(document, block, text)
                    .iter()
                    .any(|finding| finding.category == SensitiveCategory::PrivateKey)
            );
        }
        let text = "한글 (<person@example.com>),";
        let findings = scan_sensitive_block(document, block, text);
        let email = findings
            .iter()
            .find(|finding| finding.category == SensitiveCategory::EmailAddress)
            .expect("email finding");
        assert_eq!(
            &text[usize::try_from(email.byte_start).expect("start")
                ..usize::try_from(email.byte_end).expect("end")],
            "person@example.com"
        );
        assert_eq!(
            email.content_hash,
            ContentHash::from_domain_bytes("okc:sensitive-span:v3\0", b"person@example.com")
        );
    }

    #[test]
    fn metadata_only_sensitive_documents_force_local_routes_without_recording_secrets() {
        let (_temporary, _project, _run, plan) = approved_project();
        let mut corpus = plan.corpus;
        let document = &mut corpus.documents[0];
        document.blocks.clear();
        document.metadata = vec![
            okc_core::integration::MetadataValue::new(
                document.document_id,
                "credential",
                0,
                &serde_json::json!("sk-fixture-metadata-secret"),
            )
            .expect("metadata"),
        ];
        let scan = SensitiveScan::scan(&corpus).expect("scan");
        assert!(scan.blocks.is_empty());
        assert_eq!(scan.metadata.len(), 1);
        let encoded = serde_json::to_string(&scan).expect("recording");
        assert!(!encoded.contains("sk-fixture-metadata-secret"));
        assert!(encoded.contains(&corpus.documents[0].metadata[0].metadata_id));
        let mut remote = okc_ai::ProviderClient::new(okc_ai::ProviderProfile {
            kind: okc_ai::ProviderKind::Ollama,
            endpoint: "https://provider.invalid".into(),
            model: "fixture".into(),
            api_key_env: None,
            os_keychain: None,
            timeout_ms: 1,
            max_response_bytes: 1024,
            max_input_bytes: 1024,
            max_batch_items: 1,
            options: std::collections::BTreeMap::default(),
        })
        .map(|client| okc_ai::StructuredGenerator::capabilities(&client))
        .expect("capabilities");
        let documents = BTreeSet::from([corpus.documents[0].document_id]);
        for role in [
            AiRole::Embedding,
            AiRole::Organizer,
            AiRole::Synthesis,
            AiRole::Critic,
        ] {
            assert!(
                scan.authorize(role, "remote", &remote, &documents, true, true, true)
                    .is_err()
            );
        }
        assert!(
            scan.authorize(
                AiRole::Synthesis,
                "remote",
                &remote,
                &BTreeSet::new(),
                true,
                true,
                true
            )
            .is_ok()
        );
        remote.data_boundary = DataBoundary::Local;
        assert!(
            scan.authorize(
                AiRole::Synthesis,
                "local",
                &remote,
                &documents,
                false,
                false,
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn journal_resumes_complete_tasks_without_deleting_history() {
        let temporary = tempfile::tempdir().expect("temporary");
        let root = temporary.path().join("Journal.okc-project");
        let project =
            ProjectStore::create(&root, "Journal", "curator", "policy-v3").expect("project");
        let run = project
            .begin_or_resume_run(&hash("input"), &hash("config"))
            .expect("run");
        let key = TaskCacheKey {
            stage: TaskStage::Embedding,
            prompt_hash: hash("prompt"),
            schema_hash: hash("schema"),
            source_hash: hash("source"),
            provider: "ollama".into(),
            model: "fixture".into(),
            adapter: "okc-ai-0.3".into(),
            options_hash: hash("options"),
        };
        let first = project
            .register_task(&run, TaskStage::Embedding, &key, b"request")
            .expect("task");
        project
            .append_task_status(&first.task.task_id, TaskStatus::Running, None, None)
            .expect("running");
        let complete = project
            .append_task_status(
                &first.task.task_id,
                TaskStatus::Complete,
                Some(b"response"),
                None,
            )
            .expect("complete");
        let resumed = project
            .register_task(&run, TaskStage::Embedding, &key, b"request")
            .expect("resumed task");
        assert_eq!(resumed, complete);
        assert!(
            project
                .register_task(&run, TaskStage::Embedding, &key, b"different request")
                .is_err()
        );
        assert!(
            project
                .register_task(&run, TaskStage::Critic, &key, b"request")
                .is_err()
        );
        let status = project.integration_status().expect("status");
        assert_eq!(status.completed, 1);
        assert_eq!(status.tasks.len(), 1);
    }

    #[test]
    fn source_change_appends_invalidation_instead_of_deleting_runs() {
        let temporary = tempfile::tempdir().expect("temporary");
        let root = temporary.path().join("Append.okc-project");
        let mut project =
            ProjectStore::create(&root, "Append", "curator", "policy-v3").expect("project");
        let old = project
            .begin_or_resume_run(&hash("old-input"), &hash("config"))
            .expect("old run");
        let source_path = temporary.path().join("alpha");
        fs::create_dir(&source_path).expect("source directory");
        fs::write(source_path.join("note.md"), b"source").expect("source note");
        project
            .add_source(crate::SourceBinding {
                source_id: okc_core::SourceId::new("alpha").expect("source"),
                owner_display_name: None,
                path: source_path,
                snapshot_id: None,
            })
            .expect("source added");
        let new = project
            .begin_or_resume_run(&hash("new-input"), &hash("config"))
            .expect("new run");
        assert_ne!(old.run_id, new.run_id);
        let connection = project.connection().expect("connection");
        let count: u64 = connection
            .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 3);
    }

    #[test]
    fn regeneration_feedback_is_append_only_and_hash_bound() {
        let temporary = tempfile::tempdir().expect("temporary");
        let root = temporary.path().join("Feedback.okc-project");
        let project =
            ProjectStore::create(&root, "Feedback", "curator", "policy-v3").expect("project");
        let run = project
            .begin_or_resume_run(&hash("input"), &hash("config"))
            .expect("run");
        let proposal = ContentHash::from_domain_bytes("proposal", b"one");
        let critic = ContentHash::from_domain_bytes("critic", b"one");
        let feedback_hash =
            cluster_feedback_hash("cluster-a", 2, "add missing evidence", proposal, critic)
                .expect("feedback hash");
        let request = ClusterRegenerationRequest {
            cluster_id: "cluster-a".into(),
            revision: 2,
            feedback: "add missing evidence".into(),
            feedback_hash,
            previous_proposal_hash: proposal,
            previous_critic_hash: critic,
        };
        project
            .append_cluster_regeneration(&request)
            .expect("append feedback");
        assert_eq!(
            project
                .latest_cluster_regeneration(&run.run_id, "cluster-a")
                .expect("latest feedback"),
            Some(request.clone())
        );
        let mut forged = request;
        forged.feedback.push_str(" changed");
        assert!(project.append_cluster_regeneration(&forged).is_err());
        let version: u32 = project
            .connection()
            .expect("connection")
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version");
        assert_eq!(version, crate::APPLICATION_STATE_SCHEMA_VERSION);
    }

    #[test]
    fn scanner_records_only_location_category_and_hash() {
        let findings = scan_sensitive_block(
            DocumentId::from_hash(ContentHash::from_domain_bytes("test", b"doc")),
            BlockId::from_hash(ContentHash::from_domain_bytes("test", b"block")),
            "credential sk-super-secret-value and person@example.com",
        );
        assert_eq!(findings.len(), 2);
        let encoded = serde_json::to_string(&findings).expect("findings");
        assert!(!encoded.contains("super-secret"));
        assert!(!encoded.contains("person@example.com"));
    }

    #[test]
    fn sensitive_findings_force_local_semantic_routes() {
        let document_id = DocumentId::from_hash(ContentHash::from_domain_bytes("test", b"doc"));
        let block_id = BlockId::from_hash(ContentHash::from_domain_bytes("test", b"block"));
        let findings = scan_sensitive_block(document_id, block_id, "secret sk-example-value");
        let remote = ProviderCapabilities {
            schema_version: 3,
            identity: ProviderIdentity {
                provider: "openai".into(),
                model: "fixture".into(),
                adapter_version: "0.3".into(),
                response_model: None,
            },
            structured_generation: true,
            embeddings: true,
            strict_json_schema: true,
            data_boundary: DataBoundary::Remote,
            max_input_bytes: 1024,
            max_output_bytes: 1024,
            max_batch_items: 8,
        };
        assert!(
            authorize_disclosure(
                AiRole::Embedding,
                "remote",
                &remote,
                &findings,
                &BTreeSet::new(),
                true,
                true,
                true,
            )
            .is_err()
        );
        assert!(
            authorize_disclosure(
                AiRole::Synthesis,
                "remote",
                &remote,
                &findings,
                &BTreeSet::from([block_id]),
                true,
                true,
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn project_routes_use_default_and_validate_language() {
        let mut routes = AiRouteConfig::default();
        routes
            .set(Some(AiRole::Critic), "critic-local".into())
            .expect("route");
        assert_eq!(routes.profile_for(AiRole::Embedding), "default");
        assert_eq!(routes.profile_for(AiRole::Critic), "critic-local");
        let unchanged = routes.clone();
        assert!(routes.set(None, "invalid profile".into()).is_err());
        assert_eq!(routes, unchanged);
        assert!(
            routes
                .set(Some(AiRole::Critic), "invalid profile".into())
                .is_err()
        );
        assert_eq!(routes, unchanged);
        validate_bcp47("ko-KR").expect("language");
        assert!(validate_bcp47("ko--KR").is_err());
    }
}
