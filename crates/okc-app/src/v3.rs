//! Schema-3 application services, append-only run journal, and disclosure gate.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use okc_ai::{AiRole, DataBoundaryV3, ProviderCapabilitiesV3};
use okc_core::identity::{BlockId, ContentHash, DocumentId};
use okc_core::integration::{
    ApprovalBinding, ApprovedClusterRevision, ApprovedIntegrationPlan, CriticReport,
    IntegrationCorpus, SynthesisProposal, TaxonomyProposal, V3CompiledArtifact, V3Manifest,
    compile_approved_integration,
};
use rusqlite::{Connection, OptionalExtension as _, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{
    AppError, PROJECT_SCHEMA_VERSION, ProjectStore, Result, SourceBinding, hex_digest,
    validate_project_path,
};

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
    hasher.update(okc_core::canonical::to_canonical_json(&(
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
        match role {
            None => self.default = profile,
            Some(AiRole::Embedding) => self.embedding = Some(profile),
            Some(AiRole::Organizer) => self.organizer = Some(profile),
            Some(AiRole::Synthesis) => self.synthesis = Some(profile),
            Some(AiRole::Critic) => self.critic = Some(profile),
        }
        self.validate()
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
        let object = self.put_object(&okc_core::canonical::to_canonical_json(request)?)?;
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
        self.manifest.language = language;
        self.save_manifest()?;
        self.invalidate_downstream("project language changed")
    }

    pub fn set_ai_route(&mut self, role: Option<AiRole>, profile: String) -> Result<()> {
        let _lock = self.acquire_writer_lock()?;
        self.manifest.ai_routes.set(role, profile)?;
        self.save_manifest()?;
        self.invalidate_downstream("AI route changed")
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
            return self.task_state(&task_id);
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
            "SELECT task_id FROM task_definitions WHERE run_id=?1 AND stage=?2 ORDER BY task_id",
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
        let path = self.root.join("objects").join(object_id);
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
        let bytes = okc_core::canonical::to_canonical_json(plan)?;
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
                "SELECT p.plan_id,p.plan_hash,p.object_id FROM integration_plans_v4 p \
                 JOIN runs r ON r.run_id=p.run_id \
                 WHERE r.sequence=(SELECT MAX(sequence) FROM runs) \
                 ORDER BY p.sequence DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(plan_id, plan_hash, object_id)| {
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
            Ok(plan)
        })
        .transpose()
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

    pub fn record_verified_output(&self, path: &Path, manifest: &V3Manifest) -> Result<()> {
        let run = self.latest_run()?.ok_or_else(|| {
            AppError::InvalidProject("cannot record verification without a current run".into())
        })?;
        if manifest.integration_plan_id.is_empty() {
            return Err(AppError::InvalidProject(
                "verified manifest has no integration plan identity".into(),
            ));
        }
        let bytes = okc_core::canonical::to_canonical_json(manifest)?;
        let object = self.put_object(&bytes)?;
        self.connection()?.execute(
            "INSERT INTO verified_outputs_v4(run_id,plan_id,output_path,manifest_object) \
             VALUES (?1,?2,?3,?4)",
            params![
                run.run_id,
                manifest.integration_plan_id,
                path.to_string_lossy(),
                object
            ],
        )?;
        Ok(())
    }

    pub fn latest_verified_output(&self) -> Result<Option<PathBuf>> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT v.output_path FROM verified_outputs_v4 v \
                 JOIN runs r ON r.run_id=v.run_id \
                 WHERE r.sequence=(SELECT MAX(sequence) FROM runs) \
                 ORDER BY v.sequence DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(PathBuf::from))
    }

    pub fn compile_v3(
        &self,
        plan: &ApprovedIntegrationPlan,
        destination: impl AsRef<Path>,
    ) -> Result<V3CompiledArtifact> {
        let _lock = self.acquire_writer_lock()?;
        Ok(compile_approved_integration(plan, destination)?)
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

    fn connection(&self) -> Result<Connection> {
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

pub const SENSITIVE_SCANNER_VERSION: &str = "okc-sensitive-v3-1";

/// Deterministic preflight. Findings retain only category, location, and a
/// domain-separated content hash; matched secret text is never returned.
pub fn scan_sensitive_block(
    document_id: DocumentId,
    block_id: BlockId,
    text: &str,
) -> Vec<SensitiveFinding> {
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
            if end > start + marker.len() {
                candidates.push((category, start, end));
            }
        }
    }
    candidates.extend(scan_email_like(text));
    candidates.extend(scan_phone_like(text));
    candidates.sort();
    candidates.dedup();
    candidates
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
    pub boundary: DataBoundaryV3,
}

#[allow(clippy::too_many_arguments)]
pub fn authorize_disclosure(
    role: AiRole,
    profile_name: &str,
    capabilities: &ProviderCapabilitiesV3,
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
    if capabilities.data_boundary == DataBoundaryV3::Remote
        && (sensitive_disclosure || all_semantics_sensitive)
    {
        return Err(AppError::InvalidProject(
            "sensitive content requires a local provider for this role".into(),
        ));
    }
    if capabilities.data_boundary == DataBoundaryV3::Remote
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyV2ProjectManifest {
    format_family: String,
    schema_version: u32,
    #[serde(rename = "product_version")]
    _product_version: String,
    name: String,
    curator_id: String,
    policy_version: String,
    sources: Vec<SourceBinding>,
}

pub fn upgrade_v2_project(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
) -> Result<ProjectStore> {
    let source = source.as_ref();
    let destination = destination.as_ref();
    validate_project_path(destination)?;
    if fs::symlink_metadata(destination).is_ok() {
        return Err(AppError::InvalidProject(format!(
            "upgrade destination already exists: {}",
            destination.display()
        )));
    }
    let manifest: LegacyV2ProjectManifest =
        serde_json::from_reader(File::open(source.join("manifest.json"))?)?;
    if manifest.format_family != "okc" || manifest.schema_version != 2 {
        return Err(AppError::InvalidProject(
            "project upgrade accepts only an OKC schema-2 project".into(),
        ));
    }
    let LegacyV2ProjectManifest {
        name,
        curator_id,
        policy_version,
        sources,
        ..
    } = manifest;
    let mut upgraded = ProjectStore::create(destination, name, curator_id, policy_version)?;
    for source in sources {
        upgraded.add_source(source)?;
    }
    Ok(upgraded)
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
            .add_source(SourceBinding {
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
        let remote = ProviderCapabilitiesV3 {
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
            data_boundary: DataBoundaryV3::Remote,
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
        validate_bcp47("ko-KR").expect("language");
        assert!(validate_bcp47("ko--KR").is_err());
    }

    #[test]
    fn v2_upgrade_creates_a_new_project_with_source_bindings_only() {
        let temporary = tempfile::tempdir().expect("temporary");
        let old = temporary.path().join("Old.okc-project");
        let new = temporary.path().join("New.okc-project");
        let source_path = temporary.path().join("alpha");
        fs::create_dir(&old).expect("old project");
        fs::create_dir(&source_path).expect("source directory");
        fs::write(source_path.join("note.md"), b"source").expect("source note");
        let source = SourceBinding {
            source_id: okc_core::SourceId::new("alpha").expect("source ID"),
            owner_display_name: Some("Alice".into()),
            path: fs::canonicalize(source_path).expect("canonical source"),
            snapshot_id: Some("snap_old".into()),
        };
        fs::write(
            old.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "format_family": "okc",
                "schema_version": 2,
                "product_version": "0.2.0",
                "name": "Old",
                "curator_id": "curator",
                "policy_version": "policy-v2",
                "sources": [source]
            }))
            .expect("manifest"),
        )
        .expect("write manifest");
        let upgraded = upgrade_v2_project(&old, &new).expect("upgrade");
        assert_eq!(upgraded.manifest().schema_version, 3);
        assert_eq!(upgraded.manifest().sources, vec![source]);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(old.join("manifest.json")).expect("old manifest")
            )
            .expect("old JSON")["schema_version"],
            2
        );
        assert!(upgraded.latest_run().expect("latest run").is_some());
    }
}
