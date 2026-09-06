use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{self, IsTerminal as _, Stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use okc_ai::{
    AiRole, DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_TIMEOUT_MS, DataBoundary, ProviderKind,
    ProviderProfile,
};
use okc_app::integration_service::{
    ClusterReviewDecision, IntegrationCheckpoint, PreflightSummary,
};
use okc_app::project_state::{ClusterTaskOutput, TaxonomyTaskOutput};
use okc_app::provider_service::{CredentialStoreStatus, SecretInput, default_endpoint};
use okc_app::worker::{CancelOutcome, Worker, WorkerEvent};
use okc_app::workspace_bootstrap::VaultCandidateKind;
use okc_app::{
    AppError, IntegrationService, ProgressEvent, ProjectStore, ProviderService, SourceBinding,
    VaultCandidate, WorkspaceBootstrap,
};
use okc_core::SourceId;
use okc_core::integration::{CriticSeverity, DispositionKind, TaxonomyCluster};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Workspace,
    AiConnection,
    Vaults,
    Preflight,
    Taxonomy,
    Clusters,
    Build,
    Verify,
    Provenance,
    Settings,
}

impl Screen {
    const ALL: [Self; 10] = [
        Self::Workspace,
        Self::AiConnection,
        Self::Vaults,
        Self::Preflight,
        Self::Taxonomy,
        Self::Clusters,
        Self::Build,
        Self::Verify,
        Self::Provenance,
        Self::Settings,
    ];

    fn label(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::Workspace, Language::English) => "Workspace",
            (Self::AiConnection, Language::English) => "AI Connection",
            (Self::Vaults, Language::English) => "Vaults",
            (Self::Preflight, Language::English) => "Preflight",
            (Self::Taxonomy, Language::English) => "Taxonomy",
            (Self::Clusters, Language::English) => "Clusters",
            (Self::Build, Language::English) => "Build",
            (Self::Verify, Language::English) => "Verify",
            (Self::Provenance, Language::English) => "Provenance",
            (Self::Settings, Language::English) => "Settings",
            (Self::Workspace, Language::Korean) => "작업공간",
            (Self::AiConnection, Language::Korean) => "AI 연결",
            (Self::Vaults, Language::Korean) => "Vault 선택",
            (Self::Preflight, Language::Korean) => "사전 점검",
            (Self::Taxonomy, Language::Korean) => "분류체계",
            (Self::Clusters, Language::Korean) => "클러스터",
            (Self::Build, Language::Korean) => "빌드",
            (Self::Verify, Language::Korean) => "검증",
            (Self::Provenance, Language::Korean) => "출처 추적",
            (Self::Settings, Language::Korean) => "설정",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Korean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiField {
    Kind,
    Profile,
    Endpoint,
    Model,
    CredentialMode,
    Credential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CredentialMode {
    Keychain,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiForm {
    field: AiField,
    kind: ProviderKind,
    profile: String,
    endpoint: String,
    model: String,
    credential_mode: CredentialMode,
    credential_ref: String,
    secret: SecretInput,
    embedding_setup: bool,
}

impl Default for AiForm {
    fn default() -> Self {
        Self {
            field: AiField::Kind,
            kind: ProviderKind::OpenAi,
            profile: "default".into(),
            endpoint: default_endpoint(ProviderKind::OpenAi)
                .unwrap_or_default()
                .into(),
            model: String::new(),
            credential_mode: CredentialMode::Keychain,
            credential_ref: "OPENAI_API_KEY".into(),
            secret: SecretInput::empty(),
            embedding_setup: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VaultChoice {
    candidate: VaultCandidate,
    source_id: String,
    selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaxonomyEdit {
    Title,
    Path,
    Rationale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClusterInput {
    Rationale,
    Feedback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingRemoteAction {
    Integration,
    ApproveTaxonomy {
        clusters: Option<Vec<TaxonomyCluster>>,
        rationale: Option<String>,
    },
    RegenerateCluster {
        cluster_id: String,
        feedback: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Model {
    pub screen: Screen,
    pub language: Language,
    pub ascii_mode: bool,
    pub high_contrast: bool,
    pub help_visible: bool,
    pub width: u16,
    pub height: u16,
    pub project: Option<PathBuf>,
    pub status: String,
    cwd: PathBuf,
    projects: Vec<PathBuf>,
    vaults: Vec<VaultChoice>,
    cursor: usize,
    edit_source_id: bool,
    manual_path: Option<String>,
    ai: AiForm,
    configured_default: Option<String>,
    configured_embedding: Option<String>,
    keychain_status: CredentialStoreStatus,
    busy: bool,
    cancel_confirm: bool,
    progress: Option<ProgressEvent>,
    preflight: Option<PreflightSummary>,
    remote_routes: bool,
    consent_confirm: bool,
    pending_remote_action: Option<PendingRemoteAction>,
    taxonomy: Option<TaxonomyTaskOutput>,
    taxonomy_clusters: Vec<TaxonomyCluster>,
    taxonomy_cursor: usize,
    taxonomy_edit: Option<TaxonomyEdit>,
    taxonomy_edited: bool,
    taxonomy_rationale: String,
    clusters: Vec<ClusterTaskOutput>,
    cluster_cursor: usize,
    issue_cursor: usize,
    issue_acknowledged: BTreeSet<String>,
    issue_rationales: BTreeMap<String, String>,
    cluster_input: Option<ClusterInput>,
    regeneration_feedback: String,
    output: Option<PathBuf>,
    output_edit: Option<String>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            screen: Screen::Workspace,
            language: Language::English,
            ascii_mode: false,
            high_contrast: false,
            help_visible: false,
            width: MIN_WIDTH,
            height: MIN_HEIGHT,
            project: None,
            status: "[ ] Discovering the current folder".into(),
            cwd: PathBuf::from("."),
            projects: Vec::new(),
            vaults: Vec::new(),
            cursor: 0,
            edit_source_id: false,
            manual_path: None,
            ai: AiForm::default(),
            configured_default: None,
            configured_embedding: None,
            keychain_status: CredentialStoreStatus::Unavailable,
            busy: false,
            cancel_confirm: false,
            progress: None,
            preflight: None,
            remote_routes: false,
            consent_confirm: false,
            pending_remote_action: None,
            taxonomy: None,
            taxonomy_clusters: Vec::new(),
            taxonomy_cursor: 0,
            taxonomy_edit: None,
            taxonomy_edited: false,
            taxonomy_rationale: String::new(),
            clusters: Vec::new(),
            cluster_cursor: 0,
            issue_cursor: 0,
            issue_acknowledged: BTreeSet::new(),
            issue_rationales: BTreeMap::new(),
            cluster_input: None,
            regeneration_feedback: String::new(),
            output: None,
            output_edit: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    WorkerProgress(ProgressEvent),
    WorkerFinished {
        operation: String,
        result: std::result::Result<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Quit,
    CancelOperation,
    OpenProject(PathBuf),
    AddManual(PathBuf),
    SaveSources(Vec<SourceBinding>),
    ConnectProvider {
        name: String,
        profile: ProviderProfile,
        secret: Option<SecretInput>,
        embedding_only: bool,
    },
    RunPreflight,
    RunIntegration {
        allow_remote: bool,
    },
    ApproveTaxonomy {
        clusters: Option<Vec<TaxonomyCluster>>,
        rationale: Option<String>,
        allow_remote: bool,
    },
    ApproveCluster {
        cluster_id: String,
        decision: ClusterReviewDecision,
    },
    RegenerateCluster {
        cluster_id: String,
        feedback: String,
        allow_remote: bool,
    },
    Compile(PathBuf),
    Verify(PathBuf),
}

#[allow(
    clippy::too_many_lines,
    reason = "the reducer keeps input precedence explicit"
)]
pub fn reduce(mut model: Model, event: AppEvent) -> (Model, Vec<Effect>) {
    let mut effects = Vec::new();
    match event {
        AppEvent::Resize(width, height) => {
            model.width = width;
            model.height = height;
        }
        AppEvent::WorkerProgress(progress) => {
            model.progress = Some(progress);
        }
        AppEvent::WorkerFinished { operation, result } => {
            model.busy = false;
            model.progress = None;
            model.cancel_confirm = false;
            match result {
                Ok(value) => {
                    model.status = format!("[OK] {value}");
                    if operation == "sources" {
                        model.project = Some(PathBuf::from(value));
                        model.preflight = None;
                        model.taxonomy = None;
                        model.taxonomy_clusters.clear();
                        model.clusters.clear();
                    } else if operation == "preflight" {
                        model.preflight = serde_json::from_str(&value).ok();
                    } else if operation == "provider-primary" {
                        model.preflight = None;
                        model.taxonomy = None;
                        model.taxonomy_clusters.clear();
                        model.clusters.clear();
                        model.configured_default = Some(model.ai.profile.clone());
                        if model.ai.kind == ProviderKind::Anthropic {
                            model.ai = AiForm {
                                embedding_setup: true,
                                profile: "embedding".into(),
                                ..AiForm::default()
                            };
                            model.status =
                                "[OK] Anthropic connected; configure an embedding profile".into();
                        } else if model.project.is_none() {
                            model.screen = Screen::Vaults;
                        }
                    } else if operation == "provider-embedding" {
                        model.preflight = None;
                        model.taxonomy = None;
                        model.taxonomy_clusters.clear();
                        model.clusters.clear();
                        model.configured_embedding = Some(model.ai.profile.clone());
                        if model.project.is_none() {
                            model.screen = Screen::Vaults;
                        }
                    }
                }
                Err(error) => {
                    if error.contains("sensitive content requires a local provider")
                        || error.contains("require local embedding and organizer profiles")
                    {
                        model.screen = Screen::AiConnection;
                    }
                    model.status = format!("[ERROR] {error}");
                }
            }
        }
        AppEvent::Key(key) => {
            if model.cancel_confirm {
                match key.code {
                    KeyCode::Char('y' | 'Y') => {
                        model.cancel_confirm = false;
                        effects.push(Effect::CancelOperation);
                    }
                    KeyCode::Char('n' | 'N') | KeyCode::Esc => model.cancel_confirm = false,
                    _ => {}
                }
                return (model, effects);
            }
            if model.consent_confirm {
                match key.code {
                    KeyCode::Char('y' | 'Y') => {
                        model.consent_confirm = false;
                        if let Some(action) = model.pending_remote_action.take() {
                            effects.push(match action {
                                PendingRemoteAction::Integration => {
                                    Effect::RunIntegration { allow_remote: true }
                                }
                                PendingRemoteAction::ApproveTaxonomy {
                                    clusters,
                                    rationale,
                                } => Effect::ApproveTaxonomy {
                                    clusters,
                                    rationale,
                                    allow_remote: true,
                                },
                                PendingRemoteAction::RegenerateCluster {
                                    cluster_id,
                                    feedback,
                                } => Effect::RegenerateCluster {
                                    cluster_id,
                                    feedback,
                                    allow_remote: true,
                                },
                            });
                        }
                    }
                    KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                        model.consent_confirm = false;
                        model.pending_remote_action = None;
                    }
                    _ => {}
                }
                return (model, effects);
            }
            if model.busy {
                if key.code == KeyCode::Esc
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    model.cancel_confirm = true;
                }
                return (model, effects);
            }
            if handle_text_entry(&mut model, key) {
                return (model, effects);
            }
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    effects.push(Effect::Quit);
                }
                KeyCode::Char('?') => model.help_visible = !model.help_visible,
                KeyCode::Esc if model.help_visible => model.help_visible = false,
                KeyCode::Esc => effects.push(Effect::Quit),
                _ => reduce_screen_key(&mut model, key, &mut effects),
            }
        }
    }
    (model, effects)
}

#[allow(clippy::too_many_lines, reason = "centralized text-entry precedence")]
fn handle_text_entry(model: &mut Model, key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    if model.screen == Screen::AiConnection {
        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                model.ai.field = next_ai_field(model.ai.field, 1);
                return true;
            }
            KeyCode::BackTab | KeyCode::Up => {
                model.ai.field = next_ai_field(model.ai.field, -1);
                return true;
            }
            KeyCode::Left if model.ai.field == AiField::Kind => {
                set_provider_kind(&mut model.ai, -1);
                return true;
            }
            KeyCode::Right if model.ai.field == AiField::Kind => {
                set_provider_kind(&mut model.ai, 1);
                return true;
            }
            KeyCode::Char(' ') if model.ai.field == AiField::CredentialMode => {
                model.ai.credential_mode = match model.ai.credential_mode {
                    CredentialMode::Keychain => CredentialMode::Environment,
                    CredentialMode::Environment => CredentialMode::Keychain,
                };
                return true;
            }
            KeyCode::Backspace => {
                match model.ai.field {
                    AiField::Profile => {
                        model.ai.profile.pop();
                    }
                    AiField::Endpoint => {
                        model.ai.endpoint.pop();
                    }
                    AiField::Model => {
                        model.ai.model.pop();
                    }
                    AiField::Credential if model.ai.credential_mode == CredentialMode::Keychain => {
                        model.ai.secret.pop();
                    }
                    AiField::Credential => {
                        model.ai.credential_ref.pop();
                    }
                    AiField::Kind | AiField::CredentialMode => return false,
                }
                return true;
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                match model.ai.field {
                    AiField::Profile => model.ai.profile.push(character),
                    AiField::Endpoint => model.ai.endpoint.push(character),
                    AiField::Model => model.ai.model.push(character),
                    AiField::Credential if model.ai.credential_mode == CredentialMode::Keychain => {
                        model.ai.secret.push(character);
                    }
                    AiField::Credential => model.ai.credential_ref.push(character),
                    AiField::Kind | AiField::CredentialMode => return false,
                }
                return true;
            }
            KeyCode::Enter => {
                let profile = provider_profile(model);
                let secret = if model.ai.credential_mode == CredentialMode::Keychain
                    && !model.ai.secret.is_empty()
                {
                    Some(std::mem::replace(
                        &mut model.ai.secret,
                        SecretInput::empty(),
                    ))
                } else {
                    None
                };
                let operation = if model.ai.embedding_setup {
                    "provider-embedding"
                } else {
                    "provider-primary"
                };
                model.busy = true;
                model.status = format!("[ ] Testing {} with synthetic requests", model.ai.profile);
                // The caller receives this through a synthetic key handled below.
                model.progress = None;
                PENDING_EFFECT.with(|slot| {
                    *slot.borrow_mut() = Some(Effect::ConnectProvider {
                        name: model.ai.profile.clone(),
                        profile,
                        secret,
                        embedding_only: model.ai.embedding_setup,
                    });
                });
                model.status.push_str(if operation == "provider-embedding" {
                    " (embedding)"
                } else {
                    ""
                });
                return false;
            }
            _ => {}
        }
    }
    if model.screen == Screen::Vaults {
        if let Some(path) = model.manual_path.as_mut() {
            match key.code {
                KeyCode::Esc => model.manual_path = None,
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    path.push(character);
                }
                KeyCode::Enter => {
                    let path = PathBuf::from(std::mem::take(path));
                    model.manual_path = None;
                    PENDING_EFFECT.with(|slot| *slot.borrow_mut() = Some(Effect::AddManual(path)));
                    return false;
                }
                _ => {}
            }
            return true;
        }
        if model.edit_source_id {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => model.edit_source_id = false,
                KeyCode::Backspace => {
                    if let Some(choice) = model.vaults.get_mut(model.cursor) {
                        choice.source_id.pop();
                    }
                }
                KeyCode::Char(character)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && (character.is_ascii_alphanumeric()
                            || matches!(character, '-' | '_' | '.')) =>
                {
                    if let Some(choice) = model.vaults.get_mut(model.cursor) {
                        choice.source_id.push(character);
                    }
                }
                _ => {}
            }
            return true;
        }
    }
    if let Some(edit) = model.taxonomy_edit {
        let target = match edit {
            TaxonomyEdit::Title => model
                .taxonomy_clusters
                .get_mut(model.taxonomy_cursor)
                .map(|cluster| &mut cluster.title),
            TaxonomyEdit::Path => model
                .taxonomy_clusters
                .get_mut(model.taxonomy_cursor)
                .map(|cluster| &mut cluster.canonical_path),
            TaxonomyEdit::Rationale => Some(&mut model.taxonomy_rationale),
        };
        match key.code {
            KeyCode::Esc | KeyCode::Enter => model.taxonomy_edit = None,
            KeyCode::Backspace => {
                if let Some(target) = target {
                    target.pop();
                    model.taxonomy_edited = true;
                }
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(target) = target {
                    target.push(character);
                    model.taxonomy_edited = true;
                }
            }
            _ => {}
        }
        return true;
    }
    if let Some(input) = model.cluster_input {
        let issue_key = (input == ClusterInput::Rationale)
            .then(|| cluster_issue_keys(model).get(model.issue_cursor).cloned())
            .flatten();
        let target = match input {
            ClusterInput::Rationale => {
                issue_key.map(|key| model.issue_rationales.entry(key).or_default())
            }
            ClusterInput::Feedback => Some(&mut model.regeneration_feedback),
        };
        match key.code {
            KeyCode::Esc | KeyCode::Enter => model.cluster_input = None,
            KeyCode::Backspace => {
                if let Some(target) = target {
                    target.pop();
                }
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(target) = target {
                    target.push(character);
                }
            }
            _ => {}
        }
        return true;
    }
    if let Some(output) = model.output_edit.as_mut() {
        match key.code {
            KeyCode::Esc => model.output_edit = None,
            KeyCode::Enter => {
                model.output = Some(PathBuf::from(std::mem::take(output)));
                model.output_edit = None;
            }
            KeyCode::Backspace => {
                output.pop();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                output.push(character);
            }
            _ => {}
        }
        return true;
    }
    false
}

thread_local! {
    static PENDING_EFFECT: std::cell::RefCell<Option<Effect>> = const { std::cell::RefCell::new(None) };
}

#[allow(clippy::too_many_lines, reason = "screen actions remain explicit")]
fn reduce_screen_key(model: &mut Model, key: KeyEvent, effects: &mut Vec<Effect>) {
    if let Some(effect) = PENDING_EFFECT.with(|slot| slot.borrow_mut().take()) {
        effects.push(effect);
        return;
    }
    match model.screen {
        Screen::Workspace => match key.code {
            KeyCode::Up => model.cursor = model.cursor.saturating_sub(1),
            KeyCode::Down => model.cursor = next_index(model.cursor, model.projects.len()),
            KeyCode::Enter => {
                if let Some(path) = model.projects.get(model.cursor) {
                    effects.push(Effect::OpenProject(path.clone()));
                } else {
                    model.screen = Screen::AiConnection;
                }
            }
            _ => navigate_screen(model, key),
        },
        Screen::AiConnection => {}
        Screen::Vaults => match key.code {
            KeyCode::Up => model.cursor = model.cursor.saturating_sub(1),
            KeyCode::Down => model.cursor = next_index(model.cursor, model.vaults.len()),
            KeyCode::Char(' ') => {
                let selected = model.vaults.iter().filter(|item| item.selected).count();
                if let Some(choice) = model.vaults.get_mut(model.cursor)
                    && (choice.selected || selected < 10)
                {
                    choice.selected = !choice.selected;
                }
            }
            KeyCode::Char('e') => model.edit_source_id = true,
            KeyCode::Char('m') => model.manual_path = Some(String::new()),
            KeyCode::Enter => match selected_bindings(model) {
                Ok(bindings) => effects.push(Effect::SaveSources(bindings)),
                Err(error) => model.status = format!("[ERROR] {error}"),
            },
            _ => navigate_screen(model, key),
        },
        Screen::Preflight => match key.code {
            KeyCode::Enter if model.preflight.is_none() => effects.push(Effect::RunPreflight),
            KeyCode::Enter => {
                let remote = model.preflight.as_ref().is_some_and(|summary| {
                    summary
                        .routes
                        .iter()
                        .any(|route| route.boundary == okc_ai::DataBoundary::Remote)
                });
                if remote {
                    model.consent_confirm = true;
                    model.pending_remote_action = Some(PendingRemoteAction::Integration);
                } else {
                    effects.push(Effect::RunIntegration {
                        allow_remote: false,
                    });
                }
            }
            _ => navigate_screen(model, key),
        },
        Screen::Taxonomy => taxonomy_key(model, key, effects),
        Screen::Clusters => cluster_key(model, key, effects),
        Screen::Build => match key.code {
            KeyCode::Char('e') => {
                model.output_edit = Some(
                    model
                        .output
                        .as_ref()
                        .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
                );
            }
            KeyCode::Enter => {
                if let Some(path) = &model.output {
                    effects.push(Effect::Compile(path.clone()));
                } else {
                    model.status = "[ERROR] No safe output path is available".into();
                }
            }
            _ => navigate_screen(model, key),
        },
        Screen::Verify => match key.code {
            KeyCode::Enter => {
                if let Some(path) = &model.output {
                    effects.push(Effect::Verify(path.clone()));
                }
            }
            _ => navigate_screen(model, key),
        },
        Screen::Settings => match key.code {
            KeyCode::Char('l') => {
                model.language = if model.language == Language::English {
                    Language::Korean
                } else {
                    Language::English
                }
            }
            KeyCode::Char('a') => model.ascii_mode = !model.ascii_mode,
            KeyCode::Char('h') => model.high_contrast = !model.high_contrast,
            _ => navigate_screen(model, key),
        },
        Screen::Provenance => navigate_screen(model, key),
    }
    if let Some(effect) = PENDING_EFFECT.with(|slot| slot.borrow_mut().take()) {
        effects.push(effect);
    }
}

fn taxonomy_key(model: &mut Model, key: KeyEvent, effects: &mut Vec<Effect>) {
    match key.code {
        KeyCode::Up => model.taxonomy_cursor = model.taxonomy_cursor.saturating_sub(1),
        KeyCode::Down => {
            model.taxonomy_cursor =
                next_index(model.taxonomy_cursor, model.taxonomy_clusters.len());
        }
        KeyCode::Char('e') => model.taxonomy_edit = Some(TaxonomyEdit::Title),
        KeyCode::Char('p') => model.taxonomy_edit = Some(TaxonomyEdit::Path),
        KeyCode::Char('x') => model.taxonomy_edit = Some(TaxonomyEdit::Rationale),
        KeyCode::Char('m') if model.taxonomy_cursor > 0 => {
            let removed = model.taxonomy_clusters.remove(model.taxonomy_cursor);
            model.taxonomy_cursor -= 1;
            model.taxonomy_clusters[model.taxonomy_cursor]
                .document_ids
                .extend(removed.document_ids);
            model.taxonomy_clusters[model.taxonomy_cursor]
                .document_ids
                .sort();
            model.taxonomy_clusters[model.taxonomy_cursor]
                .document_ids
                .dedup();
            model.taxonomy_edited = true;
        }
        KeyCode::Char('s') => split_taxonomy_cluster(model),
        KeyCode::Left => move_taxonomy_document(model, -1),
        KeyCode::Right => move_taxonomy_document(model, 1),
        KeyCode::Char('a') | KeyCode::Enter => {
            if model.taxonomy_edited && model.taxonomy_rationale.trim().is_empty() {
                model.status = "[ERROR] Edited taxonomy requires rationale (X)".into();
            } else {
                let clusters = model
                    .taxonomy_edited
                    .then(|| model.taxonomy_clusters.clone());
                let rationale = (!model.taxonomy_rationale.trim().is_empty())
                    .then(|| model.taxonomy_rationale.clone());
                if has_remote_route(model) {
                    model.consent_confirm = true;
                    model.pending_remote_action = Some(PendingRemoteAction::ApproveTaxonomy {
                        clusters,
                        rationale,
                    });
                } else {
                    effects.push(Effect::ApproveTaxonomy {
                        clusters,
                        rationale,
                        allow_remote: false,
                    });
                }
            }
        }
        _ => navigate_screen(model, key),
    }
}

fn cluster_key(model: &mut Model, key: KeyEvent, effects: &mut Vec<Effect>) {
    match key.code {
        KeyCode::Char('[') => {
            model.cluster_cursor = model.cluster_cursor.saturating_sub(1);
            reset_cluster_review(model);
        }
        KeyCode::Char(']') => {
            model.cluster_cursor = next_index(model.cluster_cursor, model.clusters.len());
            reset_cluster_review(model);
        }
        KeyCode::Up => model.issue_cursor = model.issue_cursor.saturating_sub(1),
        KeyCode::Down => {
            model.issue_cursor = next_index(model.issue_cursor, cluster_issue_keys(model).len());
        }
        KeyCode::Char(' ') => {
            if let Some(key) = cluster_issue_keys(model).get(model.issue_cursor).cloned()
                && !model.issue_acknowledged.remove(&key)
            {
                model.issue_acknowledged.insert(key);
            }
        }
        KeyCode::Char('w') => model.cluster_input = Some(ClusterInput::Rationale),
        KeyCode::Char('r') => model.cluster_input = Some(ClusterInput::Feedback),
        KeyCode::Char('g') => {
            if let Some(cluster) = model.clusters.get(model.cluster_cursor) {
                if model.regeneration_feedback.trim().is_empty() {
                    model.status = "[ERROR] Enter regeneration feedback with R first".into();
                } else {
                    let cluster_id = cluster.proposal.cluster_id.clone();
                    let feedback = model.regeneration_feedback.clone();
                    if has_remote_route(model) {
                        model.consent_confirm = true;
                        model.pending_remote_action =
                            Some(PendingRemoteAction::RegenerateCluster {
                                cluster_id,
                                feedback,
                            });
                    } else {
                        effects.push(Effect::RegenerateCluster {
                            cluster_id,
                            feedback,
                            allow_remote: false,
                        });
                    }
                }
            }
        }
        KeyCode::Char('a') | KeyCode::Enter => approve_current_cluster(model, effects),
        _ => navigate_screen(model, key),
    }
}

fn approve_current_cluster(model: &mut Model, effects: &mut Vec<Effect>) {
    let Some(cluster) = model.clusters.get(model.cluster_cursor) else {
        return;
    };
    if cluster.critic.findings.iter().any(|finding| {
        matches!(
            finding.severity,
            CriticSeverity::Major | CriticSeverity::Critical
        )
    }) {
        model.status = "[ERROR] Major/critical findings require regeneration".into();
        return;
    }
    let issue_keys = cluster_issue_keys(model);
    if issue_keys
        .iter()
        .any(|key| !model.issue_acknowledged.contains(key))
    {
        model.status =
            "[ERROR] Acknowledge every omission/minor finding individually with Space".into();
        return;
    }
    if issue_keys.iter().any(|key| {
        model
            .issue_rationales
            .get(key)
            .is_none_or(|rationale| rationale.trim().is_empty())
    }) {
        model.status = "[ERROR] Every omission/minor finding needs its own rationale (W)".into();
        return;
    }
    let mut decision = ClusterReviewDecision::default();
    for disposition in cluster
        .proposal
        .dispositions
        .iter()
        .filter(|item| item.disposition == DispositionKind::OmissionProposed)
    {
        let key = format!(
            "{}:{}",
            disposition.target.document_id, disposition.target.target_id
        );
        decision.omission_rationales.insert(
            key.clone(),
            model.issue_rationales[&format!("omission:{key}")].clone(),
        );
    }
    for finding in cluster
        .critic
        .findings
        .iter()
        .filter(|finding| finding.severity == CriticSeverity::Minor)
    {
        decision.minor_waivers.insert(
            finding.finding_id.clone(),
            model.issue_rationales[&format!("minor:{}", finding.finding_id)].clone(),
        );
    }
    effects.push(Effect::ApproveCluster {
        cluster_id: cluster.proposal.cluster_id.clone(),
        decision,
    });
}

fn navigate_screen(model: &mut Model, key: KeyEvent) {
    let delta = match key.code {
        KeyCode::Tab | KeyCode::Right => 1,
        KeyCode::BackTab | KeyCode::Left => -1,
        _ => return,
    };
    let current = i32::try_from(
        Screen::ALL
            .iter()
            .position(|screen| *screen == model.screen)
            .unwrap_or(0),
    )
    .expect("screen index fits i32");
    let len = i32::try_from(Screen::ALL.len()).expect("screen count fits i32");
    let next = usize::try_from((current + delta).rem_euclid(len)).expect("non-negative index");
    model.screen = Screen::ALL[next];
    model.cursor = 0;
}

fn next_ai_field(field: AiField, delta: i8) -> AiField {
    const FIELDS: [AiField; 6] = [
        AiField::Kind,
        AiField::Profile,
        AiField::Endpoint,
        AiField::Model,
        AiField::CredentialMode,
        AiField::Credential,
    ];
    let current = i32::try_from(FIELDS.iter().position(|item| *item == field).unwrap_or(0))
        .expect("field index fits i32");
    let len = i32::try_from(FIELDS.len()).expect("field count fits i32");
    FIELDS
        [usize::try_from((current + i32::from(delta)).rem_euclid(len)).expect("non-negative index")]
}

fn set_provider_kind(form: &mut AiForm, delta: i8) {
    const KINDS: [ProviderKind; 5] = [
        ProviderKind::OpenAi,
        ProviderKind::Anthropic,
        ProviderKind::Gemini,
        ProviderKind::Ollama,
        ProviderKind::OpenAiCompatible,
    ];
    let current = i32::try_from(
        KINDS
            .iter()
            .position(|kind| *kind == form.kind)
            .unwrap_or(0),
    )
    .expect("provider index fits i32");
    let len = i32::try_from(KINDS.len()).expect("provider count fits i32");
    form.kind = KINDS[usize::try_from((current + i32::from(delta)).rem_euclid(len))
        .expect("non-negative index")];
    form.endpoint = default_endpoint(form.kind).unwrap_or_default().into();
    form.credential_ref = match form.kind {
        ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => "OPENAI_API_KEY",
        ProviderKind::Anthropic => "ANTHROPIC_API_KEY",
        ProviderKind::Gemini => "GEMINI_API_KEY",
        ProviderKind::Ollama => "",
    }
    .into();
}

fn provider_profile(model: &Model) -> ProviderProfile {
    let local = model.ai.kind == ProviderKind::Ollama;
    ProviderProfile {
        kind: model.ai.kind,
        endpoint: model.ai.endpoint.clone(),
        model: model.ai.model.clone(),
        api_key_env: (model.ai.credential_mode == CredentialMode::Environment
            && !model.ai.credential_ref.is_empty())
        .then(|| model.ai.credential_ref.clone()),
        os_keychain: (model.ai.credential_mode == CredentialMode::Keychain && !local)
            .then(|| model.ai.profile.clone()),
        timeout_ms: DEFAULT_TIMEOUT_MS,
        max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        max_input_bytes: 64 * 1024 * 1024,
        max_batch_items: 2_048,
        options: BTreeMap::new(),
    }
}

fn selected_bindings(model: &Model) -> Result<Vec<SourceBinding>, String> {
    model
        .vaults
        .iter()
        .filter(|choice| choice.selected)
        .map(|choice| {
            Ok(SourceBinding {
                source_id: SourceId::new(&choice.source_id).map_err(|error| error.to_string())?,
                owner_display_name: None,
                path: choice.candidate.path.clone(),
                snapshot_id: None,
            })
        })
        .collect()
}

fn next_index(current: usize, length: usize) -> usize {
    if length == 0 {
        0
    } else {
        (current + 1).min(length - 1)
    }
}

fn split_taxonomy_cluster(model: &mut Model) {
    let Some(cluster) = model.taxonomy_clusters.get_mut(model.taxonomy_cursor) else {
        return;
    };
    if cluster.document_ids.len() < 2 {
        model.status = "[ERROR] Split needs at least two documents".into();
        return;
    }
    let document = cluster.document_ids.pop().expect("checked length");
    let suffix = model.taxonomy_clusters.len() + 1;
    model.taxonomy_clusters.push(TaxonomyCluster {
        cluster_id: format!("manual-{suffix}"),
        title: format!("Split {suffix}"),
        canonical_path: format!("manual-split-{suffix}.md"),
        document_ids: vec![document],
    });
    model.taxonomy_edited = true;
}

fn move_taxonomy_document(model: &mut Model, delta: i8) {
    if model.taxonomy_clusters.len() < 2 {
        return;
    }
    let source = model.taxonomy_cursor;
    let source_index = i32::try_from(source).expect("taxonomy index fits i32");
    let len = i32::try_from(model.taxonomy_clusters.len()).expect("taxonomy count fits i32");
    let target = usize::try_from((source_index + i32::from(delta)).rem_euclid(len))
        .expect("non-negative index");
    if model.taxonomy_clusters[source].document_ids.len() < 2 {
        return;
    }
    let document = model.taxonomy_clusters[source]
        .document_ids
        .pop()
        .expect("checked length");
    model.taxonomy_clusters[target].document_ids.push(document);
    model.taxonomy_clusters[target].document_ids.sort();
    model.taxonomy_edited = true;
}

fn cluster_issue_keys(model: &Model) -> Vec<String> {
    let Some(cluster) = model.clusters.get(model.cluster_cursor) else {
        return Vec::new();
    };
    let mut keys = cluster
        .proposal
        .dispositions
        .iter()
        .filter(|item| item.disposition == DispositionKind::OmissionProposed)
        .map(|item| {
            format!(
                "omission:{}:{}",
                item.target.document_id, item.target.target_id
            )
        })
        .chain(
            cluster
                .critic
                .findings
                .iter()
                .filter(|item| item.severity == CriticSeverity::Minor)
                .map(|item| format!("minor:{}", item.finding_id)),
        )
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

fn has_remote_route(model: &Model) -> bool {
    model.remote_routes
        || model.preflight.as_ref().is_some_and(|summary| {
            summary
                .routes
                .iter()
                .any(|route| route.boundary == DataBoundary::Remote)
        })
}

fn reset_cluster_review(model: &mut Model) {
    model.issue_cursor = 0;
    model.issue_acknowledged.clear();
    model.issue_rationales.clear();
    model.regeneration_feedback.clear();
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(stdout))?,
        })
    }
    fn restore(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

pub fn run(project: Option<&Path>) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("TUI requires an interactive terminal".into());
    }
    let bootstrap = WorkspaceBootstrap::from_current_dir().map_err(|error| error.to_string())?;
    let discovery = bootstrap
        .discover(project)
        .map_err(|error| error.to_string())?;
    let providers = ProviderService::from_environment().map_err(|error| error.to_string())?;
    let mut model = Model {
        cwd: discovery.cwd,
        projects: discovery
            .projects
            .into_iter()
            .map(|candidate| candidate.path)
            .collect(),
        vaults: discovery
            .vaults
            .into_iter()
            .map(|candidate| VaultChoice {
                source_id: candidate.suggested_source_id.clone(),
                candidate,
                selected: false,
            })
            .collect(),
        keychain_status: providers.credential_store_status(),
        ..Model::default()
    };
    if model.keychain_status != CredentialStoreStatus::Available {
        model.ai.credential_mode = CredentialMode::Environment;
    }
    if model.projects.len() == 1 {
        let path = model.projects[0].clone();
        open_project(&mut model, &path)?;
    } else if model.projects.len() > 1 {
        model.status = format!(
            "[ ] Select one of {} discovered projects",
            model.projects.len()
        );
    } else if providers
        .load_config()
        .map_err(|error| error.to_string())?
        .profiles
        .is_empty()
    {
        model.screen = Screen::AiConnection;
        model.status = "[ ] Connect an AI provider before selecting Vaults".into();
    } else {
        model.screen = Screen::Vaults;
        model.status = "[ ] Select 1–10 immutable Vault sources".into();
    }

    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        old_hook(info);
    }));
    let mut terminal = TerminalGuard::enter().map_err(|error| error.to_string())?;
    let worker = Worker::spawn();
    loop {
        while let Some(event) = worker.try_recv() {
            let app_event = match event {
                WorkerEvent::Progress(progress) => AppEvent::WorkerProgress(progress),
                WorkerEvent::Finished { operation, result } => {
                    AppEvent::WorkerFinished { operation, result }
                }
            };
            let refresh = matches!(&app_event, AppEvent::WorkerFinished { operation, result: Ok(_), .. } if operation != "preflight");
            let (next, _) = reduce(model, app_event);
            model = next;
            if refresh {
                refresh_project_state(&mut model);
            }
        }
        terminal
            .terminal
            .draw(|frame| render(frame, &model))
            .map_err(|error| error.to_string())?;
        if event::poll(Duration::from_millis(80)).map_err(|error| error.to_string())? {
            let app_event = match event::read().map_err(|error| error.to_string())? {
                CrosstermEvent::Key(key) => Some(AppEvent::Key(key)),
                CrosstermEvent::Resize(width, height) => Some(AppEvent::Resize(width, height)),
                _ => None,
            };
            if let Some(app_event) = app_event {
                let (next, effects) = reduce(model, app_event);
                model = next;
                for effect in effects {
                    if execute_effect(effect, &mut model, &worker)? {
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "effect-to-service boundaries are explicit"
)]
fn execute_effect(effect: Effect, model: &mut Model, worker: &Worker) -> Result<bool, String> {
    match effect {
        Effect::Quit => return Ok(true),
        Effect::CancelOperation => {
            model.status = match worker.cancel() {
                CancelOutcome::Requested => "[!] Cancellation requested".into(),
                CancelOutcome::PublicationBarrier => {
                    "[!] Publication barrier reached; cancellation is disabled".into()
                }
                CancelOutcome::Idle => "[ ] No operation is running".into(),
            };
        }
        Effect::OpenProject(path) => open_project(model, &path)?,
        Effect::AddManual(path) => {
            let candidate = WorkspaceBootstrap::new(&model.cwd)
                .map_err(|error| error.to_string())?
                .manual_vault(path)
                .map_err(|error| error.to_string())?;
            model.vaults.push(VaultChoice {
                source_id: candidate.suggested_source_id.clone(),
                candidate,
                selected: true,
            });
            model
                .vaults
                .sort_by(|left, right| left.candidate.path.cmp(&right.candidate.path));
        }
        Effect::SaveSources(bindings) => {
            let cwd = model.cwd.clone();
            let project = model.project.clone();
            let default_profile = model.configured_default.clone();
            let embedding_profile = model.configured_embedding.clone();
            submit(worker, model, "sources", move |_| {
                let bootstrap = WorkspaceBootstrap::new(&cwd)?;
                let path = if let Some(path) = project {
                    let mut store = ProjectStore::open(&path)?;
                    store.replace_sources(bindings)?;
                    path
                } else {
                    let path = bootstrap.suggested_project_path(&bindings)?;
                    let name = cwd
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Workspace");
                    let mut store = ProjectStore::create(&path, name, "curator", "policy-v3")?;
                    if let Some(profile) = default_profile {
                        store.set_ai_route(None, profile)?;
                    }
                    if let Some(profile) = embedding_profile {
                        store.set_ai_route(Some(okc_ai::AiRole::Embedding), profile)?;
                    }
                    store.replace_sources(bindings)?;
                    path
                };
                Ok(path.to_string_lossy().into_owned())
            });
        }
        Effect::ConnectProvider {
            name,
            profile,
            secret,
            embedding_only,
        } => {
            let project = model.project.clone();
            let anthropic = profile.kind == ProviderKind::Anthropic;
            let operation = if embedding_only {
                "provider-embedding"
            } else {
                "provider-primary"
            };
            submit(worker, model, operation, move |control| {
                let service = ProviderService::from_environment()?;
                service.upsert_profile(&name, profile, secret)?;
                let capability = service.test_profile(&name, &control.cancellation)?;
                if (embedding_only || !anthropic) && !capability.embeddings {
                    return Err(AppError::InvalidProject(
                        "the selected profile does not provide embeddings".into(),
                    ));
                }
                if let Some(project) = project {
                    let mut project = ProjectStore::open(project)?;
                    project.set_ai_route(
                        if embedding_only {
                            Some(okc_ai::AiRole::Embedding)
                        } else {
                            None
                        },
                        name.clone(),
                    )?;
                }
                Ok(format!("profile `{name}` passed capability tests"))
            });
        }
        Effect::RunPreflight => {
            let Some(project) = model.project.clone() else {
                return Err("preflight requires a project".into());
            };
            submit(worker, model, "preflight", move |control| {
                let service =
                    IntegrationService::new(project, ProviderService::from_environment()?);
                Ok(serde_json::to_string(&service.preflight(&control)?)?)
            });
        }
        Effect::RunIntegration { allow_remote } => {
            let Some(project) = model.project.clone() else {
                return Err("integration requires a project".into());
            };
            submit(worker, model, "integrate", move |control| {
                IntegrationService::new(project, ProviderService::from_environment()?).execute(
                    allow_remote,
                    allow_remote,
                    &control,
                )?;
                Ok("integration checkpoint completed".into())
            });
        }
        Effect::ApproveTaxonomy {
            clusters,
            rationale,
            allow_remote,
        } => {
            let Some(project) = model.project.clone() else {
                return Err("taxonomy approval requires a project".into());
            };
            submit(worker, model, "taxonomy", move |control| {
                let service =
                    IntegrationService::new(&project, ProviderService::from_environment()?);
                service.approve_taxonomy(clusters, rationale)?;
                service.execute(allow_remote, allow_remote, &control)?;
                Ok("taxonomy approved; cluster revisions generated or resumed".into())
            });
        }
        Effect::ApproveCluster {
            cluster_id,
            decision,
        } => {
            let Some(project) = model.project.clone() else {
                return Err("cluster approval requires a project".into());
            };
            submit(worker, model, "cluster", move |_| {
                IntegrationService::new(project, ProviderService::from_environment()?)
                    .approve_cluster(&cluster_id, &decision)?;
                Ok(format!("cluster `{cluster_id}` approved"))
            });
        }
        Effect::RegenerateCluster {
            cluster_id,
            feedback,
            allow_remote,
        } => {
            let Some(project) = model.project.clone() else {
                return Err("regeneration requires a project".into());
            };
            submit(worker, model, "regenerate", move |control| {
                let service =
                    IntegrationService::new(&project, ProviderService::from_environment()?);
                service.request_cluster_regeneration(&cluster_id, feedback)?;
                service.execute(allow_remote, allow_remote, &control)?;
                Ok(format!(
                    "cluster `{cluster_id}` regenerated and re-criticized"
                ))
            });
        }
        Effect::Compile(output) => {
            let Some(project) = model.project.clone() else {
                return Err("compile requires a project".into());
            };
            submit(worker, model, "compile", move |control| {
                let (artifact, _) =
                    IntegrationService::new(project, ProviderService::from_environment()?)
                        .compile_latest(&output, &control)?;
                Ok(format!(
                    "verified output published at {}",
                    artifact.path.display()
                ))
            });
        }
        Effect::Verify(output) => {
            let Some(project) = model.project.clone() else {
                return Err("verification requires a project".into());
            };
            submit(worker, model, "verify", move |control| {
                IntegrationService::new(project, ProviderService::from_environment()?)
                    .verify(&output, &control)?;
                Ok(format!(
                    "independent verification passed for {}",
                    output.display()
                ))
            });
        }
    }
    Ok(false)
}

fn submit<F>(worker: &Worker, model: &mut Model, operation: &'static str, job: F)
where
    F: FnOnce(okc_app::OperationControl) -> okc_app::Result<String> + Send + 'static,
{
    if worker.submit(operation, job) {
        model.busy = true;
        model.status = format!("[ ] {operation} is running");
    } else {
        model.status = "[!] Another mutation is already running".into();
    }
}

fn open_project(model: &mut Model, path: &Path) -> Result<(), String> {
    let project = ProjectStore::open(path).map_err(|error| error.to_string())?;
    model.project = Some(project.root().to_path_buf());
    for source in &project.manifest().sources {
        if !model
            .vaults
            .iter()
            .any(|choice| choice.candidate.path == source.path)
        {
            let kind = if source.path.is_dir() {
                VaultCandidateKind::Directory
            } else if source
                .path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
            {
                VaultCandidateKind::Zip
            } else {
                VaultCandidateKind::TarZst
            };
            model.vaults.push(VaultChoice {
                candidate: VaultCandidate {
                    path: source.path.clone(),
                    kind,
                    suggested_source_id: source.source_id.to_string(),
                },
                source_id: source.source_id.to_string(),
                selected: true,
            });
        }
    }
    model
        .vaults
        .sort_by(|left, right| left.candidate.path.cmp(&right.candidate.path));
    for choice in &mut model.vaults {
        choice.selected = project
            .manifest()
            .sources
            .iter()
            .any(|source| source.path == choice.candidate.path);
        if let Some(source) = project
            .manifest()
            .sources
            .iter()
            .find(|source| source.path == choice.candidate.path)
        {
            choice.source_id = source.source_id.to_string();
        }
    }
    model.status = format!(
        "[OK] Opened {}",
        visible_text(&project.manifest().name, false)
    );
    refresh_project_state(model);
    Ok(())
}

fn refresh_project_state(model: &mut Model) {
    let Some(path) = model.project.clone() else {
        return;
    };
    let Ok(provider) = ProviderService::from_environment() else {
        return;
    };
    model.remote_routes = ProjectStore::open(&path)
        .ok()
        .and_then(|store| {
            provider.load_config().ok().map(|config| {
                [
                    AiRole::Embedding,
                    AiRole::Organizer,
                    AiRole::Synthesis,
                    AiRole::Critic,
                ]
                .into_iter()
                .any(|role| {
                    config
                        .profiles
                        .get(store.manifest().ai_routes.profile_for(role))
                        .is_some_and(|profile| profile.data_boundary() == DataBoundary::Remote)
                })
            })
        })
        .unwrap_or(false);
    let service = IntegrationService::new(&path, provider);
    match service.checkpoint() {
        Ok(IntegrationCheckpoint::NeedsProvider) => model.screen = Screen::AiConnection,
        Ok(IntegrationCheckpoint::NeedsSources) => model.screen = Screen::Vaults,
        Ok(IntegrationCheckpoint::NeedsDisclosure) => model.screen = Screen::Preflight,
        Ok(IntegrationCheckpoint::NeedsTaxonomy) => model.screen = Screen::Taxonomy,
        Ok(IntegrationCheckpoint::NeedsClusters) => model.screen = Screen::Clusters,
        Ok(IntegrationCheckpoint::ReadyToCompile) => model.screen = Screen::Build,
        Ok(IntegrationCheckpoint::Verified) => model.screen = Screen::Verify,
        Err(error) => {
            model.status = format!("[ERROR] {error}");
            return;
        }
    }
    if let Ok(taxonomy) = service.latest_taxonomy()
        && model
            .taxonomy
            .as_ref()
            .map(|old| old.taxonomy.taxonomy_hash)
            != Some(taxonomy.taxonomy.taxonomy_hash)
    {
        model
            .taxonomy_clusters
            .clone_from(&taxonomy.taxonomy.clusters);
        model.taxonomy = Some(taxonomy);
        model.taxonomy_edited = false;
        model.taxonomy_rationale.clear();
    }
    if let Ok(clusters) = service.completed_clusters()
        && model.clusters != clusters
    {
        model.clusters = clusters;
        reset_cluster_review(model);
    }
    if let Ok(store) = ProjectStore::open(&path) {
        if let Ok(Some(output)) = store.latest_verified_output() {
            model.output = Some(output);
        } else if let Ok(bootstrap) = WorkspaceBootstrap::new(&model.cwd) {
            model.output = bootstrap
                .suggested_output_path(&store.manifest().sources)
                .ok();
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "bounded layout and modal layering stay together"
)]
fn render(frame: &mut ratatui::Frame<'_>, model: &Model) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(
            Paragraph::new(format!(
                "[!] Terminal is {}x{}; OKC requires at least {MIN_WIDTH}x{MIN_HEIGHT}.",
                area.width, area.height
            ))
            .block(Block::default().borders(Borders::ALL).title("OKC"))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(23), Constraint::Min(40)])
        .split(area);
    let accent = if model.high_contrast {
        Color::White
    } else {
        Color::Cyan
    };
    let sidebar = Screen::ALL
        .iter()
        .map(|screen| {
            ListItem::new(format!(
                "{} {}",
                if *screen == model.screen { ">" } else { " " },
                screen.label(model.language)
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(sidebar).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" OKC 0.3 ", Style::default().fg(accent))),
        ),
        columns[0],
    );
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(columns[1]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            model.screen.label(model.language),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )))
        .block(Block::default().borders(Borders::ALL)),
        rows[0],
    );
    if model.screen == Screen::Clusters {
        render_cluster_panes(frame, rows[1], model);
    } else {
        frame.render_widget(
            Paragraph::new(screen_body(model))
                .block(Block::default().borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            rows[1],
        );
    }
    let status = if let Some(progress) = &model.progress {
        format!(
            "{:?} · {:?} · {}/{}{}",
            progress.operation,
            progress.phase,
            progress.completed,
            progress
                .total
                .map_or_else(|| "?".into(), |value| value.to_string()),
            progress
                .current_item
                .as_ref()
                .map_or_else(String::new, |item| format!(" · {item}"))
        )
    } else if model.help_visible {
        "Tab/Shift+Tab navigate · Enter act · ? help · Esc quit/cancel".into()
    } else {
        model.status.clone()
    };
    frame.render_widget(
        Paragraph::new(visible_text(&status, model.ascii_mode))
            .block(Block::default().borders(Borders::ALL).title(" Status ")),
        rows[2],
    );
    if model.cancel_confirm {
        render_modal(
            frame,
            area,
            "Cancel operation?",
            "Y request cancellation · N continue",
        );
    }
    if model.consent_confirm {
        render_modal(
            frame,
            area,
            "Remote disclosure",
            "One-time consent for the next uncached remote requests? Y/N",
        );
    }
}

fn screen_body(model: &Model) -> String {
    let value = match model.screen {
        Screen::Workspace => {
            if model.projects.is_empty() { format!("Current folder\n{}\n\nNo project found. Enter continues to AI setup.", model.cwd.display()) }
            else { format!("Current folder\n{}\n\n{}\n\n↑/↓ select · Enter open", model.cwd.display(), model.projects.iter().enumerate().map(|(index, path)| format!("{} {}", if index == model.cursor { ">" } else { " " }, path.display())).collect::<Vec<_>>().join("\n")) }
        }
        Screen::AiConnection => format!(
            "{}\n\n{} Provider: {:?}\n{} Profile: {}\n{} Endpoint: {}\n{} Model ID: {}\n{} Credential: {:?}\n{} Secret/ref: {}\n\n↑/↓ fields · ←/→ provider · Space credential mode · Enter test & save{}",
            if model.ai.embedding_setup { "Anthropic needs a separate embedding profile." } else { "A synthetic capability test must pass before this connection is usable." },
            field_marker(model.ai.field, AiField::Kind), model.ai.kind,
            field_marker(model.ai.field, AiField::Profile), model.ai.profile,
            field_marker(model.ai.field, AiField::Endpoint), model.ai.endpoint,
            field_marker(model.ai.field, AiField::Model), model.ai.model,
            field_marker(model.ai.field, AiField::CredentialMode), model.ai.credential_mode,
            field_marker(model.ai.field, AiField::Credential), if model.ai.credential_mode == CredentialMode::Keychain { model.ai.secret.masked() } else { &model.ai.credential_ref },
            if model.keychain_status == CredentialStoreStatus::Available { "" } else { "\nOS keychain unavailable/locked: environment mode or cancel only; no plaintext fallback." }
        ),
        Screen::Vaults => {
            let list = model.vaults.iter().enumerate().map(|(index, choice)| format!("{} [{}] {:<18} {}", if index == model.cursor { ">" } else { " " }, if choice.selected { "x" } else { " " }, choice.source_id, choice.candidate.path.display())).collect::<Vec<_>>().join("\n");
            format!("Active source set (maximum 10)\n\n{list}\n\nSpace select · E edit source ID · M manual path · Enter replace active set\nSymlinks, managed outputs, and ancestor/descendant combinations are rejected.{}", model.manual_path.as_ref().map_or(String::new(), |path| format!("\n\nManual path: {path}_")))
        }
        Screen::Preflight => model.preflight.as_ref().map_or_else(
            || "Press Enter to inspect immutable sources and run the local sensitive-data scan. No provider is called during this step.".into(),
            |summary| format!("Run {}\nDocuments: {} · blocks: {} · bytes: {}\nEstimated tokens: {}–{} · requests: {}–{}\nSensitive findings: {}\n\n{}\n\nEnter continues. A remote route opens a one-time disclosure confirmation before uncached transmission.", summary.run_id, summary.documents, summary.blocks, summary.input_bytes, summary.estimated_tokens_min, summary.estimated_tokens_max, summary.estimated_requests_min, summary.estimated_requests_max, summary.sensitive_findings, summary.routes.iter().map(|route| format!("{:?}: {} ({:?})", route.role, route.profile_name, route.boundary)).collect::<Vec<_>>().join("\n"))
        ),
        Screen::Taxonomy => {
            let clusters = model.taxonomy_clusters.iter().enumerate().map(|(index, cluster)| format!("{} {} → knowledge/{} ({} docs)", if index == model.taxonomy_cursor { ">" } else { " " }, cluster.title, cluster.canonical_path, cluster.document_ids.len())).collect::<Vec<_>>().join("\n");
            format!("{clusters}\n\nE title · P canonical path · ←/→ move document · M merge · S split\nX rationale · A/Enter explicitly approve\nRationale: {}", model.taxonomy_rationale)
        }
        Screen::Build => format!("Approved integration plan is sealed and provider-free.\n\nDestination: {}\n\nE edits the destination. Enter confirms it, compiles to a new path, and immediately runs independent verification. Existing targets are never overwritten; publication is non-cancellable after the atomic barrier.{}", model.output.as_ref().map_or("<unavailable>".into(), |path| path.display().to_string()), model.output_edit.as_ref().map_or(String::new(), |value| format!("\n\nEditing: {value}_"))),
        Screen::Verify => format!("Success is reported only after independent verification.\n\nArtifact: {}\n\nEnter verifies again without contacting an AI provider.", model.output.as_ref().map_or("<none>".into(), |path| path.display().to_string())),
        Screen::Provenance => format!("Project: {}\n\nGenerated artifacts retain source snapshot, document, block, and content-hash derivations. Provider recordings and every approval revision are stored as immutable project objects.", model.project.as_ref().map_or("<none>".into(), |path| path.display().to_string())),
        Screen::Settings => "L language · A ASCII-safe display · H high contrast\n\nProvider profiles store only environment-variable names or OS-keychain account references. Secret values are never rendered, logged, or written to project/TOML files.".into(),
        Screen::Clusters => unreachable!("rendered as three panes"),
    };
    visible_text(&value, model.ascii_mode)
}

#[allow(
    clippy::too_many_lines,
    reason = "the three coordinated review panes share one selected revision"
)]
fn render_cluster_panes(frame: &mut ratatui::Frame<'_>, area: Rect, model: &Model) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(32),
            Constraint::Percentage(36),
            Constraint::Percentage(32),
        ])
        .split(area);
    let Some(cluster) = model.clusters.get(model.cluster_cursor) else {
        frame.render_widget(
            Paragraph::new("No complete synthesis/critic revision yet.")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };
    let evidence = cluster
        .proposal
        .dispositions
        .iter()
        .fold(String::new(), |mut output, item| {
            writeln!(output, "{:?}\n{}", item.disposition, item.target.target_id)
                .expect("String write");
            output
        });
    let synthesis = cluster
        .proposal
        .sections
        .iter()
        .fold(String::new(), |mut output, section| {
            writeln!(output, "# {}\n{}", section.heading, section.markdown_body)
                .expect("String write");
            output
        });
    let issues = cluster_issue_keys(model);
    let critic = format!(
        "revision {}\n{}\n\nIssues\n{}\n\n[ ]/Space acknowledge each\nW rationale · R feedback · G regenerate · A approve",
        cluster.proposal.revision,
        cluster
            .critic
            .findings
            .iter()
            .map(|finding| format!(
                "{:?} {}: {}",
                finding.severity, finding.finding_id, finding.message
            ))
            .collect::<Vec<_>>()
            .join("\n"),
        issues
            .iter()
            .enumerate()
            .map(|(index, key)| format!(
                "{} [{}{}] {key}",
                if index == model.issue_cursor {
                    ">"
                } else {
                    " "
                },
                if model.issue_acknowledged.contains(key) {
                    "x"
                } else {
                    " "
                },
                if model
                    .issue_rationales
                    .get(key)
                    .is_some_and(|rationale| !rationale.trim().is_empty())
                {
                    "r"
                } else {
                    " "
                }
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    frame.render_widget(
        Paragraph::new(visible_text(&evidence, model.ascii_mode))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Source / Evidence "),
            )
            .wrap(Wrap { trim: false }),
        panes[0],
    );
    frame.render_widget(
        Paragraph::new(visible_text(&synthesis, model.ascii_mode))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Synthesis · {} ", cluster.proposal.cluster_id)),
            )
            .wrap(Wrap { trim: false }),
        panes[1],
    );
    frame.render_widget(
        Paragraph::new(visible_text(&critic, model.ascii_mode))
            .block(Block::default().borders(Borders::ALL).title(" Critic "))
            .wrap(Wrap { trim: false }),
        panes[2],
    );
}

fn render_modal(frame: &mut ratatui::Frame<'_>, area: Rect, title: &str, message: &str) {
    let width = area.width.saturating_sub(16).min(64);
    let height = 7.min(area.height);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn field_marker(current: AiField, field: AiField) -> &'static str {
    if current == field { ">" } else { " " }
}

pub fn visible_text(value: &str, ascii_mode: bool) -> String {
    let mut output = String::new();
    for character in value.chars() {
        let unsafe_bidi = matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
        if character.is_control() || unsafe_bidi || character == '\u{007f}' {
            write!(&mut output, "\\u{{{:04X}}}", u32::from(character)).expect("String write");
        } else if ascii_mode && !character.is_ascii() {
            output.push('?');
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode) -> AppEvent {
        AppEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn reducer_has_navigation_and_cancel_confirmation() {
        let mut model = Model::default();
        model.projects.push(PathBuf::from("fixture.okc-project"));
        let (model, _) = reduce(model, key(KeyCode::Right));
        assert_eq!(model.screen, Screen::AiConnection);
        let mut busy = model;
        busy.busy = true;
        let (busy, effects) = reduce(busy, key(KeyCode::Esc));
        assert!(busy.cancel_confirm);
        assert!(effects.is_empty());
        let (_, effects) = reduce(busy, key(KeyCode::Char('y')));
        assert_eq!(effects, vec![Effect::CancelOperation]);
    }

    #[test]
    fn secret_mask_is_fixed_width_and_debug_redacted() {
        let short = SecretInput::new("x".into());
        let long = SecretInput::new("a much longer token".into());
        assert_eq!(short.masked(), long.masked());
        assert!(!format!("{long:?}").contains("token"));
    }

    #[test]
    fn rendering_is_bounded_and_hostile_controls_are_visible() {
        assert_eq!(
            visible_text("a\u{001b}]52;x\u{202e}b", false),
            "a\\u{001B}]52;x\\u{202E}b"
        );
        let backend = TestBackend::new(MIN_WIDTH, MIN_HEIGHT);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, &Model::default()))
            .expect("render");
        assert!(format!("{:?}", terminal.backend().buffer()).contains("OKC"));
    }
}
