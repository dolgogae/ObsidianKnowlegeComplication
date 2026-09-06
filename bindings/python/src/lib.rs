use std::collections::BTreeMap;
use std::path::PathBuf;

use okc_interop::{Job, OkcClient, OkcError, Project, ProviderProfile, SourceInput};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn encode<T: Serialize>(value: &T) -> PyResult<String> {
    serde_json::to_string(value).map_err(|_| {
        py_error(&OkcError::internal(
            "could not serialize a Python binding result",
        ))
    })
}

fn decode<T: DeserializeOwned>(value: &str, label: &str) -> PyResult<T> {
    serde_json::from_str(value).map_err(|error| {
        py_error(&OkcError::invalid_argument(format!(
            "{label} must be valid schema-version-1 JSON ({:?})",
            error.classify()
        )))
    })
}

fn py_error(error: &OkcError) -> PyErr {
    let message = serde_json::to_string(&error).unwrap_or_else(|_| {
        "{\"code\":\"INTERNAL\",\"category\":\"internal\",\"message\":\"could not serialize a structured error\",\"retryable\":false,\"details\":{}}".into()
    });
    PyRuntimeError::new_err(message)
}

fn role_from_name(value: Option<&str>) -> PyResult<Option<okc_interop::AiRole>> {
    value
        .map(|value| match value {
            "embedding" => Ok(okc_interop::AiRole::Embedding),
            "organizer" => Ok(okc_interop::AiRole::Organizer),
            "synthesis" => Ok(okc_interop::AiRole::Synthesis),
            "critic" => Ok(okc_interop::AiRole::Critic),
            _ => Err(py_error(&OkcError::invalid_argument(
                "role must be embedding, organizer, synthesis, critic, or None",
            ))),
        })
        .transpose()
}

#[pyclass]
struct NativeJob {
    inner: Job<Value>,
}

impl NativeJob {
    fn new(inner: Job<Value>) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl NativeJob {
    fn state(&self) -> PyResult<String> {
        encode(&self.inner.state())
    }

    fn events_json(&self) -> PyResult<String> {
        encode(&self.inner.events())
    }

    fn result_json(&self, py: Python<'_>) -> PyResult<String> {
        let result = py.detach(|| self.inner.result());
        encode(&result.map_err(|error| py_error(&error))?)
    }

    fn cancel(&self) -> PyResult<String> {
        encode(&self.inner.cancel())
    }
}

#[pyclass]
struct NativeClient {
    inner: OkcClient,
}

#[pymethods]
impl NativeClient {
    #[new]
    #[pyo3(signature = (profiles_json = "[]", max_concurrent_jobs = None))]
    fn new(profiles_json: &str, max_concurrent_jobs: Option<usize>) -> PyResult<Self> {
        let profiles: Vec<ProviderProfile> = decode(profiles_json, "provider profiles")?;
        Ok(Self {
            inner: OkcClient::new(profiles, max_concurrent_jobs)
                .map_err(|error| py_error(&error))?,
        })
    }

    fn api_info_json(&self) -> PyResult<String> {
        encode(&self.inner.api_info())
    }

    #[pyo3(signature = (path, name, curator_id, policy_version, language = None))]
    fn create_project(
        &self,
        path: &str,
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

    fn open_project(&self, path: &str) -> NativeJob {
        NativeJob::new(self.inner.open_project(path).erase())
    }

    fn project_handle(&self, path: &str) -> PyResult<NativeProject> {
        Ok(NativeProject {
            inner: self
                .inner
                .project_handle(path)
                .map_err(|error| py_error(&error))?,
        })
    }

    fn test_provider(&self, name: String) -> NativeJob {
        NativeJob::new(self.inner.test_provider(name).erase())
    }

    fn verify_artifact(&self, path: &str) -> NativeJob {
        NativeJob::new(self.inner.verify_artifact(path).erase())
    }

    #[pyo3(signature = (path, output_path = None, package = false, limit = None, cursor = None))]
    fn explain_artifact(
        &self,
        path: &str,
        output_path: Option<String>,
        package: bool,
        limit: Option<usize>,
        cursor: Option<String>,
    ) -> NativeJob {
        NativeJob::new(
            self.inner
                .explain_artifact(path, output_path, package, limit, cursor)
                .erase(),
        )
    }
}

#[pyclass]
struct NativeProject {
    inner: Project,
}

#[pymethods]
impl NativeProject {
    fn path(&self) -> String {
        self.inner.path().to_string_lossy().into_owned()
    }

    fn manifest(&self) -> NativeJob {
        NativeJob::new(self.inner.manifest().erase())
    }

    fn status(&self) -> NativeJob {
        NativeJob::new(self.inner.status().erase())
    }

    fn add_source(&self, source_json: &str) -> PyResult<NativeJob> {
        let source: SourceInput = decode(source_json, "source")?;
        Ok(NativeJob::new(self.inner.add_source(source).erase()))
    }

    #[pyo3(signature = (source_id, path, snapshot_id = None))]
    fn rebind_source(
        &self,
        source_id: String,
        path: &str,
        snapshot_id: Option<String>,
    ) -> NativeJob {
        NativeJob::new(
            self.inner
                .rebind_source(source_id, PathBuf::from(path), snapshot_id)
                .erase(),
        )
    }

    fn replace_sources(&self, sources_json: &str) -> PyResult<NativeJob> {
        let sources: Vec<SourceInput> = decode(sources_json, "sources")?;
        Ok(NativeJob::new(self.inner.replace_sources(sources).erase()))
    }

    fn set_language(&self, language: Option<String>) -> NativeJob {
        NativeJob::new(self.inner.set_language(language).erase())
    }

    fn set_ai_route(&self, role: Option<&str>, profile_name: String) -> PyResult<NativeJob> {
        Ok(NativeJob::new(
            self.inner
                .set_ai_route(role_from_name(role)?, profile_name)
                .erase(),
        ))
    }

    fn preflight(&self) -> NativeJob {
        NativeJob::new(self.inner.preflight().erase())
    }

    fn integrate(
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

    fn taxonomy(&self) -> NativeJob {
        NativeJob::new(self.inner.taxonomy().erase())
    }

    #[pyo3(signature = (edited_clusters_json = None, rationale = None))]
    fn approve_taxonomy(
        &self,
        edited_clusters_json: Option<&str>,
        rationale: Option<String>,
    ) -> PyResult<NativeJob> {
        let edited_clusters = edited_clusters_json
            .map(|value| decode(value, "edited taxonomy clusters"))
            .transpose()?;
        Ok(NativeJob::new(
            self.inner
                .approve_taxonomy(edited_clusters, rationale)
                .erase(),
        ))
    }

    fn clusters(&self) -> NativeJob {
        NativeJob::new(self.inner.clusters().erase())
    }

    #[pyo3(signature = (cluster_id, omission_rationales_json = "{}", minor_waivers_json = "{}"))]
    fn approve_cluster(
        &self,
        cluster_id: String,
        omission_rationales_json: &str,
        minor_waivers_json: &str,
    ) -> PyResult<NativeJob> {
        let omission_rationales: BTreeMap<String, String> =
            decode(omission_rationales_json, "omission rationales")?;
        let minor_waivers: BTreeMap<String, String> = decode(minor_waivers_json, "minor waivers")?;
        Ok(NativeJob::new(
            self.inner
                .approve_cluster(cluster_id, omission_rationales, minor_waivers)
                .erase(),
        ))
    }

    fn regenerate_cluster(
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

    fn compile(&self, output: &str) -> NativeJob {
        NativeJob::new(self.inner.compile(PathBuf::from(output)).erase())
    }
}

#[pyfunction]
fn interop_schema_version() -> u32 {
    okc_interop::INTEROP_SCHEMA_VERSION
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeClient>()?;
    module.add_class::<NativeProject>()?;
    module.add_class::<NativeJob>()?;
    module.add_function(wrap_pyfunction!(interop_schema_version, module)?)?;
    Ok(())
}
