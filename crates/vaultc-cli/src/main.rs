mod command_provider;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use command_provider::{CommandProvider, CommandProviderConfig, ProviderCancellation};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use vaultc::approval::{ApprovalLog, ApprovedPlan, ConflictDecision, ConflictDecisionLog};
use vaultc::plan::{DraftPlan, Inspection};
use vaultc::provider::{ProposalValidation, ValidatedProposals};
use vaultc::{CompilerPolicy, SourceId, SourceSpec, VaultCompiler, VaultcError};
use vaultc_protocol::{
    AugmentationLimits, AugmentationRequest, AugmentationResponse, DocumentProjection,
    KnowledgeProposal, MessageType, ObjectKind, ObjectRef, ProjectedBlock, ProposalKindName,
    ProviderCapabilities, ProviderIdentity, TranscriptDirection, TranscriptRecord,
};

const EXIT_USAGE: u8 = 2;
const EXIT_INPUT: u8 = 3;
const EXIT_DECISION: u8 = 4;
const EXIT_PROVIDER: u8 = 5;
const EXIT_OUTPUT: u8 = 6;
const EXIT_VERIFY: u8 = 7;
const EXIT_INTERNAL: u8 = 70;
const CONTROL_FILE_LIMIT: u64 = 1024 * 1024 * 1024;
const CONTROL_LINE_LIMIT: u64 = 64 * 1024 * 1024;
const AUGMENTATION_SCHEMA_VERSION: u32 = 1;
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
        #[arg(long, value_name = "FILE")]
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
        #[arg(value_name = "OUTPUT_PATH")]
        output_path: String,
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
        let code = if matches!(&error, VaultcError::Internal(_)) {
            EXIT_INTERNAL
        } else {
            default_code
        };
        Self::new(code, error.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AugmentationRecord {
    Header {
        schema_version: u32,
        plan_id: String,
        projection_hash: String,
        provider: ProviderIdentity,
    },
    Transcript {
        record: TranscriptRecord,
    },
    Proposal {
        validation: Box<ProposalValidation>,
    },
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
                .map_err(|error| CliFailure::from_vaultc(EXIT_DECISION, error))?;
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
            format,
        } => explain_command(&artifact, &output_path, format),
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

fn explain_command(artifact: &Path, output_path: &str, format: OutputFormat) -> CliResult<()> {
    let compiler = build_compiler(CompilerPolicy::default(), None, EXIT_VERIFY)?;
    let explanation = compiler
        .explain_provenance(artifact, output_path)
        .map_err(|error| CliFailure::from_vaultc(EXIT_VERIFY, error))?;
    print_provenance(&explanation, format)
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
    let request = build_augmentation_request(&plan, requested_documents, all_documents)?;
    let mut runtime_policy = plan.policy.clone();
    runtime_policy.augmentation.allow_remote_providers =
        runtime_policy.augmentation.allow_remote_providers && allow_remote_provider;
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
        .augment(&request, &runtime_policy)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    if cancellation.is_cancelled() {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "provider operation was cancelled",
        ));
    }
    let compiler = build_compiler(plan.policy.clone(), None, EXIT_PROVIDER)?;
    let mut validated = compiler
        .validate_proposals(&plan, run.response.proposals)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    validated.transcript = run.transcript;

    let mut records =
        Vec::with_capacity(1 + validated.transcript.len() + validated.validations.len());
    records.push(AugmentationRecord::Header {
        schema_version: AUGMENTATION_SCHEMA_VERSION,
        plan_id: plan.plan_id.to_string(),
        projection_hash: plan.projection_hash.hex(),
        provider: run.capabilities.provider,
    });
    records.extend(
        validated
            .transcript
            .iter()
            .cloned()
            .map(|record| AugmentationRecord::Transcript { record }),
    );
    records.extend(validated.validations.iter().cloned().map(|validation| {
        AugmentationRecord::Proposal {
            validation: Box::new(validation),
        }
    }));
    write_canonical_jsonl(out, &records)?;
    let rejected = validated
        .validations
        .iter()
        .filter(|validation| !validation.valid)
        .count();
    println!(
        "wrote {} proposal validation(s) to {} ({} rejected)",
        validated.validations.len(),
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
    let plan: DraftPlan = read_json(plan_path, CONTROL_FILE_LIMIT, EXIT_PROVIDER)?;
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
    reject_existing_entry(output)?;
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
    if let Some(pack_path) = pack {
        validate_pack_destination(output, pack_path)?;
    }
    let compiler = build_compiler(approved.plan.policy.clone(), None, EXIT_OUTPUT)?;
    let approved = revalidate_approved_plan(&compiler, &approved)?;
    let artifact = compiler
        .compile(&approved, output)
        .map_err(|error| CliFailure::from_vaultc(EXIT_OUTPUT, error))?;
    if let Some(pack_path) = pack {
        create_pack_atomic(output, pack_path, approved.plan.policy.output.zstd_level)?;
    }
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

fn build_augmentation_request(
    plan: &DraftPlan,
    requested: &[String],
    all_documents: bool,
) -> CliResult<AugmentationRequest> {
    let requested_count = requested.len();
    let requested: BTreeSet<_> = requested.iter().cloned().collect();
    if requested.len() != requested_count {
        return Err(CliFailure::new(EXIT_USAGE, "duplicate --document-id value"));
    }
    let known: BTreeSet<_> = plan
        .workspace
        .documents
        .keys()
        .map(ToString::to_string)
        .collect();
    for id in &requested {
        if !known.contains(id) {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                format!("document `{id}` is not present in the sealed plan"),
            ));
        }
    }
    let documents = plan
        .workspace
        .documents
        .values()
        .filter(|document| all_documents || requested.contains(&document.document_id.to_string()))
        .map(|document| DocumentProjection {
            snapshot_id: document.source_file.snapshot_id.to_string(),
            document: ObjectRef {
                kind: ObjectKind::Document,
                id: document.document_id.to_string(),
                content_hash: document.body_hash.hex(),
            },
            logical_path: document.source_file.logical_path.clone(),
            title: document.title.clone(),
            selected_blocks: document
                .blocks
                .iter()
                .map(|block| ProjectedBlock {
                    block: ObjectRef {
                        kind: ObjectKind::Block,
                        id: block.block_id.to_string(),
                        content_hash: block.content_hash.hex(),
                    },
                    text: block.comparison_text.clone(),
                })
                .collect(),
        })
        .collect();
    Ok(AugmentationRequest {
        plan_id: plan.plan_id.to_string(),
        projection_hash: plan.projection_hash.hex(),
        allowed_proposal_kinds: vec![
            ProposalKindName::CreateGeneratedNote,
            ProposalKindName::ExplainConflict,
        ],
        documents,
        limits: AugmentationLimits {
            max_proposals: plan.policy.augmentation.max_proposals,
            max_generated_bytes: plan.policy.augmentation.max_generated_bytes,
        },
    })
}

fn load_augmentation(
    path: &Path,
    plan: &DraftPlan,
    compiler: &VaultCompiler,
) -> CliResult<ValidatedProposals> {
    let max_records = usize::try_from(plan.policy.augmentation.max_proposals)
        .unwrap_or(usize::MAX)
        .saturating_add(5);
    let records: Vec<AugmentationRecord> = read_jsonl(
        path,
        CONTROL_FILE_LIMIT,
        CONTROL_LINE_LIMIT,
        max_records,
        EXIT_PROVIDER,
    )?;
    let mut header = None;
    let mut transcript = Vec::new();
    let mut recorded_validations = Vec::new();
    for (index, record) in records.into_iter().enumerate() {
        match record {
            AugmentationRecord::Header {
                schema_version,
                plan_id,
                projection_hash,
                provider,
            } => {
                if index != 0 || header.is_some() {
                    return Err(CliFailure::new(
                        EXIT_PROVIDER,
                        "augmentation header must be the first and only header record",
                    ));
                }
                if schema_version != AUGMENTATION_SCHEMA_VERSION {
                    return Err(CliFailure::new(
                        EXIT_PROVIDER,
                        format!("unsupported augmentation schema version {schema_version}"),
                    ));
                }
                header = Some((plan_id, projection_hash, provider));
            }
            AugmentationRecord::Transcript { record } => transcript.push(record),
            AugmentationRecord::Proposal { validation } => {
                recorded_validations.push(*validation);
            }
        }
    }
    let Some((plan_id, projection_hash, provider)) = header else {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation file has no header",
        ));
    };
    if plan_id != plan.plan_id.to_string() || projection_hash != plan.projection_hash.hex() {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation output is stale for this plan",
        ));
    }
    validate_transcript(&transcript, &provider, &recorded_validations, plan)?;
    let proposals: Vec<_> = recorded_validations
        .iter()
        .map(|validation| validation.proposal.clone())
        .collect();
    let mut revalidated = compiler
        .validate_proposals(plan, proposals)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    if revalidated.validations != recorded_validations {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation validation records do not match deterministic revalidation",
        ));
    }
    revalidated.transcript = transcript;
    Ok(revalidated)
}

fn validate_transcript(
    transcript: &[TranscriptRecord],
    provider: &ProviderIdentity,
    validations: &[ProposalValidation],
    plan: &DraftPlan,
) -> CliResult<()> {
    validate_transcript_shape_and_hashes(transcript, plan)?;
    validate_transcript_payloads(transcript, provider, validations, plan)
}

fn validate_transcript_shape_and_hashes(
    transcript: &[TranscriptRecord],
    plan: &DraftPlan,
) -> CliResult<()> {
    if transcript.len() != 4 {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "V1 augmentation transcript must contain exactly four records",
        ));
    }
    for (index, record) in transcript.iter().enumerate() {
        if record.sequence != index as u64 {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                "augmentation transcript sequence is not contiguous",
            ));
        }
        if index == 2 {
            let hydrated = hydrate_redacted_request(&record.payload, plan)?;
            let hash = vaultc::canonical::canonical_hash(
                "vaultc:provider-transcript-payload:v1\0",
                &hydrated,
            )
            .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?
            .hex();
            if hash != record.canonical_payload_hash {
                return Err(CliFailure::new(
                    EXIT_PROVIDER,
                    "augmentation request transcript hash mismatch",
                ));
            }
        } else {
            let hash = vaultc::canonical::canonical_hash(
                "vaultc:provider-transcript-payload:v1\0",
                &record.payload,
            )
            .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?
            .hex();
            if hash != record.canonical_payload_hash {
                return Err(CliFailure::new(
                    EXIT_PROVIDER,
                    "augmentation transcript payload hash mismatch",
                ));
            }
        }
    }
    let expected_types = [
        MessageType::CapabilitiesRequest,
        MessageType::CapabilitiesResponse,
        MessageType::AugmentationRequest,
        MessageType::AugmentationResponse,
    ];
    if transcript
        .iter()
        .map(|record| record.message_type)
        .ne(expected_types)
    {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation transcript message ordering is invalid",
        ));
    }
    let expected_directions = [
        TranscriptDirection::Request,
        TranscriptDirection::Response,
        TranscriptDirection::Request,
        TranscriptDirection::Response,
    ];
    if transcript
        .iter()
        .map(|record| record.direction)
        .ne(expected_directions)
        || transcript[0].request_id != "capabilities-1"
        || transcript[1].request_id != "capabilities-1"
        || transcript[2].request_id != "augmentation-1"
        || transcript[3].request_id != "augmentation-1"
    {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation transcript direction or request IDs are invalid",
        ));
    }
    Ok(())
}

fn validate_transcript_payloads(
    transcript: &[TranscriptRecord],
    provider: &ProviderIdentity,
    validations: &[ProposalValidation],
    plan: &DraftPlan,
) -> CliResult<()> {
    let capabilities: ProviderCapabilities = serde_json::from_value(transcript[1].payload.clone())
        .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?;
    vaultc::provider::validate_capabilities(&capabilities, &plan.policy)
        .map_err(|error| CliFailure::from_vaultc(EXIT_PROVIDER, error))?;
    if !capabilities.structured_output {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation transcript provider did not declare structured output",
        ));
    }
    if &capabilities.provider != provider {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation header provider differs from negotiated capabilities",
        ));
    }
    let response: AugmentationResponse = serde_json::from_value(transcript[3].payload.clone())
        .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?;
    let response_bytes = serde_json::to_vec(&response)
        .map_err(|error| CliFailure::from_error(EXIT_INTERNAL, error))?;
    if response_bytes.len() as u64 > capabilities.max_output_bytes {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation response exceeds the provider's declared output limit",
        ));
    }
    let mut response_proposals = response.proposals;
    response_proposals.sort_by(|left, right| left.proposal_id.cmp(&right.proposal_id));
    let recorded: Vec<KnowledgeProposal> = validations
        .iter()
        .map(|validation| validation.proposal.clone())
        .collect();
    if response_proposals != recorded {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "transcript proposals do not match proposal validation records",
        ));
    }
    if recorded
        .iter()
        .any(|proposal| &proposal.provider != provider)
    {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "proposal provider identity differs from augmentation header",
        ));
    }
    Ok(())
}

fn hydrate_redacted_request(
    payload: &serde_json::Value,
    plan: &DraftPlan,
) -> CliResult<serde_json::Value> {
    let mut hydrated = payload.clone();
    let documents = hydrated
        .get_mut("documents")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            CliFailure::new(
                EXIT_PROVIDER,
                "augmentation request transcript lacks document projections",
            )
        })?;
    for projected_document in documents {
        let document_id = projected_document
            .pointer("/document/id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                CliFailure::new(
                    EXIT_PROVIDER,
                    "augmentation transcript has a malformed document projection",
                )
            })?;
        let Some(document) = plan
            .workspace
            .documents
            .values()
            .find(|document| document.document_id.to_string() == document_id)
        else {
            return Err(CliFailure::new(
                EXIT_PROVIDER,
                "augmentation transcript references an unknown document",
            ));
        };
        validate_projected_snapshot(
            projected_document,
            &document.source_file.snapshot_id.to_string(),
        )?;
        let blocks = projected_document
            .get_mut("selected_blocks")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| {
                CliFailure::new(
                    EXIT_PROVIDER,
                    "augmentation transcript has malformed selected blocks",
                )
            })?;
        for projected_block in blocks {
            let block_id = projected_block
                .pointer("/block/id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    CliFailure::new(
                        EXIT_PROVIDER,
                        "augmentation transcript has a malformed block projection",
                    )
                })?;
            let Some(block) = document
                .blocks
                .iter()
                .find(|block| block.block_id.to_string() == block_id)
            else {
                return Err(CliFailure::new(
                    EXIT_PROVIDER,
                    "augmentation transcript references an unknown document block",
                ));
            };
            let text = projected_block.get_mut("text").ok_or_else(|| {
                CliFailure::new(
                    EXIT_PROVIDER,
                    "augmentation transcript block has no redacted text field",
                )
            })?;
            if text.as_str() != Some("[redacted]") {
                return Err(CliFailure::new(
                    EXIT_PROVIDER,
                    "augmentation transcript contains unredacted source projection text",
                ));
            }
            *text = serde_json::Value::String(block.comparison_text.clone());
        }
    }
    let request: AugmentationRequest = serde_json::from_value(hydrated.clone())
        .map_err(|error| CliFailure::from_error(EXIT_PROVIDER, error))?;
    let selected_ids: Vec<_> = request
        .documents
        .iter()
        .map(|projection| projection.document.id.clone())
        .collect();
    let expected = build_augmentation_request(plan, &selected_ids, false)?;
    if request != expected {
        return Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation request transcript is stale for this plan or policy",
        ));
    }
    Ok(hydrated)
}

fn validate_projected_snapshot(
    projected_document: &serde_json::Value,
    expected_snapshot_id: &str,
) -> CliResult<()> {
    if projected_document
        .get("snapshot_id")
        .and_then(serde_json::Value::as_str)
        == Some(expected_snapshot_id)
    {
        Ok(())
    } else {
        Err(CliFailure::new(
            EXIT_PROVIDER,
            "augmentation transcript document snapshot binding is stale",
        ))
    }
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

fn read_jsonl<T: DeserializeOwned>(
    path: &Path,
    limit: u64,
    line_limit: u64,
    record_limit: usize,
    code: u8,
) -> CliResult<Vec<T>> {
    let file = open_bounded(path, limit, code)?;
    let mut reader = BufReader::new(file);
    let mut records = Vec::new();
    let mut total = 0_u64;
    loop {
        let mut line = Vec::new();
        let mut bounded_line = (&mut reader).take(line_limit.saturating_add(1));
        let read = bounded_line
            .read_until(b'\n', &mut line)
            .map_err(|error| CliFailure::new(code, format!("{}: {error}", path.display())))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > limit {
            return Err(control_file_limit_failure(path, limit, code));
        }
        if line.len() as u64 > line_limit {
            return Err(CliFailure::new(
                code,
                format!(
                    "JSONL record in `{}` exceeds the {line_limit}-byte line limit",
                    path.display()
                ),
            ));
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            return Err(CliFailure::new(
                code,
                format!(
                    "blank JSONL record at {}:{}",
                    path.display(),
                    records.len() + 1
                ),
            ));
        }
        if records.len() >= record_limit {
            return Err(CliFailure::new(
                code,
                format!(
                    "`{}` exceeds the {record_limit}-record JSONL limit",
                    path.display()
                ),
            ));
        }
        records.push(serde_json::from_slice(&line).map_err(|error| {
            CliFailure::new(
                code,
                format!(
                    "invalid JSONL at {}:{}: {error}",
                    path.display(),
                    records.len() + 1
                ),
            )
        })?);
    }
    Ok(records)
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

fn write_canonical_jsonl<T: Serialize>(path: &Path, values: &[T]) -> CliResult<()> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend(
            vaultc::canonical::to_canonical_json(value)
                .map_err(|error| CliFailure::from_vaultc(EXIT_INTERNAL, error))?,
        );
        bytes.push(b'\n');
    }
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

fn validate_pack_destination(output: &Path, pack: &Path) -> CliResult<()> {
    if !pack
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("vaultpack"))
    {
        return Err(CliFailure::new(
            EXIT_USAGE,
            "VaultPack destination must use the .vaultpack extension",
        ));
    }
    let output = resolve_existing_ancestors(output)?;
    let pack = resolve_existing_ancestors(pack)?;
    if pack == output || pack.starts_with(&output) {
        return Err(CliFailure::new(
            EXIT_OUTPUT,
            "VaultPack destination cannot be inside the Compiled Vault",
        ));
    }
    reject_existing_entry(&pack)?;
    Ok(())
}

fn reject_existing_entry(path: &Path) -> CliResult<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(CliFailure::new(
            EXIT_OUTPUT,
            format!("destination already exists: {}", path.display()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CliFailure::new(
            EXIT_OUTPUT,
            format!("cannot inspect destination `{}`: {error}", path.display()),
        )),
    }
}

fn create_pack_atomic(compiled_vault: &Path, destination: &Path, zstd_level: i32) -> CliResult<()> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| CliFailure::new(EXIT_OUTPUT, format!("{}: {error}", parent.display())))?;
    let staging = tempfile::Builder::new()
        .prefix(".vaultc-pack-")
        .tempdir_in(parent)
        .map_err(|error| CliFailure::new(EXIT_OUTPUT, format!("{}: {error}", parent.display())))?;
    let staged_pack = staging.path().join("artifact.vaultpack");
    vaultc::pack::create_pack(compiled_vault, &staged_pack, zstd_level)
        .map_err(|error| CliFailure::from_vaultc(EXIT_OUTPUT, error))?;
    fs::hard_link(&staged_pack, destination).map_err(|error| {
        CliFailure::new(
            EXIT_OUTPUT,
            format!(
                "failed to publish `{}` without overwrite: {error}",
                destination.display()
            ),
        )
    })?;
    sync_parent(parent)?;
    Ok(())
}

fn absolute_lexical(path: &Path) -> CliResult<PathBuf> {
    let combined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in combined.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    Ok(normalized)
}

fn resolve_existing_ancestors(path: &Path) -> CliResult<PathBuf> {
    let mut ancestor = absolute_lexical(path)?;
    let mut suffix = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = ancestor.file_name().ok_or_else(|| {
                    CliFailure::new(
                        EXIT_OUTPUT,
                        format!("cannot resolve destination `{}`", path.display()),
                    )
                })?;
                suffix.push(name.to_os_string());
                if !ancestor.pop() {
                    return Err(CliFailure::new(
                        EXIT_OUTPUT,
                        format!("cannot resolve destination `{}`", path.display()),
                    ));
                }
            }
            Err(error) => {
                return Err(CliFailure::new(
                    EXIT_OUTPUT,
                    format!("cannot inspect destination `{}`: {error}", path.display()),
                ));
            }
        }
    }
    let mut resolved = fs::canonicalize(&ancestor).map_err(|error| {
        CliFailure::new(
            EXIT_OUTPUT,
            format!(
                "cannot resolve destination ancestor `{}`: {error}",
                ancestor.display()
            ),
        )
    })?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
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
    explanation: &vaultc::provenance::ProvenanceExplanation,
    format: OutputFormat,
) -> CliResult<()> {
    match format {
        OutputFormat::Json => print_json(explanation),
        OutputFormat::Human => {
            println!("output: {}", explanation.output_path);
            for record in &explanation.records {
                match record {
                    vaultc::provenance::ProvenanceRecord::Output {
                        output_hash,
                        operation_id,
                        source_snapshot_ids,
                        source_document_ids,
                        proposal_id,
                        evidence,
                        ..
                    } => {
                        println!("hash: {output_hash}");
                        println!("operation: {operation_id}");
                        println!("source snapshots: {}", source_snapshot_ids.join(", "));
                        println!("source documents: {}", source_document_ids.join(", "));
                        if let Some(proposal_id) = proposal_id {
                            println!("proposal: {proposal_id}");
                        }
                        println!("evidence references: {}", evidence.len());
                    }
                }
            }
            Ok(())
        }
    }
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
    fn pack_must_be_outside_output_tree() {
        let output = Path::new("target/test-output/CompiledVault");
        assert!(validate_pack_destination(output, &output.join("bad.vaultpack")).is_err());
        assert!(
            validate_pack_destination(output, Path::new("target/test-output/good.vaultpack"))
                .is_ok()
        );
        assert!(
            validate_pack_destination(output, Path::new("target/test-output/bad.tar.zst")).is_err()
        );
    }

    #[test]
    fn terminal_diagnostics_strip_control_and_bidi_overrides() {
        assert_eq!(
            sanitize_terminal("bad\n\u{1b}[31m\u{202e}name"),
            "bad  [31m name"
        );
    }

    #[test]
    fn control_jsonl_is_line_and_record_bounded() {
        let mut file = tempfile::NamedTempFile::new().expect("temporary JSONL");
        file.write_all(b"{}\n{}\n").expect("write JSONL records");
        let path = file.path();

        let records: Vec<serde_json::Value> =
            read_jsonl(path, 64, 8, 2, EXIT_PROVIDER).expect("bounded JSONL");
        assert_eq!(records.len(), 2);
        assert!(read_jsonl::<serde_json::Value>(path, 64, 1, 2, EXIT_PROVIDER).is_err());
        assert!(read_jsonl::<serde_json::Value>(path, 64, 8, 1, EXIT_PROVIDER).is_err());
    }
}
