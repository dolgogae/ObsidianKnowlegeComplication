mod commands;
mod tui;

use std::io::{IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory as _, Parser, Subcommand, ValueEnum};
use serde::Serialize;

const EXIT_USAGE: u8 = 2;
const EXIT_INPUT: u8 = 3;
const EXIT_DECISION: u8 = 4;
const EXIT_PROVIDER: u8 = 5;
const EXIT_OUTPUT: u8 = 6;
const EXIT_VERIFY: u8 = 7;
const EXIT_INTERNAL: u8 = 70;

#[derive(Debug, Parser)]
#[command(
    name = "okc",
    version,
    about = "Compile immutable Obsidian Vault snapshots into an auditable Vault"
)]
struct Cli {
    /// Long-lived `.okc-project` used by the TUI and application services.
    #[arg(long, global = true, value_name = "PATH")]
    project: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<CommandKind>,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// Create and configure a project.
    Project {
        #[command(subcommand)]
        command: commands::ProjectCommand,
    },

    /// Manage global AI provider profiles with environment or OS-keychain references.
    Provider {
        #[command(subcommand)]
        command: commands::ProviderCommand,
    },

    /// Start or resume the integration journal.
    Integrate {
        /// Permit this run to disclose non-sensitive content to a remote profile.
        #[arg(long)]
        allow_remote_provider: bool,
        /// Confirm a non-interactive remote disclosure.
        #[arg(long)]
        yes: bool,
    },

    /// Inspect integration progress.
    Integration {
        #[command(subcommand)]
        command: commands::IntegrationCommand,
    },

    /// Review and approve taxonomy and cluster proposals.
    Review {
        #[command(subcommand)]
        command: commands::ReviewCommand,
    },

    /// Launch the interactive terminal interface.
    Tui,

    /// Materialize a fully approved integration plan into a new directory.
    Compile {
        /// Compile this approved plan instead of the selected project's latest plan.
        #[arg(long, value_name = "FILE")]
        integration_plan: Option<PathBuf>,
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },

    /// Independently verify a current Compiled Vault directory.
    Verify {
        #[arg(value_name = "DIRECTORY")]
        artifact: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },

    /// Explain one output's provenance without contacting a provider.
    Explain {
        #[arg(value_name = "DIRECTORY")]
        artifact: PathBuf,
        #[arg(value_name = "OUTPUT_PATH")]
        output_path: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },

    /// Diagnose the local terminal, project, and update-installation context.
    Doctor,

    /// Install a receipt-aware update after explicit confirmation.
    Update {
        #[arg(default_value = "stable", value_name = "stable|latest|VERSION")]
        channel_or_version: String,
        /// Confirm installation non-interactively.
        #[arg(long)]
        yes: bool,
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
}

fn main() -> ExitCode {
    let mut cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(EXIT_USAGE));
        }
    };
    if cli.command.is_none() {
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            let mut command = Cli::command();
            let _ = command.print_help();
            println!();
            return ExitCode::from(EXIT_USAGE);
        }
        cli.command = Some(CommandKind::Tui);
    }
    if cli.project.is_none()
        && cli
            .command
            .as_ref()
            .is_some_and(command_requires_discovered_project)
    {
        match discover_single_project() {
            Ok(path) => cli.project = Some(path),
            Err(error) => {
                eprintln!("okc: {}", sanitize_terminal(&error.message));
                return ExitCode::from(error.code);
            }
        }
    }
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("okc: {}", sanitize_terminal(&error.message));
            ExitCode::from(error.code)
        }
    }
}

fn command_requires_discovered_project(command: &CommandKind) -> bool {
    match command {
        CommandKind::Integrate { .. }
        | CommandKind::Integration { .. }
        | CommandKind::Review { .. }
        | CommandKind::Project {
            command:
                commands::ProjectCommand::Source { .. } | commands::ProjectCommand::AiRoute { .. },
        } => true,
        CommandKind::Compile {
            integration_plan, ..
        } => integration_plan.is_none(),
        _ => false,
    }
}

fn discover_single_project() -> CliResult<PathBuf> {
    let bootstrap = okc_app::WorkspaceBootstrap::from_current_dir()
        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
    let projects = bootstrap
        .discover_projects(None)
        .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
    match projects.as_slice() {
        [project] => Ok(project.path.clone()),
        [] => Err(CliFailure::new(
            EXIT_USAGE,
            "no cwd project found; use --project PATH or start interactive `okc` to create one",
        )),
        _ => Err(CliFailure::new(
            EXIT_USAGE,
            "multiple cwd projects found; select one with --project PATH",
        )),
    }
}

fn run(cli: Cli) -> CliResult<()> {
    let command = cli
        .command
        .ok_or_else(|| CliFailure::new(EXIT_USAGE, "a command is required"))?;
    match command {
        CommandKind::Project { command } => {
            commands::project_command(command, cli.project.as_deref())
        }
        CommandKind::Provider { command } => commands::provider_command(command),
        CommandKind::Integrate {
            allow_remote_provider,
            yes,
        } => commands::integrate_command(cli.project.as_deref(), allow_remote_provider, yes),
        CommandKind::Integration { command } => {
            commands::integration_command(command, cli.project.as_deref())
        }
        CommandKind::Review { command } => {
            commands::review_command(command, cli.project.as_deref())
        }
        CommandKind::Tui => {
            tui::run(cli.project.as_deref()).map_err(|error| CliFailure::new(EXIT_INTERNAL, error))
        }
        CommandKind::Compile {
            integration_plan,
            output,
            format,
        } => {
            if let Some(plan) = integration_plan {
                commands::compile_command(cli.project.as_deref(), &plan, &output, format)
            } else {
                commands::compile_latest_command(
                    cli.project.as_deref().ok_or_else(|| {
                        CliFailure::new(EXIT_USAGE, "compile requires a selected project")
                    })?,
                    &output,
                    format,
                )
            }
        }
        CommandKind::Verify { artifact, format } => verify_command(&artifact, format),
        CommandKind::Explain {
            artifact,
            output_path,
            format,
        } => explain_command(&artifact, &output_path, format),
        CommandKind::Doctor => doctor_command(cli.project.as_deref()),
        CommandKind::Update {
            channel_or_version,
            yes,
        } => update_command(&channel_or_version, yes),
    }
}

fn verify_command(artifact: &Path, format: OutputFormat) -> CliResult<()> {
    let manifest = okc_app::ArtifactService
        .verify(artifact)
        .map_err(|error| CliFailure::from_error(EXIT_VERIFY, error))?;
    match format {
        OutputFormat::Json => print_json(&manifest),
        OutputFormat::Human => {
            println!("valid Schema 3 artifact: true");
            println!("integration plan: {}", manifest.integration_plan_id);
            println!("checked files: {}", manifest.files.len() + 2);
            Ok(())
        }
    }
}

fn explain_command(artifact: &Path, output_path: &str, format: OutputFormat) -> CliResult<()> {
    let record = okc_app::ArtifactService
        .explain(artifact, output_path)
        .map_err(|error| CliFailure::from_error(EXIT_VERIFY, error))?;
    match format {
        OutputFormat::Json => print_json(&record),
        OutputFormat::Human => {
            println!("provenance record: {}", record.record_id);
            println!("kind: {}", record.kind);
            println!("integration plan: {}", record.integration_plan_id);
            println!("evidence references: {}", record.evidence.len());
            Ok(())
        }
    }
}

fn doctor_command(project: Option<&Path>) -> CliResult<()> {
    println!("OKC {}", env!("CARGO_PKG_VERSION"));
    println!("format: okc schema 3");
    println!("stdin TTY: {}", std::io::stdin().is_terminal());
    println!("stdout TTY: {}", std::io::stdout().is_terminal());
    println!(
        "target: {}-{}",
        std::env::consts::ARCH,
        std::env::consts::OS
    );
    if let Some(path) = project {
        let project = okc_app::ProjectStore::open(path)
            .map_err(|error| CliFailure::from_error(EXIT_INPUT, error))?;
        println!("project: {}", project.root().display());
        println!("sources: {}", project.manifest().sources.len());
        if let Some(warning) = project.privacy_warning() {
            println!("warning: {warning}");
        }
    } else {
        println!("project: none");
    }
    Ok(())
}

fn update_command(channel_or_version: &str, yes: bool) -> CliResult<()> {
    let target = okc_app::UpdateTarget::parse(channel_or_version)
        .map_err(|error| CliFailure::from_error(EXIT_USAGE, error))?;
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err(CliFailure::new(
                EXIT_USAGE,
                "update installation requires interactive confirmation or --yes",
            ));
        }
        eprint!("Install the `{channel_or_version}` OKC update? [y/N] ");
        std::io::stderr()
            .flush()
            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?;
        if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
            println!("update cancelled");
            return Ok(());
        }
    }
    match okc_app::install_update(&target)
        .map_err(|error| CliFailure::from_error(EXIT_OUTPUT, error))?
    {
        Some(update) => {
            println!(
                "updated OKC from {} to {} ({}) in {}",
                update.old_version.as_deref().unwrap_or("unknown"),
                update.new_version,
                update.release_tag,
                update.install_prefix
            );
        }
        None => println!("OKC is already at the requested release"),
    }
    Ok(())
}

fn print_json(value: &impl Serialize) -> CliResult<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CliFailure::from_error(EXIT_INTERNAL, error))?;
    println!("{json}");
    Ok(())
}

fn sanitize_terminal(value: &str) -> String {
    value
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
        .collect()
}
