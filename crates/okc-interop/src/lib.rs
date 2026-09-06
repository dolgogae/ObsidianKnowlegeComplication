//! Safe, runtime-neutral facade shared by the Python and Node.js bindings.
//!
//! The boundary contains versioned DTOs, structured errors, explicit absolute
//! paths, bounded jobs, cancellation, and per-project mutation exclusion. It
//! deliberately contains no CLI presentation, cwd discovery, prompt, signal,
//! updater, keyring, Python, or Node.js runtime type.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use okc_ai::{AI_SCHEMA_VERSION, ProviderConfig};
use okc_app::integration_service::{ClusterReviewDecision, PreflightSummary, RoleBoundary};
use okc_app::v3::{IntegrationStatus, TaxonomyTaskOutput};
use okc_app::{
    AppError, ArtifactFamily, ArtifactService, ArtifactVerification, IntegrationCheckpoint,
    IntegrationExecution, IntegrationService, OperationControl, OperationPhase,
    ProgressEvent as AppProgressEvent, ProgressObserver, ProjectStore, ProviderService,
    SourceBinding,
};
use okc_core::SourceId;
use okc_core::integration::{IntegrationCorpus, TaxonomyCluster, TaxonomyProposal, V3Manifest};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use okc_ai::AiRole;

pub const INTEROP_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MAX_CONCURRENT_JOBS: usize = 4;
pub const MAX_EVENT_QUEUE: usize = 64;
const MAX_CONCURRENT_JOBS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
    OpenAiCompatible,
    Command,
}

impl From<ProviderKind> for okc_ai::ProviderKind {
    fn from(value: ProviderKind) -> Self {
        match value {
            ProviderKind::OpenAi => Self::OpenAi,
            ProviderKind::Anthropic => Self::Anthropic,
            ProviderKind::Gemini => Self::Gemini,
            ProviderKind::Ollama => Self::Ollama,
            ProviderKind::OpenAiCompatible => Self::OpenAiCompatible,
            ProviderKind::Command => Self::Command,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProfile {
    pub name: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    pub model: String,
    pub api_key_env: Option<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: u64,
    #[serde(default = "default_max_input_bytes")]
    pub max_input_bytes: u64,
    #[serde(default = "default_max_batch_items")]
    pub max_batch_items: u32,
    #[serde(default)]
    pub options: BTreeMap<String, Value>,
}

impl Debug for ProviderProfile {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let option_keys = self.options.keys().collect::<Vec<_>>();
        formatter
            .debug_struct("ProviderProfile")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("api_key_env", &self.api_key_env)
            .field("timeout_ms", &self.timeout_ms)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("max_input_bytes", &self.max_input_bytes)
            .field("max_batch_items", &self.max_batch_items)
            .field("option_keys", &option_keys)
            .finish()
    }
}

const fn default_timeout_ms() -> u64 {
    okc_ai::DEFAULT_TIMEOUT_MS
}

const fn default_max_response_bytes() -> u64 {
    okc_ai::DEFAULT_MAX_RESPONSE_BYTES
}

const fn default_max_input_bytes() -> u64 {
    64 * 1024 * 1024
}

const fn default_max_batch_items() -> u32 {
    2_048
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Argument,
    Path,
    Project,
    Provider,
    Consent,
    Approval,
    Output,
    Verification,
    Cancellation,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidArgument,
    PathNotAbsolute,
    PathNotFound,
    PathUnsafe,
    PathUnsupported,
    ResourceLimit,
    ProjectInvalid,
    ProjectBusy,
    ProviderInvalid,
    ProviderAuthentication,
    ProviderAuthorization,
    ProviderEnvSecretMissing,
    ProviderRateLimited,
    ProviderTimeout,
    ProviderResponseInvalid,
    ProviderUnsupported,
    ProviderTransport,
    RemoteConsentRequired,
    SensitiveRemoteForbidden,
    ApprovalRequired,
    ApprovalStale,
    OutputExists,
    OutputOverlap,
    OutputDurabilityUncertain,
    VerificationFailed,
    Cancelled,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
#[serde(deny_unknown_fields)]
pub struct OkcError {
    pub code: ErrorCode,
    pub category: ErrorCategory,
    pub message: String,
    pub retryable: bool,
    pub details: BTreeMap<String, Value>,
}

impl OkcError {
    fn new(code: ErrorCode, category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            code,
            category,
            message: sanitize_message(&message.into()),
            retryable: false,
            details: BTreeMap::new(),
        }
    }

    fn detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("structured interop errors are serializable")
    }

    /// Construct a structured argument error at a language-runtime boundary.
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, ErrorCategory::Argument, message)
    }

    /// Construct a redacted internal adapter error without exposing its source.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, ErrorCategory::Internal, message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Cancelling,
    Publishing,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    const fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelOutcome {
    Requested,
    TooLate,
    AlreadyFinished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressEvent {
    pub interop_schema_version: u32,
    pub sequence: u64,
    pub operation: String,
    pub state: JobState,
    pub phase: String,
    pub completed: u64,
    pub total: Option<u64>,
    pub current_item: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiInfo {
    pub interop_schema_version: u32,
    pub api_version: String,
    pub product_version: String,
    pub max_concurrent_jobs: usize,
    pub provider_profiles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectDescriptor {
    interop_schema_version: u32,
    result_type: String,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    pub source_id: String,
    pub path: PathBuf,
    pub owner_display_name: Option<String>,
    pub snapshot_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedPayload {
    pub interop_schema_version: u32,
    pub payload: Value,
}

impl VersionedPayload {
    fn new<T: Serialize>(payload: &T) -> Result<Self, OkcError> {
        Ok(Self {
            interop_schema_version: INTEROP_SCHEMA_VERSION,
            payload: serde_json::to_value(payload).map_err(|error| serialization_error(&error))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStatus {
    pub interop_schema_version: u32,
    pub checkpoint: IntegrationCheckpoint,
    pub integration: IntegrationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightResult {
    pub interop_schema_version: u32,
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

impl From<PreflightSummary> for PreflightResult {
    fn from(value: PreflightSummary) -> Self {
        Self {
            interop_schema_version: INTEROP_SCHEMA_VERSION,
            run_id: value.run_id,
            documents: value.documents,
            blocks: value.blocks,
            input_bytes: value.input_bytes,
            estimated_tokens_min: value.estimated_tokens_min,
            estimated_tokens_max: value.estimated_tokens_max,
            estimated_requests_min: value.estimated_requests_min,
            estimated_requests_max: value.estimated_requests_max,
            sensitive_findings: value.sensitive_findings,
            routes: value.routes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationResult {
    pub interop_schema_version: u32,
    pub run_id: String,
    pub documents: usize,
    pub embedding_inputs: usize,
    pub input_bytes: u64,
    pub sensitive_findings: usize,
    pub semantic_candidates: usize,
    pub checkpoint: IntegrationCheckpoint,
    pub integration_plan_id: Option<String>,
}

impl From<IntegrationExecution> for IntegrationResult {
    fn from(value: IntegrationExecution) -> Self {
        Self {
            interop_schema_version: INTEROP_SCHEMA_VERSION,
            run_id: value.run_id,
            documents: value.documents,
            embedding_inputs: value.embedding_inputs,
            input_bytes: value.input_bytes,
            sensitive_findings: value.sensitive_findings,
            semantic_candidates: value.semantic_candidates,
            checkpoint: value.checkpoint,
            integration_plan_id: value.integration_plan_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyResult {
    pub interop_schema_version: u32,
    pub corpus: IntegrationCorpus,
    pub taxonomy: TaxonomyProposal,
}

impl From<TaxonomyTaskOutput> for TaxonomyResult {
    fn from(value: TaxonomyTaskOutput) -> Self {
        Self {
            interop_schema_version: INTEROP_SCHEMA_VERSION,
            corpus: value.corpus,
            taxonomy: value.taxonomy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileResult {
    pub interop_schema_version: u32,
    pub path: PathBuf,
    pub integration_plan_id: String,
    pub file_count: usize,
    pub manifest: V3Manifest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationResult {
    pub interop_schema_version: u32,
    pub family: ArtifactFamily,
    pub valid: bool,
    pub artifact_path: PathBuf,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplanationResult {
    pub interop_schema_version: u32,
    pub family: ArtifactFamily,
    pub artifact_path: PathBuf,
    pub payload: Value,
}

type RawResult = std::result::Result<Value, OkcError>;
type Decoder<T> = Arc<dyn Fn(&Value) -> std::result::Result<T, OkcError> + Send + Sync>;

struct JobRecord {
    state: JobState,
    publication_barrier: bool,
    events: VecDeque<ProgressEvent>,
    result: Option<RawResult>,
}

struct SharedJob {
    operation: String,
    record: Mutex<JobRecord>,
    ready: Condvar,
    cancellation: okc_core::CancellationToken,
    next_sequence: AtomicU64,
}

impl SharedJob {
    fn new(operation: String) -> Arc<Self> {
        let shared = Arc::new(Self {
            operation,
            record: Mutex::new(JobRecord {
                state: JobState::Queued,
                publication_barrier: false,
                events: VecDeque::with_capacity(MAX_EVENT_QUEUE),
                result: None,
            }),
            ready: Condvar::new(),
            cancellation: okc_core::CancellationToken::default(),
            next_sequence: AtomicU64::new(0),
        });
        shared.push_event(JobState::Queued, "queued", 0, None, None, false);
        shared
    }

    fn failed(operation: String, error: OkcError) -> Arc<Self> {
        let shared = Self::new(operation);
        shared.finish(Err(error));
        shared
    }

    fn state(&self) -> JobState {
        self.record.lock().expect("job record").state
    }

    fn begin(&self) -> bool {
        let mut record = self.record.lock().expect("job record");
        if self.cancellation.is_cancelled() {
            drop(record);
            self.finish(Err(cancelled_error()));
            return false;
        }
        record.state = JobState::Running;
        drop(record);
        self.push_event(JobState::Running, "running", 0, None, None, false);
        true
    }

    fn finish(&self, result: RawResult) {
        let mut record = self.record.lock().expect("job record");
        if record.result.is_some() {
            return;
        }
        let (state, phase) = match &result {
            Ok(_) => (JobState::Completed, "completed"),
            Err(error) if error.code == ErrorCode::Cancelled => (JobState::Cancelled, "cancelled"),
            Err(_) => (JobState::Failed, "failed"),
        };
        record.state = state;
        record.result = Some(result);
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        push_bounded(
            &mut record.events,
            ProgressEvent {
                interop_schema_version: INTEROP_SCHEMA_VERSION,
                sequence,
                operation: self.operation.clone(),
                state,
                phase: phase.into(),
                completed: u64::from(state == JobState::Completed),
                total: Some(1),
                current_item: None,
            },
            true,
        );
        self.ready.notify_all();
    }

    fn push_event(
        &self,
        state: JobState,
        phase: &str,
        completed: u64,
        total: Option<u64>,
        current_item: Option<String>,
        terminal: bool,
    ) {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let mut record = self.record.lock().expect("job record");
        push_bounded(
            &mut record.events,
            ProgressEvent {
                interop_schema_version: INTEROP_SCHEMA_VERSION,
                sequence,
                operation: self.operation.clone(),
                state,
                phase: phase.into(),
                completed,
                total,
                current_item,
            },
            terminal,
        );
    }

    fn observe(&self, event: &AppProgressEvent) {
        let phase = serde_json::to_value(event.phase)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "processing".into());
        let mut record = self.record.lock().expect("job record");
        let state = if event.phase == OperationPhase::Publishing {
            record.publication_barrier = true;
            record.state = JobState::Publishing;
            JobState::Publishing
        } else if record.publication_barrier {
            JobState::Publishing
        } else if record.state == JobState::Cancelling {
            JobState::Cancelling
        } else {
            record.state = JobState::Running;
            JobState::Running
        };
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        push_bounded(
            &mut record.events,
            ProgressEvent {
                interop_schema_version: INTEROP_SCHEMA_VERSION,
                sequence,
                operation: self.operation.clone(),
                state,
                phase,
                completed: event.completed,
                total: event.total,
                current_item: event.current_item.clone(),
            },
            false,
        );
    }

    fn cancel(&self) -> CancelOutcome {
        let mut record = self.record.lock().expect("job record");
        if record.state.terminal() {
            return CancelOutcome::AlreadyFinished;
        }
        if record.publication_barrier || record.state == JobState::Publishing {
            return CancelOutcome::TooLate;
        }
        self.cancellation.cancel();
        record.state = JobState::Cancelling;
        drop(record);
        self.push_event(JobState::Cancelling, "cancelling", 0, None, None, false);
        CancelOutcome::Requested
    }

    fn take_events(&self) -> Vec<ProgressEvent> {
        self.record
            .lock()
            .expect("job record")
            .events
            .drain(..)
            .collect()
    }

    fn wait_raw(&self) -> RawResult {
        let mut record = self.record.lock().expect("job record");
        while record.result.is_none() {
            record = self.ready.wait(record).expect("job result wait");
        }
        record.result.clone().expect("terminal result")
    }
}

fn push_bounded(queue: &mut VecDeque<ProgressEvent>, event: ProgressEvent, terminal: bool) {
    if queue.len() == MAX_EVENT_QUEUE {
        if terminal {
            queue.pop_front();
        } else if let Some(last) = queue.back_mut() {
            // Coalesce the newest intermediate update; terminal state/result
            // live separately and can never be lost.
            *last = event;
            return;
        }
    }
    queue.push_back(event);
}

#[derive(Clone)]
pub struct Job<T> {
    shared: Arc<SharedJob>,
    decoder: Decoder<T>,
    marker: PhantomData<fn() -> T>,
}

impl<T> Debug for Job<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Job")
            .field("operation", &self.shared.operation)
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl<T> Job<T> {
    pub fn state(&self) -> JobState {
        self.shared.state()
    }

    pub fn events(&self) -> Vec<ProgressEvent> {
        self.shared.take_events()
    }

    pub fn cancel(&self) -> CancelOutcome {
        self.shared.cancel()
    }

    pub fn result(&self) -> std::result::Result<T, OkcError> {
        let value = self.shared.wait_raw()?;
        (self.decoder)(&value)
    }

    /// Erase only the result type for a runtime adapter. Job state, event
    /// storage, cancellation, and terminal result remain the same allocation.
    pub fn erase(&self) -> Job<Value> {
        Job {
            shared: Arc::clone(&self.shared),
            decoder: Arc::new(|value| Ok(value.clone())),
            marker: PhantomData,
        }
    }
}

enum WorkCommand {
    Run(WorkItem),
    Shutdown,
}

struct WorkItem {
    shared: Arc<SharedJob>,
    project_reservation: Option<PathBuf>,
    work: Box<dyn FnOnce(OperationControl) -> RawResult + Send + 'static>,
}

struct Scheduler {
    sender: Sender<WorkCommand>,
    reservations: Arc<Mutex<BTreeSet<PathBuf>>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    worker_count: usize,
}

impl Scheduler {
    fn new(worker_count: usize) -> std::io::Result<Arc<Self>> {
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let reservations = Arc::new(Mutex::new(BTreeSet::new()));
        let scheduler = Arc::new(Self {
            sender,
            reservations: Arc::clone(&reservations),
            workers: Mutex::new(Vec::new()),
            worker_count,
        });
        for index in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            let reservations = Arc::clone(&reservations);
            let handle = thread::Builder::new()
                .name(format!("okc-interop-worker-{index}"))
                .spawn(move || worker_loop(&receiver, &reservations))?;
            scheduler
                .workers
                .lock()
                .expect("worker handles")
                .push(handle);
        }
        Ok(scheduler)
    }

    fn submit<T, R, F>(
        &self,
        operation: impl Into<String>,
        project: Option<PathBuf>,
        mutating: bool,
        work: F,
    ) -> Job<T>
    where
        T: DeserializeOwned + Send + 'static,
        R: Serialize + Send + 'static,
        F: FnOnce(OperationControl) -> std::result::Result<R, OkcError> + Send + 'static,
    {
        let operation = operation.into();
        let reservation = if mutating {
            if let Some(project) = project {
                let project = reservation_path(&project);
                let mut reservations = self.reservations.lock().expect("project reservations");
                if !reservations.insert(project.clone()) {
                    return typed_failed(
                        operation,
                        OkcError::new(
                            ErrorCode::ProjectBusy,
                            ErrorCategory::Project,
                            "another mutating job already owns this project",
                        )
                        .detail("project_path", project.to_string_lossy().to_string()),
                    );
                }
                Some(project)
            } else {
                None
            }
        } else {
            None
        };
        let shared = SharedJob::new(operation);
        let job = Job {
            shared: Arc::clone(&shared),
            decoder: Arc::new(|value| {
                serde_json::from_value(value.clone()).map_err(|error| serialization_error(&error))
            }),
            marker: PhantomData,
        };
        let item = WorkItem {
            shared,
            project_reservation: reservation.clone(),
            work: Box::new(move |control| {
                let output = work(control)?;
                serde_json::to_value(output).map_err(|error| serialization_error(&error))
            }),
        };
        if self.sender.send(WorkCommand::Run(item)).is_err() {
            if let Some(path) = reservation {
                self.reservations
                    .lock()
                    .expect("project reservations")
                    .remove(&path);
            }
            job.shared.finish(Err(OkcError::new(
                ErrorCode::Internal,
                ErrorCategory::Internal,
                "job scheduler is unavailable",
            )));
        }
        job
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        for _ in 0..self.worker_count {
            let _ = self.sender.send(WorkCommand::Shutdown);
        }
        // Dropping a language-runtime client can happen while Python holds the
        // GIL or Node.js is finalizing an object. Joining here could deadlock
        // if an in-flight provider talks to a host thread that needs that
        // runtime lock. Dropping JoinHandle detaches; queued work precedes the
        // shutdown markers and each bounded worker exits after it drains.
        self.workers.lock().expect("worker handles").clear();
    }
}

fn typed_failed<T: DeserializeOwned + Send + 'static>(
    operation: String,
    error: OkcError,
) -> Job<T> {
    Job {
        shared: SharedJob::failed(operation, error),
        decoder: Arc::new(|value| {
            serde_json::from_value(value.clone()).map_err(|error| serialization_error(&error))
        }),
        marker: PhantomData,
    }
}

fn worker_loop(
    receiver: &Arc<Mutex<Receiver<WorkCommand>>>,
    reservations: &Arc<Mutex<BTreeSet<PathBuf>>>,
) {
    loop {
        let command = receiver.lock().expect("job receiver").recv();
        let Ok(command) = command else {
            break;
        };
        let WorkCommand::Run(item) = command else {
            break;
        };
        if item.shared.begin() {
            let observer = Arc::new(JobProgressObserver {
                shared: Arc::clone(&item.shared),
            });
            let control = OperationControl {
                cancellation: item.shared.cancellation.clone(),
                observer,
            };
            let mut result = (item.work)(control);
            if item.shared.cancellation.is_cancelled()
                && !item
                    .shared
                    .record
                    .lock()
                    .expect("job record")
                    .publication_barrier
            {
                result = Err(cancelled_error());
            }
            item.shared.finish(result);
        }
        if let Some(project) = item.project_reservation {
            reservations
                .lock()
                .expect("project reservations")
                .remove(&project);
        }
    }
}

struct JobProgressObserver {
    shared: Arc<SharedJob>,
}

impl ProgressObserver for JobProgressObserver {
    fn observe(&self, event: &AppProgressEvent) {
        self.shared.observe(event);
    }
}

#[derive(Clone)]
pub struct OkcClient {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    profiles: BTreeMap<String, ProviderProfile>,
    providers: ProviderService,
    scheduler: Arc<Scheduler>,
    max_concurrent_jobs: usize,
}

impl Debug for OkcClient {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OkcClient")
            .field("profiles", &self.inner.profiles.keys().collect::<Vec<_>>())
            .field("max_concurrent_jobs", &self.inner.max_concurrent_jobs)
            .finish()
    }
}

impl OkcClient {
    pub fn new(
        profiles: impl IntoIterator<Item = ProviderProfile>,
        max_concurrent_jobs: Option<usize>,
    ) -> std::result::Result<Self, OkcError> {
        let max_concurrent_jobs = max_concurrent_jobs.unwrap_or(DEFAULT_MAX_CONCURRENT_JOBS);
        if !(1..=MAX_CONCURRENT_JOBS).contains(&max_concurrent_jobs) {
            return Err(OkcError::new(
                ErrorCode::InvalidArgument,
                ErrorCategory::Argument,
                format!("max_concurrent_jobs must be within 1..={MAX_CONCURRENT_JOBS}"),
            ));
        }
        let mut public_profiles = BTreeMap::new();
        let mut native_profiles = BTreeMap::new();
        for profile in profiles {
            if profile.name.is_empty()
                || profile.name.len() > 128
                || !profile
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(OkcError::new(
                    ErrorCode::ProviderInvalid,
                    ErrorCategory::Provider,
                    "provider profile name is invalid",
                ));
            }
            if profile.kind == ProviderKind::Command {
                return Err(OkcError::new(
                    ErrorCode::ProviderUnsupported,
                    ErrorCategory::Provider,
                    "schema-3 command provider profiles are not implemented",
                )
                .detail("profile_name", profile.name));
            }
            let name = profile.name.clone();
            if public_profiles
                .insert(name.clone(), profile.clone())
                .is_some()
            {
                return Err(OkcError::new(
                    ErrorCode::ProviderInvalid,
                    ErrorCategory::Provider,
                    "provider profile names must be unique",
                )
                .detail("profile_name", name));
            }
            let native = okc_ai::ProviderProfile {
                kind: profile.kind.into(),
                endpoint: profile.endpoint.clone(),
                model: profile.model.clone(),
                api_key_env: profile.api_key_env.clone(),
                os_keychain: None,
                timeout_ms: profile.timeout_ms,
                max_response_bytes: profile.max_response_bytes,
                max_input_bytes: profile.max_input_bytes,
                max_batch_items: profile.max_batch_items,
                options: profile.options.clone(),
            };
            native.validate().map_err(map_provider_error)?;
            native_profiles.insert(name, native);
        }
        let provider_config = ProviderConfig {
            schema_version: AI_SCHEMA_VERSION,
            profiles: native_profiles,
        };
        let providers =
            ProviderService::from_fixed_config(&provider_config).map_err(map_app_error)?;
        let scheduler = Scheduler::new(max_concurrent_jobs).map_err(|error| {
            OkcError::new(
                ErrorCode::Internal,
                ErrorCategory::Internal,
                "could not start the bounded job scheduler",
            )
            .detail("io_kind", format!("{:?}", error.kind()))
        })?;
        Ok(Self {
            inner: Arc::new(ClientInner {
                profiles: public_profiles,
                providers,
                scheduler,
                max_concurrent_jobs,
            }),
        })
    }

    pub fn api_info(&self) -> ApiInfo {
        ApiInfo {
            interop_schema_version: INTEROP_SCHEMA_VERSION,
            api_version: "1".into(),
            product_version: env!("CARGO_PKG_VERSION").into(),
            max_concurrent_jobs: self.inner.max_concurrent_jobs,
            provider_profiles: self.inner.profiles.keys().cloned().collect(),
        }
    }

    pub fn create_project(
        &self,
        path: impl Into<PathBuf>,
        name: String,
        curator_id: String,
        policy_version: String,
        language: Option<String>,
    ) -> Job<Project> {
        let path = match absolute_lexical(path.into(), "project path") {
            Ok(path) => path,
            Err(error) => {
                return project_job(
                    typed_failed::<ProjectDescriptor>("project.create".into(), error),
                    self.clone(),
                );
            }
        };
        let reservation = reservation_path(&path);
        if let Some(language) = language.as_deref()
            && let Err(error) = okc_app::v3::validate_bcp47(language)
        {
            return project_job(
                typed_failed::<ProjectDescriptor>("project.create".into(), map_app_error(error)),
                self.clone(),
            );
        }
        let client = self.clone();
        let raw: Job<ProjectDescriptor> =
            self.inner
                .scheduler
                .submit("project.create", Some(reservation), true, move |_| {
                    let mut project = ProjectStore::create(&path, name, curator_id, policy_version)
                        .map_err(map_app_error)?;
                    if language.is_some() {
                        project.set_language(language).map_err(map_app_error)?;
                    }
                    Ok(ProjectDescriptor {
                        interop_schema_version: INTEROP_SCHEMA_VERSION,
                        result_type: "project".into(),
                        path: project.root().to_path_buf(),
                    })
                });
        project_job(raw, client)
    }

    pub fn open_project(&self, path: impl Into<PathBuf>) -> Job<Project> {
        let path = match absolute_lexical(path.into(), "project path") {
            Ok(path) => path,
            Err(error) => {
                return project_job(
                    typed_failed::<ProjectDescriptor>("project.open".into(), error),
                    self.clone(),
                );
            }
        };
        let client = self.clone();
        let raw: Job<ProjectDescriptor> =
            self.inner
                .scheduler
                .submit("project.open", None, false, move |_| {
                    let project = ProjectStore::open(&path).map_err(map_app_error)?;
                    Ok(ProjectDescriptor {
                        interop_schema_version: INTEROP_SCHEMA_VERSION,
                        result_type: "project".into(),
                        path: project.root().to_path_buf(),
                    })
                });
        project_job(raw, client)
    }

    /// Build a runtime-neutral handle without reading the filesystem. Runtime
    /// bindings use this only after a successful create/open job result.
    pub fn project_handle(
        &self,
        path: impl Into<PathBuf>,
    ) -> std::result::Result<Project, OkcError> {
        Ok(Project {
            client: self.clone(),
            path: absolute_lexical(path.into(), "project path")?,
        })
    }

    pub fn test_provider(&self, name: String) -> Job<VersionedPayload> {
        let providers = self.inner.providers.clone();
        self.inner
            .scheduler
            .submit("provider.test", None, false, move |control| {
                let result = providers
                    .test_profile(&name, &control.cancellation)
                    .map_err(map_app_error)?;
                VersionedPayload::new(&json!({
                    "profile_name": result.profile_name,
                    "generation": result.generation,
                    "embeddings": result.embeddings
                }))
            })
    }

    pub fn verify_artifact(&self, path: impl Into<PathBuf>) -> Job<VerificationResult> {
        let path = match absolute_lexical(path.into(), "artifact path") {
            Ok(path) => path,
            Err(error) => return typed_failed("artifact.verify".into(), error),
        };
        self.inner
            .scheduler
            .submit("artifact.verify", None, false, move |_| {
                normalize_verification(
                    &path,
                    &ArtifactService.verify(&path).map_err(map_app_error)?,
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn explain_artifact(
        &self,
        path: impl Into<PathBuf>,
        output_path: Option<String>,
        package: bool,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> Job<ExplanationResult> {
        let path = match absolute_lexical(path.into(), "artifact path") {
            Ok(path) => path,
            Err(error) => return typed_failed("artifact.explain".into(), error),
        };
        self.inner
            .scheduler
            .submit("artifact.explain", None, false, move |_| {
                let explanation = ArtifactService
                    .explain(
                        &path,
                        output_path.as_deref(),
                        package,
                        limit.unwrap_or(256),
                        cursor.as_deref(),
                    )
                    .map_err(map_app_error)?;
                Ok(ExplanationResult {
                    interop_schema_version: INTEROP_SCHEMA_VERSION,
                    family: explanation.family(),
                    artifact_path: path,
                    payload: explanation.payload_json().map_err(map_app_error)?,
                })
            })
    }
}

fn project_job(raw: Job<ProjectDescriptor>, client: OkcClient) -> Job<Project> {
    Job {
        shared: raw.shared,
        decoder: Arc::new(move |value| {
            let descriptor: ProjectDescriptor = serde_json::from_value(value.clone())
                .map_err(|error| serialization_error(&error))?;
            if descriptor.interop_schema_version != INTEROP_SCHEMA_VERSION
                || descriptor.result_type != "project"
            {
                return Err(OkcError::new(
                    ErrorCode::Internal,
                    ErrorCategory::Internal,
                    "job returned an invalid project descriptor",
                ));
            }
            client.project_handle(descriptor.path)
        }),
        marker: PhantomData,
    }
}

#[derive(Clone)]
pub struct Project {
    client: OkcClient,
    path: PathBuf,
}

impl Debug for Project {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Project")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Project {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn manifest(&self) -> Job<VersionedPayload> {
        let path = self.path.clone();
        self.client
            .inner
            .scheduler
            .submit("project.manifest", None, false, move |_| {
                let project = ProjectStore::open(path).map_err(map_app_error)?;
                VersionedPayload::new(project.manifest())
            })
    }

    pub fn status(&self) -> Job<ProjectStatus> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client.inner.scheduler.submit(
            "project.status",
            Some(self.path.clone()),
            true,
            move |_| {
                let integration = ProjectStore::open(&path)
                    .and_then(|project| project.integration_status())
                    .map_err(map_app_error)?;
                let checkpoint = IntegrationService::new(path, providers)
                    .checkpoint()
                    .map_err(map_app_error)?;
                Ok(ProjectStatus {
                    interop_schema_version: INTEROP_SCHEMA_VERSION,
                    checkpoint,
                    integration,
                })
            },
        )
    }

    pub fn add_source(&self, source: SourceInput) -> Job<VersionedPayload> {
        let binding = match source_binding(source) {
            Ok(binding) => binding,
            Err(error) => {
                return typed_failed("project.source.add".into(), error);
            }
        };
        self.mutate_sources("project.source.add", move |project| {
            project.add_source_explicit(binding)?;
            Ok(json!({"changed": true}))
        })
    }

    pub fn rebind_source(
        &self,
        source_id: String,
        path: PathBuf,
        snapshot_id: Option<String>,
    ) -> Job<VersionedPayload> {
        let path = match absolute_lexical(path, "source path") {
            Ok(path) => path,
            Err(error) => {
                return typed_failed("project.source.rebind".into(), error);
            }
        };
        let source_id = match SourceId::new(source_id).map_err(map_core_error) {
            Ok(source_id) => source_id,
            Err(error) => {
                return typed_failed("project.source.rebind".into(), error);
            }
        };
        self.mutate_sources("project.source.rebind", move |project| {
            project.rebind_source_explicit(&source_id, path, snapshot_id)?;
            Ok(json!({"changed": true}))
        })
    }

    pub fn replace_sources(&self, sources: Vec<SourceInput>) -> Job<VersionedPayload> {
        let bindings = sources
            .into_iter()
            .map(source_binding)
            .collect::<std::result::Result<Vec<_>, _>>();
        let bindings = match bindings {
            Ok(bindings) => bindings,
            Err(error) => {
                return typed_failed("project.source.replace".into(), error);
            }
        };
        self.mutate_sources("project.source.replace", move |project| {
            project.replace_sources_explicit(bindings)?;
            Ok(json!({"changed": true}))
        })
    }

    pub fn set_language(&self, language: Option<String>) -> Job<VersionedPayload> {
        self.mutate_sources("project.language.set", move |project| {
            project.set_language(language)?;
            Ok(json!({"changed": true}))
        })
    }

    pub fn set_ai_route(
        &self,
        role: Option<AiRole>,
        profile_name: String,
    ) -> Job<VersionedPayload> {
        let known = self.client.inner.profiles.contains_key(&profile_name);
        if !known {
            return typed_failed(
                "project.ai_route.set".into(),
                OkcError::new(
                    ErrorCode::ProviderInvalid,
                    ErrorCategory::Provider,
                    "AI route references an unknown immutable provider profile",
                )
                .detail("profile_name", profile_name),
            );
        }
        self.mutate_sources("project.ai_route.set", move |project| {
            project.set_ai_route(role, profile_name)?;
            Ok(json!({"changed": true}))
        })
    }

    pub fn preflight(&self) -> Job<PreflightResult> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client.inner.scheduler.submit(
            "integration.preflight",
            Some(self.path.clone()),
            true,
            move |control| {
                let result = IntegrationService::new(path, providers)
                    .preflight(&control)
                    .map_err(map_app_error)?;
                Ok(PreflightResult::from(result))
            },
        )
    }

    pub fn integrate(
        &self,
        allow_remote_provider: bool,
        remote_disclosure_confirmed: bool,
    ) -> Job<IntegrationResult> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client.inner.scheduler.submit(
            "integration.execute",
            Some(self.path.clone()),
            true,
            move |control| {
                let result = IntegrationService::new(path, providers)
                    .execute(allow_remote_provider, remote_disclosure_confirmed, &control)
                    .map_err(map_app_error)?;
                Ok(IntegrationResult::from(result))
            },
        )
    }

    pub fn taxonomy(&self) -> Job<TaxonomyResult> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client
            .inner
            .scheduler
            .submit("integration.taxonomy.get", None, false, move |_| {
                let result = IntegrationService::new(path, providers)
                    .latest_taxonomy()
                    .map_err(map_app_error)?;
                Ok(TaxonomyResult::from(result))
            })
    }

    pub fn approve_taxonomy(
        &self,
        edited_clusters: Option<Vec<TaxonomyCluster>>,
        rationale: Option<String>,
    ) -> Job<VersionedPayload> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client.inner.scheduler.submit(
            "integration.taxonomy.approve",
            Some(self.path.clone()),
            true,
            move |_| {
                let value = IntegrationService::new(path, providers)
                    .approve_taxonomy(edited_clusters, rationale)
                    .map_err(map_app_error)?;
                VersionedPayload::new(&value)
            },
        )
    }

    pub fn clusters(&self) -> Job<VersionedPayload> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client
            .inner
            .scheduler
            .submit("integration.clusters.get", None, false, move |_| {
                let value = IntegrationService::new(path, providers)
                    .completed_clusters()
                    .map_err(map_app_error)?;
                VersionedPayload::new(&value)
            })
    }

    pub fn approve_cluster(
        &self,
        cluster_id: String,
        omission_rationales: BTreeMap<String, String>,
        minor_waivers: BTreeMap<String, String>,
    ) -> Job<VersionedPayload> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client.inner.scheduler.submit(
            "integration.cluster.approve",
            Some(self.path.clone()),
            true,
            move |_| {
                let decision = ClusterReviewDecision {
                    omission_rationales,
                    minor_waivers,
                };
                let value = IntegrationService::new(path, providers)
                    .approve_cluster(&cluster_id, &decision)
                    .map_err(map_app_error)?;
                VersionedPayload::new(&value)
            },
        )
    }

    pub fn regenerate_cluster(
        &self,
        cluster_id: String,
        feedback: String,
        allow_remote_provider: bool,
        remote_disclosure_confirmed: bool,
    ) -> Job<VersionedPayload> {
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client.inner.scheduler.submit(
            "integration.cluster.regenerate",
            Some(self.path.clone()),
            true,
            move |control| {
                let service = IntegrationService::new(path, providers);
                let request = service
                    .request_cluster_regeneration(&cluster_id, feedback)
                    .map_err(map_app_error)?;
                let execution = service
                    .execute(allow_remote_provider, remote_disclosure_confirmed, &control)
                    .map_err(map_app_error)?;
                VersionedPayload::new(&json!({
                    "regeneration": request,
                    "execution": execution
                }))
            },
        )
    }

    pub fn compile(&self, output: PathBuf) -> Job<CompileResult> {
        let output = match absolute_lexical(output, "output path") {
            Ok(path) => path,
            Err(error) => {
                return typed_failed("integration.compile".into(), error);
            }
        };
        let path = self.path.clone();
        let providers = self.client.inner.providers.clone();
        self.client.inner.scheduler.submit(
            "integration.compile",
            Some(self.path.clone()),
            true,
            move |control| {
                let (artifact, manifest) = IntegrationService::new(path, providers)
                    .compile_latest(output, &control)
                    .map_err(map_app_error)?;
                Ok(CompileResult {
                    interop_schema_version: INTEROP_SCHEMA_VERSION,
                    path: artifact.path,
                    integration_plan_id: artifact.integration_plan_id,
                    file_count: artifact.files,
                    manifest,
                })
            },
        )
    }

    fn mutate_sources<F>(&self, operation: &'static str, mutation: F) -> Job<VersionedPayload>
    where
        F: FnOnce(&mut ProjectStore) -> okc_app::Result<Value> + Send + 'static,
    {
        let path = self.path.clone();
        self.client
            .inner
            .scheduler
            .submit(operation, Some(self.path.clone()), true, move |_| {
                let mut project = ProjectStore::open(path).map_err(map_app_error)?;
                let value = mutation(&mut project).map_err(map_app_error)?;
                VersionedPayload::new(&value)
            })
    }
}

fn source_binding(source: SourceInput) -> std::result::Result<SourceBinding, OkcError> {
    Ok(SourceBinding {
        source_id: SourceId::new(source.source_id).map_err(map_core_error)?,
        owner_display_name: source.owner_display_name,
        path: absolute_lexical(source.path, "source path")?,
        snapshot_id: source.snapshot_id,
    })
}

fn normalize_verification(
    path: &Path,
    verification: &ArtifactVerification,
) -> std::result::Result<VerificationResult, OkcError> {
    let family = verification.family();
    let payload = verification.payload_json().map_err(map_app_error)?;
    Ok(VerificationResult {
        interop_schema_version: INTEROP_SCHEMA_VERSION,
        family,
        valid: true,
        artifact_path: path.to_path_buf(),
        payload,
    })
}

fn absolute_lexical(path: PathBuf, label: &str) -> std::result::Result<PathBuf, OkcError> {
    if !path.is_absolute() {
        return Err(OkcError::new(
            ErrorCode::PathNotAbsolute,
            ErrorCategory::Path,
            format!("{label} must be an explicit absolute path"),
        )
        .detail("path", path.to_string_lossy().to_string()));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(OkcError::new(
            ErrorCode::PathUnsafe,
            ErrorCategory::Path,
            format!("{label} must not contain . or .. components"),
        )
        .detail("path", path.to_string_lossy().to_string()));
    }
    Ok(path)
}

fn reservation_path(path: &Path) -> PathBuf {
    let mut ancestor = path.to_path_buf();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        if let Some(name) = ancestor.file_name() {
            suffix.push(name.to_os_string());
        }
        if !ancestor.pop() {
            return path.to_path_buf();
        }
    }
    let Ok(mut normalized) = std::fs::canonicalize(&ancestor) else {
        return path.to_path_buf();
    };
    for component in suffix.into_iter().rev() {
        normalized.push(component);
    }
    normalized
}

fn serialization_error(error: &serde_json::Error) -> OkcError {
    OkcError::new(
        ErrorCode::Internal,
        ErrorCategory::Internal,
        "could not encode or decode an interop result",
    )
    .detail("json_error_class", format!("{:?}", error.classify()))
}

fn cancelled_error() -> OkcError {
    OkcError::new(
        ErrorCode::Cancelled,
        ErrorCategory::Cancellation,
        "operation was cancelled before publication",
    )
}

fn map_app_error(error: AppError) -> OkcError {
    match error {
        AppError::Io(error) => map_io_error(&error),
        AppError::Json(error) => serialization_error(&error),
        AppError::Sqlite(error) => OkcError::new(
            ErrorCode::ProjectInvalid,
            ErrorCategory::Project,
            "project state database operation failed",
        )
        .detail("sqlite_code", format!("{:?}", error.sqlite_error_code())),
        AppError::Core(error) => map_core_error(error),
        AppError::Provider(error) => map_provider_error(error),
        AppError::Credential(error) => OkcError::new(
            ErrorCode::ProviderAuthentication,
            ErrorCategory::Provider,
            error.to_string(),
        ),
        AppError::InvalidProject(message) => map_project_message(message),
        AppError::Locked(path) => {
            let mut error = OkcError::new(
                ErrorCode::ProjectBusy,
                ErrorCategory::Project,
                "project is already locked by another writer",
            )
            .detail("lock_path", path.to_string_lossy().to_string());
            error.retryable = true;
            error
        }
        AppError::Update(_) => OkcError::new(
            ErrorCode::Internal,
            ErrorCategory::Internal,
            "updater functionality is not available through language bindings",
        ),
        AppError::Artifact(message) => OkcError::new(
            ErrorCode::VerificationFailed,
            ErrorCategory::Verification,
            message,
        ),
    }
}

fn map_project_message(message: String) -> OkcError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("remote disclosure requires") {
        return OkcError::new(
            ErrorCode::RemoteConsentRequired,
            ErrorCategory::Consent,
            message,
        );
    }
    if lower.contains("sensitive content requires a local provider") {
        return OkcError::new(
            ErrorCode::SensitiveRemoteForbidden,
            ErrorCategory::Consent,
            message,
        );
    }
    if lower.contains("cancel") {
        return cancelled_error();
    }
    if lower.contains("provider profile") {
        return OkcError::new(ErrorCode::ProviderInvalid, ErrorCategory::Provider, message);
    }
    if lower.contains("stale") {
        return OkcError::new(ErrorCode::ApprovalStale, ErrorCategory::Approval, message);
    }
    if lower.contains("outside every source") || lower.contains("overlap") {
        return OkcError::new(ErrorCode::OutputOverlap, ErrorCategory::Output, message);
    }
    if lower.contains("approval") || lower.contains("approved") {
        return OkcError::new(
            ErrorCode::ApprovalRequired,
            ErrorCategory::Approval,
            message,
        );
    }
    OkcError::new(ErrorCode::ProjectInvalid, ErrorCategory::Project, message)
}

fn map_provider_error(error: okc_ai::ProviderError) -> OkcError {
    use okc_ai::ProviderErrorKind as Kind;
    let code = match error.kind {
        Kind::Authentication
            if error
                .message
                .contains("configured API-key environment variable") =>
        {
            ErrorCode::ProviderEnvSecretMissing
        }
        Kind::Authentication => ErrorCode::ProviderAuthentication,
        Kind::Authorization => ErrorCode::ProviderAuthorization,
        Kind::RateLimited => ErrorCode::ProviderRateLimited,
        Kind::Timeout => ErrorCode::ProviderTimeout,
        Kind::InvalidResponse | Kind::ResponseTooLarge | Kind::Refusal | Kind::ContextLimit => {
            ErrorCode::ProviderResponseInvalid
        }
        Kind::UnsupportedCapability => ErrorCode::ProviderUnsupported,
        Kind::RemotePolicy => ErrorCode::RemoteConsentRequired,
        Kind::Cancelled => ErrorCode::Cancelled,
        Kind::Transport => ErrorCode::ProviderTransport,
        Kind::InvalidRequest => ErrorCode::ProviderInvalid,
    };
    let category = match code {
        ErrorCode::Cancelled => ErrorCategory::Cancellation,
        ErrorCode::RemoteConsentRequired => ErrorCategory::Consent,
        _ => ErrorCategory::Provider,
    };
    let mut mapped = OkcError::new(code, category, error.message);
    mapped.retryable = error.retryable;
    if let Some(value) = error.retry_after_ms {
        mapped.details.insert("retry_after_ms".into(), value.into());
    }
    mapped
}

#[allow(clippy::too_many_lines)]
fn map_core_error(error: okc_core::OkcError) -> OkcError {
    use okc_core::OkcError as Core;
    match error {
        Core::InvalidConfig(message) => {
            OkcError::new(ErrorCode::InvalidArgument, ErrorCategory::Argument, message)
        }
        Core::UnsafePath { path, reason } => OkcError::new(
            ErrorCode::PathUnsafe,
            ErrorCategory::Path,
            "path failed the portable safety policy",
        )
        .detail("path", path)
        .detail("reason", reason),
        Core::UnsupportedSource(path) => OkcError::new(
            ErrorCode::PathUnsupported,
            ErrorCategory::Path,
            "source or artifact type is unsupported",
        )
        .detail("path", path.to_string_lossy().to_string()),
        Core::ResourceLimit(message) => {
            OkcError::new(ErrorCode::ResourceLimit, ErrorCategory::Path, message)
        }
        Core::MalformedInput { path, reason } => OkcError::new(
            ErrorCode::PathUnsafe,
            ErrorCategory::Path,
            "input is malformed",
        )
        .detail("path", path)
        .detail("reason", reason),
        Core::IdentityMismatch(message)
        | Core::PlanStale(message)
        | Core::ApprovalStale(message) => {
            OkcError::new(ErrorCode::ApprovalStale, ErrorCategory::Approval, message)
        }
        Core::ProposalInvalid(message) => OkcError::new(
            ErrorCode::ProviderResponseInvalid,
            ErrorCategory::Provider,
            message,
        ),
        Core::OutputExists(path) => OkcError::new(
            ErrorCode::OutputExists,
            ErrorCategory::Output,
            "output destination already exists",
        )
        .detail("path", path.to_string_lossy().to_string()),
        Core::PublishedButDurabilityUncertain { path, source } => OkcError::new(
            ErrorCode::OutputDurabilityUncertain,
            ErrorCategory::Output,
            "output was published but parent-directory durability is uncertain",
        )
        .detail("path", path.to_string_lossy().to_string())
        .detail("io_kind", format!("{:?}", source.kind())),
        Core::PackPublicationAfterCompile {
            compiled_vault,
            pack,
            source,
        } => OkcError::new(
            ErrorCode::OutputDurabilityUncertain,
            ErrorCategory::Output,
            "Compiled Vault remains published after Pack publication failed",
        )
        .detail(
            "compiled_vault",
            compiled_vault.to_string_lossy().to_string(),
        )
        .detail("pack", pack.to_string_lossy().to_string())
        .detail("cause", map_core_error(*source).to_json()),
        Core::StagingDispositionFailed {
            staging, original, ..
        } => OkcError::new(
            ErrorCode::Internal,
            ErrorCategory::Output,
            "staging disposition failed after an output error",
        )
        .detail("staging", staging.to_string_lossy().to_string())
        .detail("cause", map_core_error(*original).to_json()),
        Core::VerificationFailed(message) => OkcError::new(
            ErrorCode::VerificationFailed,
            ErrorCategory::Verification,
            message,
        ),
        Core::Provider(message) => {
            OkcError::new(ErrorCode::ProviderInvalid, ErrorCategory::Provider, message)
        }
        Core::Io { path, source } => {
            map_io_error(&source).detail("path", path.to_string_lossy().to_string())
        }
        Core::Json(error) => serialization_error(&error),
        Core::TomlDecode(_) | Core::TomlEncode(_) => OkcError::new(
            ErrorCode::InvalidArgument,
            ErrorCategory::Argument,
            "configuration serialization failed",
        ),
        Core::Sqlite(_) => OkcError::new(
            ErrorCode::ProjectInvalid,
            ErrorCategory::Project,
            "compiler workspace database operation failed",
        ),
        Core::Internal(message) => {
            OkcError::new(ErrorCode::Internal, ErrorCategory::Internal, message)
        }
        _ => OkcError::new(
            ErrorCode::Internal,
            ErrorCategory::Internal,
            "unrecognized compiler failure",
        ),
    }
}

fn map_io_error(error: &std::io::Error) -> OkcError {
    if error.kind() == std::io::ErrorKind::NotFound {
        OkcError::new(
            ErrorCode::PathNotFound,
            ErrorCategory::Path,
            "required path was not found",
        )
    } else {
        OkcError::new(
            ErrorCode::PathUnsafe,
            ErrorCategory::Path,
            "filesystem operation failed",
        )
        .detail("io_kind", format!("{:?}", error.kind()))
    }
}

fn sanitize_message(message: &str) -> String {
    message
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '\u{202a}'..='\u{202e}' | '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'
                )
            {
                ' '
            } else {
                character
            }
        })
        .take(4_096)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn client(max: usize) -> OkcClient {
        OkcClient::new(Vec::<ProviderProfile>::new(), Some(max)).expect("client")
    }

    #[test]
    fn relative_paths_fail_as_structured_job_errors() {
        let error = client(1)
            .open_project("relative.okc-project")
            .result()
            .expect_err("relative path");
        assert_eq!(error.code, ErrorCode::PathNotAbsolute);
        assert_eq!(error.category, ErrorCategory::Path);
    }

    #[test]
    fn job_events_are_bounded_and_terminal_result_is_retained() {
        let scheduler = Scheduler::new(1).expect("scheduler");
        let job: Job<u64> = scheduler.submit("events", None, false, |control| {
            for index in 0..200 {
                control.observer.observe(&AppProgressEvent {
                    operation: okc_app::OperationKind::Integrate,
                    phase: OperationPhase::Processing,
                    completed: index,
                    total: Some(200),
                    current_item: None,
                });
            }
            Ok(42)
        });
        assert_eq!(job.result().expect("result"), 42);
        let events = job.events();
        assert!(events.len() <= MAX_EVENT_QUEUE);
        assert_eq!(
            events.last().expect("terminal event").state,
            JobState::Completed
        );
        assert_eq!(job.result().expect("retained result"), 42);
    }

    #[test]
    fn scheduler_drop_never_waits_for_an_in_flight_runtime_job() {
        let scheduler = Scheduler::new(1).expect("scheduler");
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let job: Job<u8> = scheduler.submit("lifetime", None, false, move |_| {
            started_tx.send(()).expect("started");
            release_rx.recv().expect("release");
            Ok(7)
        });
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker started");

        let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
        let drop_thread = thread::spawn(move || {
            drop(scheduler);
            dropped_tx.send(()).expect("drop notification");
        });
        let drop_was_non_blocking = dropped_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        release_tx.send(()).expect("release worker");

        assert_eq!(job.result().expect("job survived scheduler drop"), 7);
        drop_thread.join().expect("scheduler drop thread");
        assert!(drop_was_non_blocking);
    }

    #[test]
    fn same_project_mutations_fail_busy_while_distinct_projects_run_in_parallel() {
        let scheduler = Scheduler::new(2).expect("scheduler");
        let first_path = PathBuf::from("/tmp/okc-interop-project-a");
        let second_path = PathBuf::from("/tmp/okc-interop-project-b");
        let start = Instant::now();
        let first: Job<u8> = scheduler.submit("first", Some(first_path.clone()), true, |_| {
            thread::sleep(Duration::from_millis(100));
            Ok(1)
        });
        let busy: Job<u8> = scheduler.submit("busy", Some(first_path), true, |_| Ok(2));
        let second: Job<u8> = scheduler.submit("second", Some(second_path), true, |_| {
            thread::sleep(Duration::from_millis(100));
            Ok(3)
        });
        assert_eq!(
            busy.result().expect_err("busy").code,
            ErrorCode::ProjectBusy
        );
        assert_eq!(first.result().expect("first"), 1);
        assert_eq!(second.result().expect("second"), 3);
        assert!(start.elapsed() < Duration::from_millis(190));
    }

    #[cfg(unix)]
    #[test]
    fn project_reservations_collapse_filesystem_aliases() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary");
        let project = temporary.path().join("project.okc-project");
        let alias = temporary.path().join("alias.okc-project");
        std::fs::create_dir(&project).expect("project directory");
        symlink(&project, &alias).expect("project alias");
        let scheduler = Scheduler::new(1).expect("scheduler");
        let first: Job<u8> = scheduler.submit("first", Some(project), true, |_| {
            thread::sleep(Duration::from_millis(50));
            Ok(1)
        });
        let busy: Job<u8> = scheduler.submit("busy", Some(alias), true, |_| Ok(2));
        assert_eq!(
            busy.result().expect_err("alias is busy").code,
            ErrorCode::ProjectBusy
        );
        assert_eq!(first.result().expect("first"), 1);
    }

    #[test]
    fn cancellation_changes_state_and_publication_barrier_is_too_late() {
        let scheduler = Scheduler::new(1).expect("scheduler");
        let cancelled: Job<u8> = scheduler.submit("cancel", None, false, |control| {
            while !control.cancellation.is_cancelled() {
                thread::yield_now();
            }
            Err::<u8, _>(cancelled_error())
        });
        while cancelled.state() == JobState::Queued {
            thread::yield_now();
        }
        assert_eq!(cancelled.cancel(), CancelOutcome::Requested);
        assert_eq!(
            cancelled.result().expect_err("cancelled").code,
            ErrorCode::Cancelled
        );
        assert_eq!(cancelled.cancel(), CancelOutcome::AlreadyFinished);

        let publishing: Job<u8> = scheduler.submit("publish", None, false, |control| {
            control.observer.observe(&AppProgressEvent {
                operation: okc_app::OperationKind::Compile,
                phase: OperationPhase::Publishing,
                completed: 0,
                total: Some(1),
                current_item: None,
            });
            thread::sleep(Duration::from_millis(50));
            Ok(1)
        });
        while publishing.state() != JobState::Publishing {
            thread::yield_now();
        }
        assert_eq!(publishing.cancel(), CancelOutcome::TooLate);
        assert_eq!(publishing.result().expect("published"), 1);
    }

    #[test]
    fn provider_profiles_reject_command_and_never_expose_raw_keys() {
        let profile = ProviderProfile {
            name: "command".into(),
            kind: ProviderKind::Command,
            endpoint: "command://local".into(),
            model: "model".into(),
            api_key_env: None,
            timeout_ms: default_timeout_ms(),
            max_response_bytes: default_max_response_bytes(),
            max_input_bytes: default_max_input_bytes(),
            max_batch_items: default_max_batch_items(),
            options: BTreeMap::new(),
        };
        let error = OkcClient::new([profile], None).expect_err("command unsupported");
        assert_eq!(error.code, ErrorCode::ProviderUnsupported);
        assert!(
            !serde_json::to_string(&error)
                .expect("error JSON")
                .contains("api_key")
        );
    }

    #[test]
    fn missing_environment_secret_is_typed_and_secret_values_are_never_reported() {
        let profile = ProviderProfile {
            name: "missing-secret".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:9".into(),
            model: "model".into(),
            api_key_env: Some("OKC_INTEROP_TEST_SECRET_8FC4EF7A_DO_NOT_SET".into()),
            timeout_ms: 1,
            max_response_bytes: 1024,
            max_input_bytes: 1024,
            max_batch_items: 1,
            options: BTreeMap::new(),
        };
        let client = OkcClient::new([profile], Some(1)).expect("client");
        let error = client
            .test_provider("missing-secret".into())
            .result()
            .expect_err("missing environment secret");
        assert_eq!(error.code, ErrorCode::ProviderEnvSecretMissing);
        assert_eq!(error.category, ErrorCategory::Provider);
        assert!(
            !serde_json::to_string(&error)
                .expect("structured error")
                .contains("raw-secret-value")
        );
    }

    #[test]
    fn invalid_project_input_is_rejected_before_filesystem_creation() {
        let temporary = tempfile::tempdir().expect("temporary");
        let path = temporary.path().join("invalid.okc-project");
        let error = client(1)
            .create_project(
                &path,
                "valid".into(),
                "curator".into(),
                "policy-v3".into(),
                Some("not a language".into()),
            )
            .result()
            .expect_err("invalid language");
        assert_eq!(error.code, ErrorCode::ProjectInvalid);
        assert!(!path.exists());
    }

    #[test]
    fn stale_project_failures_have_a_stable_approval_code() {
        let error = map_project_message("approved taxonomy is stale for the corpus".into());
        assert_eq!(error.code, ErrorCode::ApprovalStale);
        assert_eq!(error.category, ErrorCategory::Approval);
    }

    #[test]
    fn unknown_provider_failures_have_a_stable_provider_code() {
        let error = map_project_message("unknown provider profile `missing`".into());
        assert_eq!(error.code, ErrorCode::ProviderInvalid);
        assert_eq!(error.category, ErrorCategory::Provider);
    }

    #[test]
    fn application_results_carry_the_interop_schema_version() {
        let result = PreflightResult::from(PreflightSummary {
            run_id: "run_fixture".into(),
            documents: 1,
            blocks: 2,
            input_bytes: 3,
            estimated_tokens_min: 1,
            estimated_tokens_max: 2,
            estimated_requests_min: 3,
            estimated_requests_max: 4,
            sensitive_findings: 0,
            routes: Vec::new(),
        });
        let encoded = serde_json::to_value(result).expect("result JSON");
        assert_eq!(encoded["interop_schema_version"], INTEROP_SCHEMA_VERSION);
        assert_eq!(encoded["run_id"], "run_fixture");
    }
}
