use std::fmt::Write as _;
use std::io::{self, IsTerminal as _, Stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use okc_app::ProjectStore;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Project,
    Dashboard,
    SourcesPolicy,
    Inspect,
    Plan,
    ConflictReview,
    Augmentation,
    Preflight,
    Compile,
    Verify,
    Provenance,
    SettingsHelp,
}

impl Screen {
    const ALL: [Self; 12] = [
        Self::Project,
        Self::Dashboard,
        Self::SourcesPolicy,
        Self::Inspect,
        Self::Plan,
        Self::ConflictReview,
        Self::Augmentation,
        Self::Preflight,
        Self::Compile,
        Self::Verify,
        Self::Provenance,
        Self::SettingsHelp,
    ];

    fn label(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::Project, Language::English) => "Project",
            (Self::Dashboard, Language::English) => "Dashboard",
            (Self::SourcesPolicy, Language::English) => "Sources & Policy",
            (Self::Inspect, Language::English) => "Inspect",
            (Self::Plan, Language::English) => "Plan",
            (Self::ConflictReview, Language::English) => "Conflict Review",
            (Self::Augmentation, Language::English) => "AI Proposals",
            (Self::Preflight, Language::English) => "Build Preflight",
            (Self::Compile, Language::English) => "Compile & Pack",
            (Self::Verify, Language::English) => "Independent Verify",
            (Self::Provenance, Language::English) => "Provenance",
            (Self::SettingsHelp, Language::English) => "Settings & Help",
            (Self::Project, Language::Korean) => "프로젝트",
            (Self::Dashboard, Language::Korean) => "대시보드",
            (Self::SourcesPolicy, Language::Korean) => "소스 및 정책",
            (Self::Inspect, Language::Korean) => "검사",
            (Self::Plan, Language::Korean) => "계획",
            (Self::ConflictReview, Language::Korean) => "충돌 검토",
            (Self::Augmentation, Language::Korean) => "AI 제안",
            (Self::Preflight, Language::Korean) => "빌드 사전 점검",
            (Self::Compile, Language::Korean) => "컴파일 및 팩",
            (Self::Verify, Language::Korean) => "독립 검증",
            (Self::Provenance, Language::Korean) => "출처 추적",
            (Self::SettingsHelp, Language::Korean) => "설정 및 도움말",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Korean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "orthogonal accessibility, search, and help toggles are explicit reducer state"
)]
pub struct Model {
    pub screen: Screen,
    pub language: Language,
    pub ascii_mode: bool,
    pub high_contrast: bool,
    pub search_active: bool,
    pub help_visible: bool,
    pub width: u16,
    pub height: u16,
    pub project: Option<PathBuf>,
    pub status: String,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            screen: Screen::Project,
            language: Language::English,
            ascii_mode: false,
            high_contrast: false,
            search_active: false,
            help_visible: false,
            width: MIN_WIDTH,
            height: MIN_HEIGHT,
            project: None,
            status: "[ ] No project open".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    WorkerFinished { operation: String, success: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Quit,
    OpenProject(PathBuf),
    StartOperation(&'static str),
}

pub fn reduce(mut model: Model, event: AppEvent) -> (Model, Vec<Effect>) {
    let mut effects = Vec::new();
    match event {
        AppEvent::Resize(width, height) => {
            model.width = width;
            model.height = height;
        }
        AppEvent::WorkerFinished { operation, success } => {
            model.status = format!("{} {operation}", if success { "[OK]" } else { "[ERROR]" });
        }
        AppEvent::Key(key) => match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                effects.push(Effect::Quit);
            }
            KeyCode::Char('?') => model.help_visible = !model.help_visible,
            KeyCode::Char('/') => model.search_active = true,
            KeyCode::Esc if model.search_active => model.search_active = false,
            KeyCode::Esc if model.help_visible => model.help_visible = false,
            KeyCode::Esc => effects.push(Effect::Quit),
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
                model.screen = adjacent_screen(model.screen, 1);
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
                model.screen = adjacent_screen(model.screen, -1);
            }
            KeyCode::Char('l') if model.screen == Screen::SettingsHelp => {
                model.language = match model.language {
                    Language::English => Language::Korean,
                    Language::Korean => Language::English,
                };
            }
            KeyCode::Char('a') if model.screen == Screen::SettingsHelp => {
                model.ascii_mode = !model.ascii_mode;
            }
            KeyCode::Char('h') if model.screen == Screen::SettingsHelp => {
                model.high_contrast = !model.high_contrast;
            }
            KeyCode::Enter => match model.screen {
                Screen::Project => {
                    if let Some(path) = &model.project {
                        effects.push(Effect::OpenProject(path.clone()));
                    } else {
                        model.status = "[ ] Pass --project PATH to open a project".into();
                    }
                }
                Screen::Inspect => effects.push(Effect::StartOperation("inspect")),
                Screen::Plan => effects.push(Effect::StartOperation("plan")),
                Screen::Compile => effects.push(Effect::StartOperation("compile")),
                Screen::Verify => effects.push(Effect::StartOperation("verify")),
                _ => model.status = "[ ] Select an item or continue with Tab".into(),
            },
            _ => {}
        },
    }
    (model, effects)
}

fn adjacent_screen(screen: Screen, delta: i8) -> Screen {
    let current = Screen::ALL
        .iter()
        .position(|candidate| *candidate == screen)
        .unwrap_or(0);
    let len = i32::try_from(Screen::ALL.len()).expect("screen count fits i32");
    let current = i32::try_from(current).expect("screen index fits i32");
    let next = usize::try_from((current + i32::from(delta)).rem_euclid(len))
        .expect("wrapped screen index is non-negative");
    Screen::ALL[next]
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
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
    let mut model = Model::default();
    if let Some(path) = project {
        let project = ProjectStore::open(path).map_err(|error| error.to_string())?;
        model.project = Some(project.root().to_path_buf());
        model.screen = Screen::Dashboard;
        model.status = format!(
            "[OK] Opened {}",
            visible_text(project.manifest().name.as_str(), false)
        );
        if let Some(warning) = project.privacy_warning() {
            model.status = format!("[!] {warning}");
        }
    }

    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        old_hook(info);
    }));
    let mut terminal = TerminalGuard::enter().map_err(|error| error.to_string())?;
    loop {
        terminal
            .terminal
            .draw(|frame| render(frame, &model))
            .map_err(|error| error.to_string())?;
        if event::poll(Duration::from_millis(100)).map_err(|error| error.to_string())? {
            let app_event = match event::read().map_err(|error| error.to_string())? {
                CrosstermEvent::Key(key) => Some(AppEvent::Key(key)),
                CrosstermEvent::Resize(width, height) => Some(AppEvent::Resize(width, height)),
                _ => None,
            };
            if let Some(app_event) = app_event {
                let (next, effects) = reduce(model, app_event);
                model = next;
                for effect in effects {
                    match effect {
                        Effect::Quit => return Ok(()),
                        Effect::OpenProject(path) => {
                            model.status = format!("[ ] Open requested: {}", path.display());
                        }
                        Effect::StartOperation(operation) => {
                            let (next, _) = reduce(
                                model,
                                AppEvent::WorkerFinished {
                                    operation: operation.into(),
                                    success: false,
                                },
                            );
                            model = next;
                        }
                    }
                }
            }
        }
    }
}

fn render(frame: &mut ratatui::Frame<'_>, model: &Model) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let warning = format!(
            "[!] Terminal is {}x{}; OKC requires at least {MIN_WIDTH}x{MIN_HEIGHT}.",
            area.width, area.height
        );
        frame.render_widget(
            Paragraph::new(warning)
                .block(Block::default().borders(Borders::ALL).title("OKC"))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(40)])
        .split(area);
    let sidebar_items: Vec<_> = Screen::ALL
        .iter()
        .map(|screen| {
            let marker = if *screen == model.screen { ">" } else { " " };
            ListItem::new(format!("{marker} {}", screen.label(model.language)))
        })
        .collect();
    let accent = if model.high_contrast {
        Color::White
    } else {
        Color::Cyan
    };
    frame.render_widget(
        List::new(sidebar_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" OKC 0.2 ", Style::default().fg(accent))),
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
    let title = model.screen.label(model.language);
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            title,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )]))
        .block(Block::default().borders(Borders::ALL)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(screen_body(model))
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        rows[1],
    );
    let help = if model.search_active {
        "/ Search: _   Esc cancel"
    } else if model.help_visible {
        "Tab/Shift+Tab navigate | arrows move | Enter select | / search | Esc quit"
    } else {
        &model.status
    };
    frame.render_widget(
        Paragraph::new(visible_text(help, model.ascii_mode))
            .block(Block::default().borders(Borders::ALL).title(" Status ")),
        rows[2],
    );
}

fn screen_body(model: &Model) -> String {
    let body = match model.screen {
        Screen::Project => {
            "Create or open a .okc-project. V1 import requires source rebinding and snapshot verification."
        }
        Screen::Dashboard => {
            "Project health\n[ ] Sources inspected\n[ ] Plan reviewed\n[ ] Independent verification required before success"
        }
        Screen::SourcesPolicy => {
            "Sources are immutable and MCP-origin-neutral. Configure stable source IDs and owner display names."
        }
        Screen::Inspect => "Inspect directories, ZIP, or tar.zst sources. Press Enter to start.",
        Screen::Plan => {
            "Review exact duplicates, near-duplicate candidates, diagnostics, and typed conflicts."
        }
        Screen::ConflictReview => {
            "LINK_AMBIGUITY requires one sealed target or preserve-original waiver. No arbitrary replacement text is accepted."
        }
        Screen::Augmentation => {
            "AI is optional. Select documents explicitly; remote disclosure requires fresh consent. Approvals are individual."
        }
        Screen::Preflight => {
            "Preflight checks decisions, resource estimate, destinations, and source immutability."
        }
        Screen::Compile => {
            "Compile and create an optional deterministic .okcpack. Publication becomes non-cancellable at the atomic barrier."
        }
        Screen::Verify => "Build success is shown only after independent verification passes.",
        Screen::Provenance => {
            "Browse output -> operation -> source/decision/proposal/approval derivation records."
        }
        Screen::SettingsHelp => {
            "L toggle English/Korean | A toggle ASCII mode | H toggle high contrast | ? help"
        }
    };
    visible_text(body, model.ascii_mode)
}

pub fn visible_text(value: &str, ascii_mode: bool) -> String {
    let mut output = String::new();
    for character in value.chars() {
        let unsafe_bidi = matches!(
            character,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        );
        if character.is_control() || unsafe_bidi || character == '\u{007f}' {
            write!(&mut output, "\\u{{{:04X}}}", u32::from(character))
                .expect("writing to a String cannot fail");
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
    fn reducer_supports_keyboard_navigation_settings_and_quit() {
        let model = Model::default();
        let (model, effects) = reduce(model, key(KeyCode::Tab));
        assert_eq!(model.screen, Screen::Dashboard);
        assert!(effects.is_empty());
        let mut settings = model;
        settings.screen = Screen::SettingsHelp;
        let (settings, _) = reduce(settings, key(KeyCode::Char('l')));
        assert_eq!(settings.language, Language::Korean);
        let (_, effects) = reduce(settings, key(KeyCode::Esc));
        assert_eq!(effects, vec![Effect::Quit]);
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
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("OKC"));
    }
}
