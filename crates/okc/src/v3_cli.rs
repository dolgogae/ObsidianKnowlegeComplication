use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::{Subcommand, ValueEnum};
use okc_ai::{AiRole, ProviderKind, ProviderProfile};
use okc_app::v3::{ClusterTaskOutput, TaskStage, TaskStatus, TaxonomyTaskOutput};
use okc_app::{ProjectStore, SourceBinding};
use okc_core::integration::{
    ApprovedClusterRevision, ApprovedIntegrationPlan, CriticSeverity, explain_v3_directory,
    verify_v3_directory,
};
use okc_core::{CancellationToken, SourceId};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    CliFailure, CliResult, EXIT_DECISION, EXIT_INPUT, EXIT_OUTPUT, EXIT_PROVIDER, EXIT_USAGE,
    EXIT_VERIFY, OutputFormat, print_json, sanitize_terminal,
};

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Create a new absent schema-3 project.
    Create {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        curator: String,
        #[arg(long, default_value = "policy-v3")]
        policy_version: String,
        #[arg(long)]
        language: Option<String>,
    },
    /// Reconstruct a V3 project from V2 source bindings without mutating V2.
    Upgrade {
        #[arg(value_name = "V2_PROJECT")]
        source: PathBuf,
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
    },
    /// Add an immutable source binding to the selected project.
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    /// Configure a role-to-profile route in the selected project.
    AiRoute {
        #[command(subcommand)]
        command: AiRouteCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum SourceCommand {
    Add {
        #[arg(value_name = "SOURCE_ID")]
        source_id: String,
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long)]
        owner: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AiRouteCommand {
    Set {
        /// One PROFILE sets the default; ROLE PROFILE sets a role override.
        #[arg(value_name = "ROLE_OR_PROFILE", required = true, num_args = 1..=2)]
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliAiRole {
    Embedding,
    Organizer,
    Synthesis,
    Critic,
}

impl From<CliAiRole> for AiRole {
    fn from(value: CliAiRole) -> Self {
        match value {
            CliAiRole::Embedding => Self::Embedding,
            CliAiRole::Organizer => Self::Organizer,
            CliAiRole::Synthesis => Self::Synthesis,
            CliAiRole::Critic => Self::Critic,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum ProviderCommand {
    Add {
        name: String,
        #[arg(long, value_enum)]
        kind: CliProviderKind,
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        api_key_env: Option<String>,
        /// Account reference in the native credential store (never the secret).
        #[arg(long, conflicts_with = "api_key_env")]
        os_keychain: Option<String>,
    },
    List,
    Show {
        name: String,
    },
    Test {
        name: String,
    },
    Remove {
        name: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
    OpenAiCompatible,
    Command,
}

impl From<CliProviderKind> for ProviderKind {
    fn from(value: CliProviderKind) -> Self {
        match value {
            CliProviderKind::OpenAi => Self::OpenAi,
            CliProviderKind::Anthropic => Self::Anthropic,
            CliProviderKind::Gemini => Self::Gemini,
            CliProviderKind::Ollama => Self::Ollama,
            CliProviderKind::OpenAiCompatible => Self::OpenAiCompatible,
            CliProviderKind::Command => Self::Command,
        }
    }
}

#[derive(Debug, Clone, Copy, Subcommand)]
pub enum IntegrationCommand {
    Status {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyReviewCommand,
    },
    Cluster {
        #[command(subcommand)]
        command: ClusterReviewCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum TaxonomyReviewCommand {
    Show,
    Export {
        #[arg(long)]
        out: PathBuf,
    },
    Approve {
        /// Optional edited cluster array. The full edited taxonomy is resealed.
        #[arg(long, value_name = "FILE")]
        edited_clusters: Option<PathBuf>,
        #[arg(long)]
        rationale: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ClusterReviewCommand {
    List,
    Show {
        cluster_id: String,
    },
    Export {
        cluster_id: String,
        #[arg(long)]
        out: PathBuf,
    },
    Approve {
        cluster_id: String,
        /// Exact `document-id:target-id=rationale`; repeat for each omission.
        #[arg(long, value_name = "KEY=RATIONALE")]
        omission_rationale: Vec<String>,
        /// Exact `finding-id=rationale`; repeat for each minor finding.
        #[arg(long, value_name = "ID=RATIONALE")]
        minor_waiver: Vec<String>,
    },
    Regenerate {
        cluster_id: String,
        /// Curator feedback bound to the previous proposal and critic hashes.
        #[arg(long)]
        feedback: String,
    },
}

pub fn project_command(command: ProjectCommand, selected: Option<&Path>) -> CliResult<()> {
    match command {
        ProjectCommand::Create {
            path,
            name,
            curator,
            policy_version,
            language,
        } => {
            let mut project = ProjectStore::create(&path, name, curator, policy_version)
                .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
            if language.is_some() {
                project
                    .set_language(language)
                    .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
            }
            println!("created schema-3 project {}", path.display());
            Ok(())
        }
        ProjectCommand::Upgrade { source, out } => {
            okc_app::v3::upgrade_v2_project(&source, &out)
                .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
            println!(
                "created schema-3 project {} from V2 source bindings; V2 was not modified",
                out.display()
            );
            Ok(())
        }
        ProjectCommand::Source { command } => {
            let path = require_project(selected)?;
            let mut project = ProjectStore::open(path)
                .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
            match command {
                SourceCommand::Add {
                    source_id,
                    path,
                    owner,
                } => {
                    project
                        .add_source(SourceBinding {
                            source_id: SourceId::new(source_id)
                                .map_err(|error| CliFailure::from_error(EXIT_USAGE, error))?,
                            owner_display_name: owner,
                            path,
                            snapshot_id: None,
                        })
                        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
                    println!("source added; downstream approvals are stale in a new run");
                }
            }
            Ok(())
        }
        ProjectCommand::AiRoute { command } => {
            let path = require_project(selected)?;
            let mut project = ProjectStore::open(path)
                .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
            match command {
                AiRouteCommand::Set { values } => {
                    let (role, profile) = parse_ai_route_values(&values)?;
                    project
                        .set_ai_route(role, profile)
                        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
                    println!("AI route updated; dependent approvals are stale in a new run");
                }
            }
            Ok(())
        }
    }
}

fn parse_ai_route_values(values: &[String]) -> CliResult<(Option<AiRole>, String)> {
    match values {
        [profile] => Ok((None, profile.clone())),
        [role, profile] => {
            let role = <CliAiRole as ValueEnum>::from_str(role, true)
                .map_err(|message| CliFailure::new(EXIT_USAGE, message))?;
            Ok((Some(role.into()), profile.clone()))
        }
        _ => Err(CliFailure::new(
            EXIT_USAGE,
            "ai-route set requires PROFILE or ROLE PROFILE",
        )),
    }
}

pub fn provider_command(command: ProviderCommand) -> CliResult<()> {
    let service = okc_app::ProviderService::from_environment()
        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
    let path = service.config_path().to_path_buf();
    let config = service
        .load_config()
        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
    match command {
        ProviderCommand::Add {
            name,
            kind,
            endpoint,
            model,
            api_key_env,
            os_keychain,
        } => {
            validate_profile_name(&name)?;
            let profile = ProviderProfile {
                kind: kind.into(),
                endpoint,
                model,
                api_key_env,
                os_keychain,
                timeout_ms: okc_ai::DEFAULT_TIMEOUT_MS,
                max_response_bytes: okc_ai::DEFAULT_MAX_RESPONSE_BYTES,
                max_input_bytes: 64 * 1024 * 1024,
                max_batch_items: 2_048,
                options: BTreeMap::new(),
            };
            profile
                .validate()
                .map_err(|error| CliFailure::from_error(EXIT_USAGE, error))?;
            if config.profiles.contains_key(&name) {
                return Err(CliFailure::new(
                    EXIT_USAGE,
                    format!("provider profile `{name}` already exists"),
                ));
            }
            service
                .upsert_profile(&name, profile, None)
                .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
            println!("added provider profile `{name}` to {}", path.display());
            Ok(())
        }
        ProviderCommand::List => {
            for (name, profile) in &config.profiles {
                println!(
                    "{name}\t{:?}\t{}\t{:?}",
                    profile.kind,
                    profile.model,
                    profile.data_boundary()
                );
            }
            Ok(())
        }
        ProviderCommand::Show { name } => {
            let profile = config.profiles.get(&name).ok_or_else(|| {
                CliFailure::new(EXIT_USAGE, format!("unknown provider profile `{name}`"))
            })?;
            let value = serde_json::to_value(profile)
                .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&value)
                    .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
            );
            Ok(())
        }
        ProviderCommand::Remove { name } => {
            service
                .remove_profile(&name)
                .map_err(|error| CliFailure::from_error(EXIT_USAGE, error))?;
            println!("removed provider profile `{name}`");
            Ok(())
        }
        ProviderCommand::Test { name } => {
            service
                .test_profile(&name, &CancellationToken::default())
                .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?;
            println!("provider profile `{name}` passed schema-3 synthetic capability tests");
            Ok(())
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn integrate_command(
    selected: Option<&Path>,
    allow_remote_provider: bool,
    yes: bool,
) -> CliResult<()> {
    let path = require_project(selected)?;
    let service = okc_app::IntegrationService::new(
        path,
        okc_app::ProviderService::from_environment()
            .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?,
    );
    let execution = service
        .execute(
            allow_remote_provider,
            yes,
            &okc_app::OperationControl::quiet(),
        )
        .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?;
    println!("run: {}", execution.run_id);
    println!("Markdown documents: {}", execution.documents);
    println!("embedding inputs: {}", execution.embedding_inputs);
    println!("input bytes: {}", execution.input_bytes);
    println!(
        "sensitive findings after exceptions: {}",
        execution.sensitive_findings
    );
    println!("semantic candidates: {}", execution.semantic_candidates);
    match execution.checkpoint {
        okc_app::IntegrationCheckpoint::NeedsTaxonomy => println!(
            "taxonomy proposal awaits `okc --project {} review taxonomy approve`",
            path.display()
        ),
        okc_app::IntegrationCheckpoint::NeedsClusters => {
            println!("cluster revisions await individual review and approval");
        }
        okc_app::IntegrationCheckpoint::ReadyToCompile => {
            if let Some(plan_id) = execution.integration_plan_id {
                println!("approved integration plan: {plan_id}");
            }
        }
        checkpoint => println!("integration checkpoint: {checkpoint:?}"),
    }
    Ok(())
}

pub fn integration_command(command: IntegrationCommand, selected: Option<&Path>) -> CliResult<()> {
    let project = ProjectStore::open(require_project(selected)?)
        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
    match command {
        IntegrationCommand::Status { format } => {
            let status = project
                .integration_status()
                .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
            match format {
                OutputFormat::Json => print_json(&status),
                OutputFormat::Human => {
                    if let Some(run) = status.run {
                        println!("run: {}", run.run_id);
                        println!("tasks: {}", status.tasks.len());
                        println!("complete: {}", status.completed);
                        println!("failed: {}", status.failed);
                    } else {
                        println!("no integration run");
                    }
                    Ok(())
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn review_command(command: ReviewCommand, selected: Option<&Path>) -> CliResult<()> {
    let project = ProjectStore::open(require_project(selected)?)
        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
    let run = project
        .latest_run()
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
        .ok_or_else(|| CliFailure::new(EXIT_INPUT, "project has no integration run"))?;
    match command {
        ReviewCommand::Taxonomy { command } => {
            let proposed = latest_taxonomy_output(&project, &run)?;
            match command {
                TaxonomyReviewCommand::Show => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&proposed.taxonomy)
                            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
                    );
                }
                TaxonomyReviewCommand::Export { out } => {
                    write_new_json(&out, &proposed.taxonomy.clusters)?;
                    println!("exported editable taxonomy clusters to {}", out.display());
                }
                TaxonomyReviewCommand::Approve {
                    edited_clusters,
                    rationale,
                } => {
                    let clusters = if let Some(path) = edited_clusters {
                        Some(
                            serde_json::from_reader(
                                File::open(&path)
                                    .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?,
                            )
                            .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?,
                        )
                    } else {
                        None
                    };
                    let approved = okc_app::IntegrationService::new(
                        project.root(),
                        okc_app::ProviderService::from_environment()
                            .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?,
                    )
                    .approve_taxonomy(clusters, rationale)
                    .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
                    println!("approved taxonomy {}", approved.taxonomy.taxonomy_hash);
                }
            }
        }
        ReviewCommand::Cluster { command } => {
            let outputs = completed_cluster_outputs(&project, &run)?;
            match command {
                ClusterReviewCommand::List => {
                    for output in outputs {
                        let approval = project
                            .latest_approval::<ApprovedClusterRevision>(
                                &run.run_id,
                                "cluster",
                                &output.proposal.cluster_id,
                            )
                            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
                            .is_some_and(|approval| {
                                approval.target_hash == output.critic.critic_hash.hex()
                            });
                        let blocking = output
                            .critic
                            .findings
                            .iter()
                            .filter(|finding| {
                                matches!(
                                    finding.severity,
                                    CriticSeverity::Major | CriticSeverity::Critical
                                )
                            })
                            .count();
                        println!(
                            "{}\trevision={}\tfindings={}\tblocking={}\tapproved={approval}",
                            output.proposal.cluster_id,
                            output.proposal.revision,
                            output.critic.findings.len(),
                            blocking,
                        );
                    }
                }
                ClusterReviewCommand::Show { cluster_id } => {
                    let output = select_cluster_output(outputs, &cluster_id)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&output)
                            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
                    );
                }
                ClusterReviewCommand::Export { cluster_id, out } => {
                    let output = select_cluster_output(outputs, &cluster_id)?;
                    write_new_json(&out, &output)?;
                    println!("exported cluster review to {}", out.display());
                }
                ClusterReviewCommand::Approve {
                    cluster_id,
                    omission_rationale,
                    minor_waiver,
                } => {
                    let decision = okc_app::integration_service::ClusterReviewDecision {
                        omission_rationales: parse_rationale_map(&omission_rationale)?,
                        minor_waivers: parse_rationale_map(&minor_waiver)?,
                    };
                    let service = okc_app::IntegrationService::new(
                        project.root(),
                        okc_app::ProviderService::from_environment()
                            .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?,
                    );
                    let approved = service
                        .approve_cluster(&cluster_id, &decision)
                        .map_err(|error| CliFailure::from_error(EXIT_DECISION, error))?;
                    if let Some(plan) = project
                        .latest_approved_integration_plan()
                        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
                    {
                        println!("sealed integration plan {}", plan.integration_plan_id);
                    }
                    println!(
                        "approved cluster `{cluster_id}` revision {}",
                        approved.proposal.revision
                    );
                }
                ClusterReviewCommand::Regenerate {
                    cluster_id,
                    feedback,
                } => {
                    let service = okc_app::IntegrationService::new(
                        project.root(),
                        okc_app::ProviderService::from_environment()
                            .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?,
                    );
                    let request = service
                        .request_cluster_regeneration(&cluster_id, feedback)
                        .map_err(|error| CliFailure::from_error(EXIT_DECISION, error))?;
                    println!(
                        "requested cluster `{cluster_id}` revision {}; rerun integrate with any required remote consent",
                        request.revision
                    );
                }
            }
        }
    }
    Ok(())
}

fn parse_rationale_map(values: &[String]) -> CliResult<BTreeMap<String, String>> {
    let mut parsed = BTreeMap::new();
    for value in values {
        let (key, rationale) = value.split_once('=').ok_or_else(|| {
            CliFailure::new(EXIT_USAGE, "rationale entries must use KEY=RATIONALE")
        })?;
        if key.is_empty() || rationale.trim().is_empty() || parsed.contains_key(key) {
            return Err(CliFailure::new(
                EXIT_USAGE,
                "rationale keys must be unique and both sides must be non-empty",
            ));
        }
        parsed.insert(key.to_owned(), rationale.to_owned());
    }
    Ok(parsed)
}

fn latest_taxonomy_output(
    project: &ProjectStore,
    run: &okc_app::v3::RunRecord,
) -> CliResult<TaxonomyTaskOutput> {
    let task = project
        .tasks_for_stage(&run.run_id, TaskStage::Organizer)
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
        .into_iter()
        .find(|task| task.status == TaskStatus::Complete)
        .ok_or_else(|| {
            CliFailure::new(EXIT_INPUT, "integration has no complete taxonomy proposal")
        })?;
    project
        .complete_task_response(&task)
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))
}

fn completed_cluster_outputs(
    project: &ProjectStore,
    run: &okc_app::v3::RunRecord,
) -> CliResult<Vec<ClusterTaskOutput>> {
    project
        .tasks_for_stage(&run.run_id, TaskStage::Critic)
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
        .into_iter()
        .filter(|task| task.status == TaskStatus::Complete)
        .map(|task| {
            project
                .complete_task_response(&task)
                .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))
        })
        .collect()
}

fn select_cluster_output(
    outputs: Vec<ClusterTaskOutput>,
    cluster_id: &str,
) -> CliResult<ClusterTaskOutput> {
    outputs
        .into_iter()
        .filter(|output| output.proposal.cluster_id == cluster_id)
        .max_by_key(|output| output.proposal.revision)
        .ok_or_else(|| {
            CliFailure::new(
                EXIT_INPUT,
                format!(
                    "cluster `{}` has no completed critic report",
                    sanitize_terminal(cluster_id)
                ),
            )
        })
}

fn write_new_json(path: &Path, value: &impl Serialize) -> CliResult<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
    serde_json::to_writer_pretty(&mut file, value)
        .and_then(|()| file.write_all(b"\n").map_err(serde_json::Error::io))
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
    file.sync_all()
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))
}

pub fn compile_command(
    selected: Option<&Path>,
    plan_path: &Path,
    output: &Path,
    format: OutputFormat,
) -> CliResult<()> {
    let plan: ApprovedIntegrationPlan = serde_json::from_reader(
        File::open(plan_path).map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?,
    )
    .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
    plan.validate()
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
    let artifact = if let Some(project_path) = selected {
        ProjectStore::open(project_path)
            .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?
            .compile_v3(&plan, output)
            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
    } else {
        okc_core::integration::compile_approved_integration(&plan, output)
            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
    };
    verify_v3_directory(&artifact.path)
        .map_err(|error| CliFailure::from_error(EXIT_VERIFY, error))?;
    match format {
        OutputFormat::Json => print_json(&json!({
            "schema_version": 3,
            "path": artifact.path,
            "integration_plan_id": artifact.integration_plan_id,
            "files": artifact.files
        })),
        OutputFormat::Human => {
            println!(
                "compiled and independently verified V3 Vault: {}",
                artifact.path.display()
            );
            println!("integration plan: {}", artifact.integration_plan_id);
            Ok(())
        }
    }
}

pub fn compile_latest_command(
    project_path: &Path,
    output: &Path,
    format: OutputFormat,
) -> CliResult<()> {
    let providers = okc_app::ProviderService::from_environment()
        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
    let service = okc_app::IntegrationService::new(project_path, providers);
    let (artifact, _) = service
        .compile_latest(output, &okc_app::OperationControl::quiet())
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
    match format {
        OutputFormat::Json => print_json(&json!({
            "schema_version": 3,
            "path": artifact.path,
            "integration_plan_id": artifact.integration_plan_id,
            "files": artifact.files
        })),
        OutputFormat::Human => {
            println!(
                "compiled and independently verified V3 Vault: {}",
                artifact.path.display()
            );
            println!("integration plan: {}", artifact.integration_plan_id);
            Ok(())
        }
    }
}

pub fn is_v3_artifact(path: &Path) -> bool {
    let manifest = path.join(".okc/manifest.json");
    fs::read(manifest)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|value| value.get("schema_version").and_then(Value::as_u64) == Some(3))
}

pub fn verify_command(artifact: &Path, format: OutputFormat) -> CliResult<()> {
    let manifest = verify_v3_directory(artifact)
        .map_err(|error| CliFailure::from_error(EXIT_VERIFY, error))?;
    match format {
        OutputFormat::Json => print_json(&manifest),
        OutputFormat::Human => {
            println!("valid V3 artifact: true");
            println!("integration plan: {}", manifest.integration_plan_id);
            println!("checked files: {}", manifest.files.len() + 2);
            Ok(())
        }
    }
}

pub fn explain_command(
    artifact: &Path,
    output_path: Option<&str>,
    package: bool,
    format: OutputFormat,
) -> CliResult<()> {
    if package {
        return Err(CliFailure::new(
            EXIT_USAGE,
            "schema-3 directory explanation requires an output path; V3 Pack is not enabled",
        ));
    }
    let output_path = output_path
        .ok_or_else(|| CliFailure::new(EXIT_USAGE, "schema-3 explain requires an output path"))?;
    let record = explain_v3_directory(artifact, output_path)
        .map_err(|error| CliFailure::from_error(EXIT_VERIFY, error))?;
    match format {
        OutputFormat::Json => print_json(&record),
        OutputFormat::Human => {
            println!("V3 provenance record: {}", record.record_id);
            println!("kind: {}", record.kind);
            println!("integration plan: {}", record.integration_plan_id);
            println!("evidence references: {}", record.evidence.len());
            Ok(())
        }
    }
}

fn require_project(selected: Option<&Path>) -> CliResult<&Path> {
    selected.ok_or_else(|| CliFailure::new(EXIT_USAGE, "this command requires --project PATH"))
}

fn validate_profile_name(name: &str) -> CliResult<()> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Err(CliFailure::new(
            EXIT_USAGE,
            format!(
                "invalid provider profile name `{}`",
                sanitize_terminal(name)
            ),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_are_bounded_and_shell_free_data() {
        validate_profile_name("local_ollama-1").expect("valid profile");
        assert!(validate_profile_name("bad/profile").is_err());
        assert!(validate_profile_name("").is_err());
    }

    #[test]
    fn command_shapes_are_parseable_by_clap() {
        use clap::Parser as _;

        let cli = crate::Cli::try_parse_from([
            "okc",
            "--project",
            "Knowledge.okc-project",
            "integration",
            "status",
            "--format",
            "json",
        ])
        .expect("schema-3 command");
        assert!(matches!(
            cli.command,
            Some(crate::CommandKind::Integration { .. })
        ));

        let review = crate::Cli::try_parse_from([
            "okc",
            "--project",
            "Knowledge.okc-project",
            "review",
            "cluster",
            "approve",
            "cluster-one",
        ])
        .expect("review command");
        assert!(matches!(
            review.command,
            Some(crate::CommandKind::Review { .. })
        ));

        for arguments in [
            vec!["okc", "project", "ai-route", "set", "local-default"],
            vec![
                "okc",
                "project",
                "ai-route",
                "set",
                "embedding",
                "local-embedding",
            ],
        ] {
            crate::Cli::try_parse_from(arguments).expect("AI route command");
        }
        assert!(crate::Cli::try_parse_from(["okc", "project", "ai-route", "set"]).is_err());
        assert_eq!(
            parse_ai_route_values(&["critic".into(), "critic-local".into()]).expect("role route"),
            (Some(AiRole::Critic), "critic-local".into())
        );
    }
}
