mod command_provider;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::builder::{OsStringValueParser, TypedValueParser as _};
use clap::{Parser, Subcommand, ValueEnum};
use command_provider::{CommandProvider, CommandProviderConfig, ProviderCancellation};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use vaultc::approval::{ApprovalLog, ApprovedPlan, ConflictDecision, ConflictDecisionLog};
use vaultc::augmentation::{DocumentSelection, RecordedAugmentation, RemoteProviderConsent};
use vaultc::identity::DocumentId;
use vaultc::plan::{DraftPlan, Inspection};
use vaultc::provider::ValidatedProposals;
use vaultc::{CompileOptions, CompilerPolicy, SourceId, SourceSpec, VaultCompiler, VaultcError};

const EXIT_USAGE: u8 = 2;
const EXIT_INPUT: u8 = 3;
const EXIT_DECISION: u8 = 4;
const EXIT_PROVIDER: u8 = 5;
const EXIT_OUTPUT: u8 = 6;
const EXIT_VERIFY: u8 = 7;
const EXIT_INTERNAL: u8 = 70;
const CONTROL_FILE_LIMIT: u64 = 1024 * 1024 * 1024;
const DECISIONS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Parser)]
#[command(
    name = "vaultc",
    version,
    about = "Compile immutable Obsidian Vault snapshots into an auditable Vault"
)]
struct Cli {
    /// TOML compiler policy. Defaults to the V1 policy.
    #[arg(long, global = true, value_name = "FILE")]
    policy: Option<PathBuf>,

    /// `SQLite` build workspace used by inspect and plan.
    #[arg(
        long,
        global = true,
        value_name = "FILE",
        default_value = ".vaultc-work/build.sqlite"
    )]
    workspace: PathBuf,

    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// Seal and inspect one or more immutable Vault sources.
    Inspect {
        /// Source as ID=PATH. PATH alone derives ID from its final component.
        #[arg(required = true, value_name = "SOURCE")]
        sources: Vec<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },

    /// Inspect sources and emit a deterministic draft plan.
    Plan {
        /// Source as ID=PATH. PATH alone derives ID from its final component.
        #[arg(required = true, value_name = "SOURCE")]
        sources: Vec<String>,
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },

    /// Request evidence-bound proposals through the NDJSON subprocess protocol.
    Augment {
        #[arg(value_name = "PLAN")]
        plan: PathBuf,
        /// Executable path or name. It is launched directly, never through a shell.
        #[arg(long, value_name = "PROGRAM")]
        provider_cmd: PathBuf,
        /// Argument passed verbatim to the provider executable.
        #[arg(long, value_name = "ARG", allow_hyphen_values = true)]
        provider_arg: Vec<OsString>,
        #[arg(long, value_name = "DIRECTORY")]
        provider_working_directory: Option<PathBuf>,
        #[arg(long, default_value_t = 120, value_name = "SECONDS")]
        provider_timeout_seconds: u64,
        #[arg(long, default_value_t = 16 * 1024 * 1024, value_name = "BYTES")]
        provider_max_output_bytes: usize,
        #[arg(long, default_value_t = 16 * 1024 * 1024, value_name = "BYTES")]
        provider_max_line_bytes: usize,
        #[arg(long, default_value_t = 8, value_name = "COUNT")]
        provider_max_messages: usize,
        /// Select a document to disclose. Repeat to select more than one.
        #[arg(long, value_name = "DOCUMENT_ID")]
        document_id: Vec<String>,
        /// Explicitly disclose every document projection in the sealed plan.
        #[arg(long, conflicts_with = "document_id")]
        all_documents: bool,
        /// Additional operator consent for a plan whose policy permits remote providers.
        #[arg(long)]
        allow_remote_provider: bool,
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },

    /// Revalidate a canonical augmentation recording without contacting a provider.
    Replay {
        #[arg(value_name = "PLAN")]
        plan: PathBuf,
        #[arg(long, value_name = "FILE")]
        augmentation: PathBuf,
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },

    /// Bind decisions to a plan and produce an approved immutable plan.
    /// V1 conflict overlays support explicit `waived_by_policy` only.
    Approve {
        #[arg(value_name = "PLAN")]
        plan: PathBuf,
        /// Versioned proposal decisions and conflict waivers as JSON.
        #[arg(long, value_name = "FILE")]
        decisions: PathBuf,
        /// Augmentation JSONL emitted by `vaultc augment`.
        #[arg(long, value_name = "FILE")]
        proposals: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },

    /// Materialize an approved plan into a new destination atomically.
    Compile {
        #[arg(value_name = "APPROVED_PLAN")]
        approved_plan: PathBuf,
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
        #[arg(
            long,
            value_name = "FILE",
            value_parser = OsStringValueParser::new().try_map(parse_vaultpack_path)
        )]
        pack: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },

    /// Independently verify a Compiled Vault directory or `VaultPack`.
    Verify {
        #[arg(value_name = "PATH_OR_PACK")]
        artifact: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },

    /// Explain output provenance without contacting a provider.
    Explain {
        #[arg(value_name = "PATH_OR_PACK")]
        artifact: PathBuf,
        #[arg(
            value_name = "OUTPUT_PATH",
            required_unless_present = "package",
            conflicts_with = "package"
        )]
        output_path: Option<String>,
        /// Explain the virtual package record instead of an inner artifact path.
        #[arg(long, conflicts_with = "output_path")]
        package: bool,
        /// Maximum records returned in this page.
        #[arg(
            long,
            value_name = "COUNT",
            default_value_t = 256,
            value_parser = clap::value_parser!(u16).range(1..=4096)
        )]
        limit: u16,
        /// Opaque cursor returned by the preceding provenance page.
        #[arg(long, value_name = "CURSOR")]
        cursor: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Human => "human",
            Self::Json => "json",
        })
    }
}

#[derive(Debug)]
struct CliFailure {
    code: u8,
    message: String,
}

type CliResult<T> = Result<T, CliFailure>;

impl CliFailure {
    fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn from_error(code: u8, error: impl std::fmt::Display) -> Self {
        Self::new(code, error.to_string())
    }

    #[allow(clippy::needless_pass_by_value)]
    fn from_vaultc(default_code: u8, error: VaultcError) -> Self {
        let code = if vaultc_error_contains_internal(&error) {
            EXIT_INTERNAL
        } else {
            default_code
        };
        Self::new(code, error.to_string())
    }
}

fn vaultc_error_contains_internal(error: &VaultcError) -> bool {
    match error {
        VaultcError::Internal(_) => true,
        VaultcError::PackPublicationAfterCompile { source, .. } => {
            vaultc_error_contains_internal(source)
        }
        _ => false,
    }
}

fn plan_failure(error: &VaultcError) -> CliFailure {
    let code = match error {
        VaultcError::UnsafePath { .. }
        | VaultcError::UnsupportedSource(_)
        | VaultcError::ResourceLimit(_)
        | VaultcError::MalformedInput { .. }
        | VaultcError::IdentityMismatch(_)
        | VaultcError::Io { .. } => EXIT_INPUT,
        VaultcError::InvalidConfig(_) => EXIT_USAGE,
        VaultcError::PlanStale(_) => EXIT_DECISION,
        _ => EXIT_INTERNAL,
    };
    CliFailure::new(code, error.to_string())
}

fn parse_vaultpack_path(value: OsString) -> Result<PathBuf, &'static str> {
    let path = PathBuf::from(value);
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("vaultpack"))
    {
        Ok(path)
    } else {
        Err("VaultPack destination must use the .vaultpack extension")
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionDocument {
    schema_version: u32,
    plan_id: String,
    #[serde(default)]
    decisions: Vec<vaultc::ApprovalDecision>,
    #[serde(default)]
    conflicts: Vec<ConflictDecision>,
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(EXIT_USAGE));
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vaultc: {}", sanitize_terminal(&error.message));
            ExitCode::from(error.code)
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> CliResult<()> {
    match cli.command {
        CommandKind::Inspect { sources, format } => {
            let policy = load_policy(cli.policy.as_deref())?;
            let compiler = build_compiler(policy, Some(&cli.workspace), EXIT_USAGE)?;
            let inspection = compiler
                .inspect(parse_sources(&sources)?)
                .map_err(|error| CliFailure::from_vaultc(EXIT_INPUT, error))?;
            print_inspection(&inspection, format)
        }
        CommandKind::Plan {
            sources,
            out,
            format,
        } => {
            let policy = load_policy(cli.policy.as_deref())?;
            let compiler = build_compiler(policy, Some(&cli.workspace), EXIT_USAGE)?;
            let inspection = compiler
                .inspect(parse_sources(&sources)?)
                .map_err(|error| CliFailure::from_vaultc(EXIT_INPUT, error))?;
            let plan = compiler
                .plan(&inspection)
                .map_err(|error| plan_failure(&error))?;
            write_canonical_json(&out, &plan)?;
            print_plan(&plan, &out, format)?;
            let unresolved = plan.unresolved_required_conflicts().count();
            if unresolved > 0 {
                return Err(CliFailure::new(
                    EXIT_DECISION,
                    format!(
                        "wrote `{}` with {unresolved} required conflict(s) awaiting a decision",
                        out.display()
                    ),
                ));
            }
            Ok(())
        }
        CommandKind::Augment {
            plan,
            provider_cmd,
            provider_arg,
            provider_working_directory,
            provider_timeout_seconds,
            provider_max_output_bytes,
            provider_max_line_bytes,
            provider_max_messages,
            document_id,
            all_documents,
            allow_remote_provider,
            out,
        } => augment_command(
            &plan,
            provider_cmd,
            provider_arg,
            provider_working_directory,
            provider_timeout_seconds,
            provider_max_output_bytes,
            provider_max_line_bytes,
            provider_max_messages,
            &document_id,
            all_documents,
            allow_remote_provider,
            &out,
        ),
        CommandKind::Replay {
            plan,
            augmentation,
            out,
        } => replay_command(&plan, &augmentation, &out),
        CommandKind::Approve {
            plan,
            decisions,
            proposals,
            out,
        } => approve_command(&plan, &decisions, proposals.as_deref(), &out),
        CommandKind::Compile {
            approved_plan,
            output,
            pack,
            format,
        } => compile_command(
            cli.policy.as_deref(),
            &approved_plan,
            &output,
            pack.as_deref(),
            format,
        ),
        CommandKind::Verify { artifact, format } => verify_command(&artifact, format),
        CommandKind::Explain {
            artifact,
            output_path,
            package,
            limit,
            cursor,
            format,
        } => explain_command(
            &artifact,
            output_path.as_deref(),
            package,
            usize::from(limit),
            cursor.as_deref(),
            format,
        ),
    }
}

fn verify_command(artifact: &Path, format: OutputFormat) -> CliResult<()> {
    let compiler = build_compiler(CompilerPolicy::default(), None, EXIT_VERIFY)?;
    let mut report = compiler
        .verify(artifact)
        .map_err(|error| CliFailure::from_vaultc(EXIT_VERIFY, error))?;
    // Pack verification happens in a private extraction directory. Do not
    // leak that ephemeral implementation path through the public CLI.
    report.artifact_path = artifact.to_path_buf();
    print_verification(&report, format)
}

fn explain_command(
    artifact: &Path,
    output_path: Option<&str>,
    package: bool,
    limit: usize,
    cursor: Option<&str>,
    format: OutputFormat,
) -> CliResult<()> {
    let compiler = build_compiler(CompilerPolicy::default(), None, EXIT_VERIFY)?;
    let mut query = if package {
        vaultc::provenance::ProvenanceQuery::package()
    } else {
        let output_path = output_path.ok_or_else(|| {
            CliFailure::new(
                EXIT_USAGE,
                "explain requires OUTPUT_PATH or the mutually exclusive --package flag",
            )
        })?;
        vaultc::provenance::ProvenanceQuery::artifact_path(output_path.to_owned())
    };
    query = query
        .with_limit(limit)
        .map_err(|error| CliFailure::from_vaultc(EXIT_USAGE, error))?;
    if let Some(cursor) = cursor {
        query = query.with_cursor(cursor.to_owned());
    }
    let page = compiler
        .explain_provenance_page(artifact, &query)
        .map_err(|error| CliFailure::from_vaultc(EXIT_VERIFY, error))?;
    print_provenance(&page, format)
}

#[allow(clippy::too_many_arguments)]
fn augment_command(
    plan_path: &Path,
    provider_cmd: PathBuf,
    provider_arguments: Vec<OsString>,
    provider_working_directory: Option<PathBuf>,
    provider_timeout_seconds: u64,
    provider_max_output_bytes: usize,
    provider_max_line_bytes: usize,
    provider_max_messages: usize,
    requested_documents: &[String],
    all_documents: bool,
    allow_remote_provider: bool,
    out: &Path,
) -> CliResult<()> {
    if requested_documents.is_empty() && !all_documents {
        return Err(CliFailure::new(
            EXIT_USAGE,
            "augmentation requires at least one --document-id or explicit --all-documents",
        ));
    }
    let plan: DraftPlan = read_json(plan_path, CONTROL_FILE_LIMIT, EXIT_PROVIDER)?;
    plan.validate_integrity()
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    let compiler = build_compiler(plan.policy.clone(), None, EXIT_PROVIDER)?;
    let selection = document_selection(requested_documents, all_documents)?;
    let request = compiler
        .build_augmentation_request(&plan, &selection)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    let consent = if allow_remote_provider {
        RemoteProviderConsent::Granted
    } else {
        RemoteProviderConsent::Denied
    };
    let cancellation = ProviderCancellation::default();
    install_provider_cancellation(&cancellation)?;
    let provider = CommandProvider::new(CommandProviderConfig {
        program: provider_cmd,
        arguments: provider_arguments,
        working_directory: provider_working_directory,
        timeout: Duration::from_secs(provider_timeout_seconds),
        max_line_bytes: provider_max_line_bytes,
        max_output_bytes: provider_max_output_bytes,
        max_messages: provider_max_messages,
        cancellation: cancellation.clone(),
    })
    .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    let run = provider
        .augment(&request, |capabilities| {
            compiler.authorize_augmentation_exchange(&plan, &request, capabilities, consent)
        })
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    if cancellation.is_cancelled() {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "provider operation was cancelled",
        ));
    }
    let recording = compiler
        .record_augmentation_exchange(&plan, run.authorization, &run.response)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    let bytes = recording
        .to_canonical_jsonl()
        .map_err(|error| CliFailure::from_vaultc(EXIT_INTERNAL, error))?;
    write_atomic(out, &bytes)?;
    let rejected = recording
        .validations()
        .iter()
        .filter(|validation| !validation.valid)
        .count();
    println!(
        "wrote {} proposal validation(s) to {} ({} rejected)",
        recording.validations().len(),
        out.display(),
        rejected
    );
    if rejected > 0 {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "provider output contained invalid proposals; inspect the emitted validation records",
        ));
    }
    Ok(())
}

fn document_selection(requested: &[String], all_documents: bool) -> CliResult<DocumentSelection> {
    if all_documents {
        return Ok(DocumentSelection::All);
    }
    let mut seen = BTreeSet::new();
    let mut document_ids = Vec::with_capacity(requested.len());
    for value in requested {
        let document_id = value.parse::<DocumentId>().map_err(|error| {
            CliFailure::new(
                EXIT_USAGE,
                format!("invalid --document-id `{value}`: {error}"),
            )
        })?;
        if !seen.insert(document_id) {
            return Err(CliFailure::new(EXIT_USAGE, "duplicate --document-id value"));
        }
        document_ids.push(document_id);
    }
    Ok(DocumentSelection::Explicit(document_ids))
}

fn replay_command(plan_path: &Path, augmentation_path: &Path, out: &Path) -> CliResult<()> {
    let plan: DraftPlan = read_json(plan_path, CONTROL_FILE_LIMIT, EXIT_PROVIDER)?;
    plan.validate_integrity()
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    let compiler = build_compiler(plan.policy.clone(), None, EXIT_PROVIDER)?;
    let input = read_control_bytes(augmentation_path, CONTROL_FILE_LIMIT, EXIT_PROVIDER)?;
    let recording = RecordedAugmentation::from_canonical_jsonl(&input)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    let replayed = compiler
        .replay_augmentation(&plan, &recording)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    let output = replayed
        .to_canonical_jsonl()
        .map_err(|error| CliFailure::from_vaultc(EXIT_INTERNAL, error))?;
    if output != input {
        return Err(CliFailure::new(
            EXIT_INTERNAL,
            "canonical augmentation replay changed the recording bytes",
        ));
    }
    write_atomic(out, &output)?;
    println!(
        "replayed augmentation for plan {} to {}",
        plan.plan_id,
        out.display()
    );
    Ok(())
}

fn install_provider_cancellation(cancellation: &ProviderCancellation) -> CliResult<()> {
    let signal_token = cancellation.clone();
    ctrlc::set_handler(move || signal_token.cancel()).map_err(|error| {
        CliFailure::new(
            EXIT_PROVIDER,
            format!("failed to install provider cancellation handler: {error}"),
        )
    })
}

fn approve_command(
    plan_path: &Path,
    decisions_path: &Path,
    proposals_path: Option<&Path>,
    out: &Path,
) -> CliResult<()> {
    let plan: DraftPlan = read_json(plan_path, CONTROL_FILE_LIMIT, EXIT_DECISION)?;
    plan.validate_integrity()
        .map_err(|error| CliFailure::from_vaultc(EXIT_DECISION, error))?;
    let decision_document: DecisionDocument =
        read_json(decisions_path, CONTROL_FILE_LIMIT, EXIT_PROVIDER)?;
    if decision_document.schema_version != DECISIONS_SCHEMA_VERSION {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            format!(
                "unsupported decisions schema version {}",
                decision_document.schema_version
            ),
        ));
    }
    let plan_id = plan.plan_id.to_string();
    if decision_document.plan_id != plan_id {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "decisions belong to a different plan",
        ));
    }
    let compiler = build_compiler(plan.policy.clone(), None, EXIT_PROVIDER)?;
    let conflict_log = ConflictDecisionLog {
        decisions: decision_document.conflicts,
    };
    // Validate the conflict overlay independently so conflict-action failures
    // retain exit family 4 instead of being conflated with provider failures.
    vaultc::approval::validate_conflict_decisions(&plan, &conflict_log.decisions)
        .map_err(|error| CliFailure::from_vaultc(EXIT_DECISION, error))?;
    let decided_conflicts: BTreeSet<_> = conflict_log
        .decisions
        .iter()
        .map(|decision| decision.conflict_id.as_str())
        .collect();
    let unresolved = plan
        .unresolved_required_conflicts()
        .filter(|conflict| !decided_conflicts.contains(conflict.conflict_id.as_str()))
        .count();
    if unresolved > 0 {
        return Err(CliFailure::new(
            EXIT_DECISION,
            format!("{unresolved} required conflict(s) still need explicit decisions"),
        ));
    }
    let validated = if let Some(path) = proposals_path {
        load_augmentation(path, &plan, &compiler)?
    } else {
        ValidatedProposals::default()
    };
    validate_approval_decisions(&plan, &validated, &decision_document.decisions)?;
    let approval_log = ApprovalLog {
        decisions: decision_document.decisions,
    };
    let approved = compiler
        .approve_with_conflicts(plan, validated, approval_log, conflict_log)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    write_canonical_json(out, &approved)?;
    println!(
        "wrote approved plan {} with {} approved proposal(s) to {}",
        approved.plan.plan_id,
        approved.approved_proposals.len(),
        out.display()
    );
    Ok(())
}

fn compile_command(
    policy_path: Option<&Path>,
    approved_plan_path: &Path,
    output: &Path,
    pack: Option<&Path>,
    format: OutputFormat,
) -> CliResult<()> {
    let approved: ApprovedPlan = read_json(approved_plan_path, CONTROL_FILE_LIMIT, EXIT_OUTPUT)?;
    approved
        .plan
        .validate_integrity()
        .map_err(|error| CliFailure::from_vaultc(EXIT_OUTPUT, error))?;
    if let Some(path) = policy_path {
        let supplied = CompilerPolicy::from_file(path)
            .map_err(|error| CliFailure::from_vaultc(EXIT_USAGE, error))?;
        let supplied_hash = supplied
            .semantic_hash()
            .map_err(|error| CliFailure::from_vaultc(EXIT_USAGE, error))?;
        let embedded_hash = approved
            .plan
            .policy
            .semantic_hash()
            .map_err(|error| CliFailure::from_vaultc(EXIT_OUTPUT, error))?;
        if supplied_hash != embedded_hash {
            return Err(CliFailure::new(
                EXIT_OUTPUT,
                "supplied policy does not match the policy sealed into the approved plan",
            ));
        }
    }
    let compiler = build_compiler(approved.plan.policy.clone(), None, EXIT_OUTPUT)?;
    let approved = revalidate_approved_plan(&compiler, &approved)?;
    let options = CompileOptions {
        create_pack: pack.map(Path::to_path_buf),
    };
    let artifact = compiler
        .compile_with_options(&approved, output, &options)
        .map_err(|error| CliFailure::from_vaultc(EXIT_OUTPUT, error))?;
    match format {
        OutputFormat::Json => print_json(&serde_json::json!({
            "artifact": artifact,
            "vaultpack": pack.map(Path::to_path_buf),
        })),
        OutputFormat::Human => {
            println!("compiled artifact {}", artifact.artifact_id);
            println!("plan: {}", artifact.plan_id);
            println!("path: {}", artifact.path.display());
            println!("files: {}", artifact.files.len());
            if let Some(pack) = pack {
                println!("vaultpack: {}", pack.display());
            }
            Ok(())
        }
    }
}

fn revalidate_approved_plan(
    compiler: &VaultCompiler,
    approved: &ApprovedPlan,
) -> CliResult<ApprovedPlan> {
    vaultc::approval::validate_approved_plan(approved, compiler.policy())
        .map_err(|error| CliFailure::from_vaultc(EXIT_OUTPUT, error))?;
    Ok(approved.clone())
}

fn load_augmentation(
    path: &Path,
    plan: &DraftPlan,
    compiler: &VaultCompiler,
) -> CliResult<ValidatedProposals> {
    let bytes = read_control_bytes(path, CONTROL_FILE_LIMIT, EXIT_PROVIDER)?;
    let recording = RecordedAugmentation::from_canonical_jsonl(&bytes)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    compiler
        .replay_augmentation(plan, &recording)
        .map(RecordedAugmentation::into_validated)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))
}

fn validate_approval_decisions(
    plan: &DraftPlan,
    validated: &ValidatedProposals,
    decisions: &[vaultc::ApprovalDecision],
) -> CliResult<()> {
    let mut proposal_by_id = BTreeMap::new();
    for validation in &validated.validations {
        if proposal_by_id
            .insert(validation.proposal.proposal_id.as_str(), validation)
            .is_some()
        {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                "augmentation contains duplicate proposal IDs",
            ));
        }
    }
    let mut seen = BTreeSet::new();
    for decision in decisions {
        if !seen.insert(decision.proposal_id.as_str()) {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                format!("duplicate decision for proposal `{}`", decision.proposal_id),
            ));
        }
        if decision.plan_id != plan.plan_id.to_string() {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                format!(
                    "proposal `{}` decision belongs to another plan",
                    decision.proposal_id
                ),
            ));
        }
        let Some(validation) = proposal_by_id.get(decision.proposal_id.as_str()) else {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                format!(
                    "decision references unknown proposal `{}`",
                    decision.proposal_id
                ),
            ));
        };
        if !validation.valid {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                format!(
                    "decision references invalid proposal `{}`",
                    decision.proposal_id
                ),
            ));
        }
        if decision.proposal_content_hash != validation.content_hash {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                format!(
                    "proposal `{}` decision has a stale content hash",
                    decision.proposal_id
                ),
            ));
        }
        if decision.approver.trim().is_empty() || decision.policy_version.trim().is_empty() {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                format!(
                    "proposal `{}` decision lacks approver or policy version",
                    decision.proposal_id
                ),
            ));
        }
    }
    Ok(())
}

fn parse_sources(values: &[String]) -> CliResult<Vec<SourceSpec>> {
    let mut sources = Vec::with_capacity(values.len());
    let mut ids = BTreeSet::new();
    for value in values {
        let (explicit_id, path_text) = value
            .split_once('=')
            .map_or((None, value.as_str()), |(id, path)| (Some(id), path));
        if path_text.is_empty() {
            return Err(CliFailure::new(EXIT_USAGE, "source path cannot be empty"));
        }
        let path = PathBuf::from(path_text);
        let id_text = explicit_id.map_or_else(|| derive_source_id(&path), ToOwned::to_owned);
        let source_id =
            SourceId::new(id_text).map_err(|error| CliFailure::from_vaultc(EXIT_USAGE, error))?;
        if !ids.insert(source_id.clone()) {
            return Err(CliFailure::new(
                EXIT_USAGE,
                format!("duplicate source ID `{source_id}`"),
            ));
        }
        let metadata = fs::metadata(&path)
            .map_err(|error| CliFailure::new(EXIT_INPUT, format!("{}: {error}", path.display())))?;
        let source = if metadata.is_dir() {
            SourceSpec::directory(source_id.as_str(), &path)
        } else if metadata.is_file() {
            SourceSpec::archive(source_id.as_str(), &path)
        } else {
            return Err(CliFailure::new(
                EXIT_INPUT,
                format!(
                    "source `{}` is not a regular file or directory",
                    path.display()
                ),
            ));
        }
        .map_err(|error| CliFailure::from_vaultc(EXIT_INPUT, error))?;
        sources.push(source);
    }
    Ok(sources)
}

fn derive_source_id(path: &Path) -> String {
    let mut name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source")
        .to_owned();
    for extension in [".tar.zst", ".tzst", ".zip"] {
        if name.to_ascii_lowercase().ends_with(extension) {
            name.truncate(name.len() - extension.len());
            break;
        }
    }
    let mut sanitized = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            sanitized.push(character);
        } else {
            sanitized.push('-');
        }
    }
    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "source".into()
    } else {
        sanitized.into()
    }
}

fn load_policy(path: Option<&Path>) -> CliResult<CompilerPolicy> {
    path.map_or_else(
        || Ok(CompilerPolicy::default()),
        |path| {
            CompilerPolicy::from_file(path)
                .map_err(|error| CliFailure::from_vaultc(EXIT_USAGE, error))
        },
    )
}

fn build_compiler(
    policy: CompilerPolicy,
    workspace: Option<&Path>,
    exit_code: u8,
) -> CliResult<VaultCompiler> {
    let mut builder = VaultCompiler::builder().policy(policy);
    if let Some(workspace) = workspace {
        builder = builder.workspace(workspace);
    }
    builder
        .build()
        .map_err(|error| CliFailure::from_vaultc(exit_code, error))
}

fn read_json<T: DeserializeOwned>(path: &Path, limit: u64, code: u8) -> CliResult<T> {
    let file = open_bounded(path, limit, code)?;
    let mut reader = file.take(limit.saturating_add(1));
    let decoded = serde_json::from_reader(&mut reader);
    if reader.limit() == 0 {
        return Err(control_file_limit_failure(path, limit, code));
    }
    decoded.map_err(|error| {
        CliFailure::new(
            code,
            format!("invalid JSON in `{}`: {error}", path.display()),
        )
    })
}

fn read_control_bytes(path: &Path, limit: u64, code: u8) -> CliResult<Vec<u8>> {
    let file = open_bounded(path, limit, code)?;
    let mut reader = file.take(limit.saturating_add(1));
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| CliFailure::new(code, format!("{}: {error}", path.display())))?;
    if bytes.len() as u64 > limit {
        return Err(control_file_limit_failure(path, limit, code));
    }
    Ok(bytes)
}

fn open_bounded(path: &Path, limit: u64, code: u8) -> CliResult<File> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| CliFailure::new(code, format!("{}: {error}", path.display())))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CliFailure::new(
            code,
            format!("`{}` must be a regular non-symlink file", path.display()),
        ));
    }
    if metadata.len() > limit {
        return Err(control_file_limit_failure(path, limit, code));
    }
    let file = File::open(path)
        .map_err(|error| CliFailure::new(code, format!("{}: {error}", path.display())))?;
    let opened = file
        .metadata()
        .map_err(|error| CliFailure::new(code, format!("{}: {error}", path.display())))?;
    if !opened.is_file() || opened.len() > limit {
        return Err(control_file_limit_failure(path, limit, code));
    }
    Ok(file)
}

fn control_file_limit_failure(path: &Path, limit: u64, code: u8) -> CliFailure {
    CliFailure::new(
        code,
        format!(
            "`{}` exceeds the {limit}-byte control-file limit",
            path.display()
        ),
    )
}

fn write_canonical_json<T: Serialize>(path: &Path, value: &T) -> CliResult<()> {
    let bytes = vaultc::canonical::to_canonical_json_pretty(value)
        .map_err(|error| CliFailure::from_vaultc(EXIT_INTERNAL, error))?;
    write_atomic(path, &bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> CliResult<()> {
    if path.exists() {
        return Err(CliFailure::new(
            EXIT_OUTPUT,
            format!("destination already exists: {}", path.display()),
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| CliFailure::new(EXIT_OUTPUT, format!("{}: {error}", parent.display())))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| CliFailure::new(EXIT_OUTPUT, format!("{}: {error}", parent.display())))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| CliFailure::new(EXIT_OUTPUT, format!("{}: {error}", path.display())))?;
    temporary.persist_noclobber(path).map_err(|error| {
        CliFailure::new(EXIT_OUTPUT, format!("{}: {}", path.display(), error.error))
    })?;
    sync_parent(parent)?;
    Ok(())
}

fn sync_parent(parent: &Path) -> CliResult<()> {
    #[cfg(unix)]
    {
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                CliFailure::new(EXIT_OUTPUT, format!("{}: {error}", parent.display()))
            })?;
    }
    Ok(())
}

fn print_inspection(inspection: &Inspection, format: OutputFormat) -> CliResult<()> {
    match format {
        OutputFormat::Json => print_json(inspection),
        OutputFormat::Human => {
            let files: usize = inspection
                .snapshots
                .iter()
                .map(|snapshot| snapshot.files.len())
                .sum();
            println!("inspection: {}", inspection.inspection_hash);
            println!("snapshots: {}", inspection.snapshots.len());
            println!("files: {files}");
            println!("documents: {}", inspection.workspace.documents.len());
            println!("canvases: {}", inspection.workspace.canvases.len());
            println!("assets: {}", inspection.workspace.assets.len());
            println!("bases: {}", inspection.workspace.bases.len());
            println!("diagnostics: {}", inspection.diagnostics.len());
            Ok(())
        }
    }
}

fn print_plan(plan: &DraftPlan, out: &Path, format: OutputFormat) -> CliResult<()> {
    match format {
        OutputFormat::Json => print_json(&serde_json::json!({
            "plan_id": plan.plan_id,
            "projection_hash": plan.projection_hash,
            "operations": plan.operations.len(),
            "conflicts": plan.conflicts.len(),
            "required_unresolved_conflicts": plan.unresolved_required_conflicts().count(),
            "out": out,
        })),
        OutputFormat::Human => {
            println!("plan: {}", plan.plan_id);
            println!("operations: {}", plan.operations.len());
            println!("conflicts: {}", plan.conflicts.len());
            println!(
                "required unresolved conflicts: {}",
                plan.unresolved_required_conflicts().count()
            );
            println!("path: {}", out.display());
            Ok(())
        }
    }
}

fn print_verification(report: &vaultc::VerificationReport, format: OutputFormat) -> CliResult<()> {
    match format {
        OutputFormat::Json => print_json(report),
        OutputFormat::Human => {
            println!("valid: {}", report.valid);
            println!("artifact: {}", report.artifact_id);
            println!("plan: {}", report.plan_id);
            println!("checked files: {}", report.checked_files);
            Ok(())
        }
    }
}

fn print_provenance(
    page: &vaultc::provenance::ProvenancePage,
    format: OutputFormat,
) -> CliResult<()> {
    match format {
        OutputFormat::Json => print_json(page),
        OutputFormat::Human => {
            println!("schema version: {}", page.schema_version);
            println!("graph: {}", page.graph_hash);
            match &page.subject {
                vaultc::provenance::ProvenanceSubject::ArtifactPath { path } => {
                    println!("output: {path}");
                }
                vaultc::provenance::ProvenanceSubject::Package => {
                    println!("subject: package");
                }
            }
            println!("records: {}", page.records.len());
            for (index, record) in page.records.iter().enumerate() {
                let record = human_safe_json(record)?;
                let number = index + 1;
                println!("record {number}: {record}");
            }
            println!("complete: {}", page.complete);
            println!(
                "next cursor: {}",
                page.next_cursor.as_deref().unwrap_or("none")
            );
            Ok(())
        }
    }
}

fn human_safe_json<T: Serialize>(value: &T) -> CliResult<String> {
    serde_json::to_string(value)
        .map(|encoded| sanitize_terminal(&encoded))
        .map_err(|error| CliFailure::from_error(EXIT_INTERNAL, error))
}

fn print_json<T: Serialize>(value: &T) -> CliResult<()> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)
        .and_then(|()| lock.write_all(b"\n").map_err(serde_json::Error::io))
        .map_err(|error| CliFailure::from_error(EXIT_INTERNAL, error))
}

fn sanitize_terminal(message: &str) -> String {
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
        .take(4096)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_is_derived_without_absolute_path_material() {
        assert_eq!(
            derive_source_id(Path::new("/private/example/Team Vault.zip")),
            "Team-Vault"
        );
        assert_eq!(
            derive_source_id(Path::new("Knowledge.tar.zst")),
            "Knowledge"
        );
    }

    #[test]
    fn compile_pack_argument_requires_vaultpack_extension() {
        let cli = Cli::try_parse_from([
            "vaultc",
            "compile",
            "approved.json",
            "--output",
            "compiled",
            "--pack",
            "release.VAULTPACK",
        ])
        .expect("ASCII-case-insensitive VaultPack extension");
        match cli.command {
            CommandKind::Compile { pack, .. } => {
                assert_eq!(pack, Some(PathBuf::from("release.VAULTPACK")));
            }
            _ => panic!("expected compile command"),
        }

        let error = Cli::try_parse_from([
            "vaultc",
            "compile",
            "approved.json",
            "--output",
            "compiled",
            "--pack",
            "release.tar.zst",
        ])
        .expect_err("non-VaultPack extension must be a usage error");
        assert_eq!(error.exit_code(), i32::from(EXIT_USAGE));
    }

    #[test]
    fn terminal_diagnostics_strip_control_and_bidi_overrides() {
        assert_eq!(
            sanitize_terminal("bad\n\u{1b}[31m\u{202e}name"),
            "bad  [31m name"
        );
    }

    #[test]
    fn human_provenance_record_sanitizes_untrusted_original_path() {
        let record = serde_json::json!({
            "kind": {
                "type": "source",
                "value": {
                    "type": "vault_file",
                    "value": {
                        "original_source_path": "notes/safe\u{202e}txt\u{1b}.md",
                        "source_path": "notes/safe.txt.md",
                        "path_encoding": "utf8"
                    }
                }
            }
        });

        let rendered = human_safe_json(&record).expect("render untrusted provenance record");
        assert!(!rendered.contains('\u{202e}'));
        assert!(!rendered.contains('\u{1b}'));
        assert!(rendered.contains("notes/safe txt\\u001b.md"));
    }

    #[test]
    fn replay_arguments_have_no_provider_or_remote_consent_surface() {
        let cli = Cli::try_parse_from([
            "vaultc",
            "replay",
            "plan.json",
            "--augmentation",
            "augmentation.jsonl",
            "--out",
            "replayed.jsonl",
        ])
        .expect("valid replay arguments");
        match cli.command {
            CommandKind::Replay {
                plan,
                augmentation,
                out,
            } => {
                assert_eq!(plan, PathBuf::from("plan.json"));
                assert_eq!(augmentation, PathBuf::from("augmentation.jsonl"));
                assert_eq!(out, PathBuf::from("replayed.jsonl"));
            }
            _ => panic!("expected replay command"),
        }

        let error = Cli::try_parse_from([
            "vaultc",
            "replay",
            "plan.json",
            "--augmentation",
            "augmentation.jsonl",
            "--out",
            "replayed.jsonl",
            "--allow-remote-provider",
        ])
        .expect_err("replay must not accept durable remote consent");
        assert_eq!(error.exit_code(), i32::from(EXIT_USAGE));
    }

    #[test]
    fn explain_arguments_require_one_subject_and_a_bounded_limit() {
        let cli = Cli::try_parse_from([
            "vaultc",
            "explain",
            "artifact.vaultpack",
            "--package",
            "--limit",
            "4096",
        ])
        .expect("valid package explanation arguments");
        match cli.command {
            CommandKind::Explain {
                output_path,
                package,
                limit,
                ..
            } => {
                assert!(output_path.is_none());
                assert!(package);
                assert_eq!(limit, 4096);
            }
            _ => panic!("expected explain command"),
        }

        for arguments in [
            vec!["vaultc", "explain", "artifact"],
            vec![
                "vaultc",
                "explain",
                "artifact",
                "knowledge/Index.md",
                "--package",
            ],
            vec![
                "vaultc",
                "explain",
                "artifact",
                "knowledge/Index.md",
                "--limit",
                "0",
            ],
            vec![
                "vaultc",
                "explain",
                "artifact",
                "knowledge/Index.md",
                "--limit",
                "4097",
            ],
        ] {
            let error = Cli::try_parse_from(arguments).expect_err("invalid explain arguments");
            assert_eq!(error.exit_code(), i32::from(EXIT_USAGE));
        }
    }

    #[test]
    fn plan_error_families_are_classified_without_hiding_invariants() {
        let input_errors = [
            VaultcError::UnsafePath {
                path: "unsafe".into(),
                reason: "escapes source root".into(),
            },
            VaultcError::UnsupportedSource(PathBuf::from("unsupported")),
            VaultcError::ResourceLimit("source is too large".into()),
            VaultcError::MalformedInput {
                path: "bad.zip".into(),
                reason: "invalid archive".into(),
            },
            VaultcError::IdentityMismatch("changed.md".into()),
            VaultcError::Io {
                path: PathBuf::from("missing.md"),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
            },
        ];
        for error in input_errors {
            assert_eq!(plan_failure(&error).code, EXIT_INPUT);
        }

        assert_eq!(
            plan_failure(&VaultcError::InvalidConfig("bad policy".into())).code,
            EXIT_USAGE
        );
        assert_eq!(
            plan_failure(&VaultcError::PlanStale("decision boundary".into())).code,
            EXIT_DECISION
        );
        assert_eq!(
            plan_failure(&VaultcError::Internal("broken invariant".into())).code,
            EXIT_INTERNAL
        );
        assert_eq!(
            plan_failure(&VaultcError::Provider("impossible during planning".into())).code,
            EXIT_INTERNAL
        );
    }

    #[test]
    fn compile_error_families_preserve_nested_internal_invariants() {
        let wrapped_internal = VaultcError::PackPublicationAfterCompile {
            compiled_vault: PathBuf::from("compiled"),
            pack: PathBuf::from("compiled.vaultpack"),
            source: Box::new(VaultcError::Internal("broken pack invariant".into())),
        };
        assert_eq!(
            CliFailure::from_vaultc(EXIT_OUTPUT, wrapped_internal).code,
            EXIT_INTERNAL
        );

        let wrapped_runtime = VaultcError::PackPublicationAfterCompile {
            compiled_vault: PathBuf::from("compiled"),
            pack: PathBuf::from("compiled.vaultpack"),
            source: Box::new(VaultcError::Io {
                path: PathBuf::from("compiled.vaultpack"),
                source: std::io::Error::other("runtime pack failure"),
            }),
        };
        assert_eq!(
            CliFailure::from_vaultc(EXIT_OUTPUT, wrapped_runtime).code,
            EXIT_OUTPUT
        );

        let durability = VaultcError::PublishedButDurabilityUncertain {
            path: PathBuf::from("compiled.vaultpack"),
            source: std::io::Error::other("directory sync failed"),
        };
        assert_eq!(
            CliFailure::from_vaultc(EXIT_OUTPUT, durability).code,
            EXIT_OUTPUT
        );
    }
}
