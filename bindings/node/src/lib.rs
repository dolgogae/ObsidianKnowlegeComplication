use std::collections::BTreeMap;
use std::path::PathBuf;

use napi::bindgen_prelude::AsyncTask;
use napi::{Env, Error, Result, Status, Task};
use napi_derive::napi;
use okc_interop::{Job, OkcClient, OkcError, Project, ProviderProfile, SourceInput};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn encode<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| {
        napi_error(&OkcError::internal(
            "could not serialize a Node.js binding result",
        ))
    })
}

fn decode<T: DeserializeOwned>(value: &str, label: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|error| {
        napi_error(&OkcError::invalid_argument(format!(
            "{label} must be valid schema-version-2 JSON ({:?})",
            error.classify()
        )))
    })
}

fn napi_error(error: &OkcError) -> Error {
    let message = serde_json::to_string(&error).unwrap_or_else(|_| {
        "{\"code\":\"INTERNAL\",\"category\":\"internal\",\"message\":\"could not serialize a structured error\",\"retryable\":false,\"details\":{}}".into()
    });
    Error::new(Status::GenericFailure, message)
}

fn role_from_name(value: Option<String>) -> Result<Option<okc_interop::AiRole>> {
    value
        .map(|value| match value.as_str() {
            "embedding" => Ok(okc_interop::AiRole::Embedding),
            "organizer" => Ok(okc_interop::AiRole::Organizer),
            "synthesis" => Ok(okc_interop::AiRole::Synthesis),
            "critic" => Ok(okc_interop::AiRole::Critic),
            _ => Err(napi_error(&OkcError::invalid_argument(
                "role must be embedding, organizer, synthesis, critic, or null",
            ))),
        })
        .transpose()
}

pub struct WaitJob {
    job: Job<Value>,
}

impl Task for WaitJob {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> Result<Self::Output> {
        encode(&self.job.result().map_err(|error| napi_error(&error))?)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi]
pub struct NativeJob {
    inner: Job<Value>,
}

impl NativeJob {
    fn new(inner: Job<Value>) -> Self {
        Self { inner }
    }
}

#[napi]
impl NativeJob {
    #[napi]
    pub fn state(&self) -> Result<String> {
        encode(&self.inner.state())
    }

    #[napi]
    pub fn events_json(&self) -> Result<String> {
        encode(&self.inner.events())
    }

    #[napi]
    pub fn result_json(&self) -> AsyncTask<WaitJob> {
        AsyncTask::new(WaitJob {
            job: self.inner.clone(),
        })
    }

    #[napi]
    pub fn cancel(&self) -> Result<String> {
        encode(&self.inner.cancel())
    }
}

#[napi]
pub struct NativeClient {
    inner: OkcClient,
}

#[napi]
impl NativeClient {
    #[napi(constructor)]
    pub fn new(profiles_json: String, max_concurrent_jobs: Option<u32>) -> Result<Self> {
        let profiles: Vec<ProviderProfile> = decode(&profiles_json, "provider profiles")?;
        let max_concurrent_jobs = max_concurrent_jobs
            .map(usize::try_from)
            .transpose()
            .map_err(|_| napi_error(&OkcError::invalid_argument("job limit is out of range")))?;
        Ok(Self {
            inner: OkcClient::new(profiles, max_concurrent_jobs)
                .map_err(|error| napi_error(&error))?,
        })
    }

    #[napi]
    pub fn api_info_json(&self) -> Result<String> {
        encode(&self.inner.api_info())
    }

    #[napi]
    pub fn create_project(
        &self,
        path: String,
        name: String,
        curator_id: String,
        policy_version: String,
        language: Option<String>,
    ) -> NativeJob {
        NativeJob::new(
            self.inner
                .create_project(path, name, curator_id, policy_version, language)
                .erase(),
        )
    }

    #[napi]
    pub fn open_project(&self, path: String) -> NativeJob {
        NativeJob::new(self.inner.open_project(path).erase())
    }

    #[napi]
    pub fn project_handle(&self, path: String) -> Result<NativeProject> {
        Ok(NativeProject {
            inner: self
                .inner
                .project_handle(path)
                .map_err(|error| napi_error(&error))?,
        })
    }

    #[napi]
    pub fn test_provider(&self, name: String) -> NativeJob {
        NativeJob::new(self.inner.test_provider(name).erase())
    }

    #[napi]
    pub fn verify_artifact(&self, path: String) -> NativeJob {
        NativeJob::new(self.inner.verify_artifact(path).erase())
    }

    #[napi]
    pub fn explain_artifact(&self, path: String, output_path: String) -> NativeJob {
        NativeJob::new(self.inner.explain_artifact(path, output_path).erase())
    }
}

#[napi]
pub struct NativeProject {
    inner: Project,
}

#[napi]
impl NativeProject {
    #[napi]
    pub fn path(&self) -> String {
        self.inner.path().to_string_lossy().into_owned()
    }

    #[napi]
    pub fn manifest(&self) -> NativeJob {
        NativeJob::new(self.inner.manifest().erase())
    }

    #[napi]
    pub fn status(&self) -> NativeJob {
        NativeJob::new(self.inner.status().erase())
    }

    #[napi]
    pub fn add_source(&self, source_json: String) -> Result<NativeJob> {
        let source: SourceInput = decode(&source_json, "source")?;
        Ok(NativeJob::new(self.inner.add_source(source).erase()))
    }

    #[napi]
    pub fn rebind_source(
        &self,
        source_id: String,
        path: String,
        snapshot_id: Option<String>,
    ) -> NativeJob {
        NativeJob::new(
            self.inner
                .rebind_source(source_id, PathBuf::from(path), snapshot_id)
                .erase(),
        )
    }

    #[napi]
    pub fn replace_sources(&self, sources_json: String) -> Result<NativeJob> {
        let sources: Vec<SourceInput> = decode(&sources_json, "sources")?;
        Ok(NativeJob::new(self.inner.replace_sources(sources).erase()))
    }

    #[napi]
    pub fn set_language(&self, language: Option<String>) -> NativeJob {
        NativeJob::new(self.inner.set_language(language).erase())
    }

    #[napi]
    pub fn set_ai_route(&self, role: Option<String>, profile_name: String) -> Result<NativeJob> {
        Ok(NativeJob::new(
            self.inner
                .set_ai_route(role_from_name(role)?, profile_name)
                .erase(),
        ))
    }

    #[napi]
    pub fn preflight(&self) -> NativeJob {
        NativeJob::new(self.inner.preflight().erase())
    }

    #[napi]
    pub fn integrate(
        &self,
        allow_remote_provider: bool,
        remote_disclosure_confirmed: bool,
    ) -> NativeJob {
        NativeJob::new(
            self.inner
                .integrate(allow_remote_provider, remote_disclosure_confirmed)
                .erase(),
        )
    }

    #[napi]
    pub fn taxonomy(&self) -> NativeJob {
        NativeJob::new(self.inner.taxonomy().erase())
    }

    #[napi]
    pub fn approve_taxonomy(
        &self,
        edited_clusters_json: Option<String>,
        rationale: Option<String>,
    ) -> Result<NativeJob> {
        let edited_clusters = edited_clusters_json
            .as_deref()
            .map(|value| decode(value, "edited taxonomy clusters"))
            .transpose()?;
        Ok(NativeJob::new(
            self.inner
                .approve_taxonomy(edited_clusters, rationale)
                .erase(),
        ))
    }

    #[napi]
    pub fn clusters(&self) -> NativeJob {
        NativeJob::new(self.inner.clusters().erase())
    }

    #[napi]
    pub fn approve_cluster(
        &self,
        cluster_id: String,
        omission_rationales_json: String,
        minor_waivers_json: String,
    ) -> Result<NativeJob> {
        let omission_rationales: BTreeMap<String, String> =
            decode(&omission_rationales_json, "omission rationales")?;
        let minor_waivers: BTreeMap<String, String> = decode(&minor_waivers_json, "minor waivers")?;
        Ok(NativeJob::new(
            self.inner
                .approve_cluster(cluster_id, omission_rationales, minor_waivers)
                .erase(),
        ))
    }

    #[napi]
    pub fn regenerate_cluster(
        &self,
        cluster_id: String,
        feedback: String,
        allow_remote_provider: bool,
        remote_disclosure_confirmed: bool,
    ) -> NativeJob {
        NativeJob::new(
            self.inner
                .regenerate_cluster(
                    cluster_id,
                    feedback,
                    allow_remote_provider,
                    remote_disclosure_confirmed,
                )
                .erase(),
        )
    }

    #[napi]
    pub fn compile(&self, output: String) -> NativeJob {
        NativeJob::new(self.inner.compile(PathBuf::from(output)).erase())
    }
}

#[napi]
pub const INTEROP_SCHEMA_VERSION: u32 = okc_interop::INTEROP_SCHEMA_VERSION;
