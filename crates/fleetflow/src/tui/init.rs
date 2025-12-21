//! 設定ファイル初期化ウィザード

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::io;

use super::terminal::{restore_terminal, setup_terminal};

/// 初期化ウィザードの状態
#[derive(Debug, Clone)]
struct InitWizardState {
    step: WizardStep,
    selected_template: usize,
    templates: Vec<Template>,
    config_path: ConfigPath,
}

/// ウィザードのステップ
#[derive(Debug, Clone, PartialEq)]
enum WizardStep {
    Welcome,
    SelectTemplate,
    SelectPath,
    Confirm,
}

/// 設定ファイルのテンプレート
#[derive(Debug, Clone)]
struct Template {
    name: String,
    description: String,
    content: String,
}

/// 設定ファイルの保存先
#[derive(Debug, Clone, PartialEq)]
enum ConfigPath {
    CurrentDir,   // ./flow.kdl
    FlowDir,      // ./.fleetflow/flow.kdl
    GlobalConfig, // ~/.config/flow/flow.kdl
}

impl InitWizardState {
    fn new() -> Self {
        Self {
            step: WizardStep::Welcome,
            selected_template: 0,
            templates: vec![
                Template {
                    name: "PostgreSQL".to_string(),
                    description: "PostgreSQL データベースのみのシンプルな構成".to_string(),
                    content: include_str!("../../resources/templates/simple.kdl").to_string(),
                },
                Template {
                    name: "Full Stack".to_string(),
                    description: "PostgreSQL + Redis + Webアプリの完全な構成".to_string(),
                    content: include_str!("../../resources/templates/fullstack.kdl").to_string(),
                },
                Template {
                    name: "空の設定".to_string(),
                    description: "コメントのみの空の設定ファイル".to_string(),
                    content: "// Flow 設定ファイル\n\n".to_string(),
                },
            ],
            config_path: ConfigPath::CurrentDir,
        }
    }

    fn next_step(&mut self) {
        self.step = match self.step {
            WizardStep::Welcome => WizardStep::SelectTemplate,
            WizardStep::SelectTemplate => WizardStep::SelectPath,
            WizardStep::SelectPath => WizardStep::Confirm,
            WizardStep::Confirm => WizardStep::Confirm,
        };
    }

    fn previous_step(&mut self) {
        self.step = match self.step {
            WizardStep::Welcome => WizardStep::Welcome,
            WizardStep::SelectTemplate => WizardStep::Welcome,
            WizardStep::SelectPath => WizardStep::SelectTemplate,
            WizardStep::Confirm => WizardStep::SelectPath,
        };
    }

    fn select_next_template(&mut self) {
        if self.selected_template < self.templates.len() - 1 {
            self.selected_template += 1;
        }
    }

    fn select_previous_template(&mut self) {
        if self.selected_template > 0 {
            self.selected_template -= 1;
        }
    }

    fn cycle_config_path(&mut self) {
        self.config_path = match self.config_path {
            ConfigPath::CurrentDir => ConfigPath::FlowDir,
            ConfigPath::FlowDir => ConfigPath::GlobalConfig,
            ConfigPath::GlobalConfig => ConfigPath::CurrentDir,
        };
    }

    fn get_config_path_string(&self) -> &str {
        match self.config_path {
            ConfigPath::CurrentDir => "./flow.kdl",
            ConfigPath::FlowDir => "./.fleetflow/flow.kdl",
            ConfigPath::GlobalConfig => "~/.config/fleetflow/flow.kdl",
        }
    }
}

/// 初期化ウィザードを実行
pub fn run_init_wizard() -> io::Result<Option<(String, String)>> {
    let mut terminal = setup_terminal()?;
    let mut state = InitWizardState::new();

    let result = loop {
        terminal.draw(|f| draw_ui(f, &state))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    break None;
                }
                KeyCode::Enter => {
                    if state.step == WizardStep::Confirm {
                        let template = &state.templates[state.selected_template];
                        let path = state.get_config_path_string().to_string();
                        break Some((path, template.content.clone()));
                    } else {
                        state.next_step();
                    }
                }
                KeyCode::Backspace => {
                    state.previous_step();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if state.step == WizardStep::SelectTemplate {
                        state.select_next_template();
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if state.step == WizardStep::SelectTemplate {
                        state.select_previous_template();
                    }
                }
                KeyCode::Tab => {
                    if state.step == WizardStep::SelectPath {
                        state.cycle_config_path();
                    }
                }
                _ => {}
            }
        }
    };

    restore_terminal(&mut terminal)?;
    Ok(result)
}

fn draw_ui(frame: &mut Frame, state: &InitWizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_header(frame, chunks[0]);
    draw_content(frame, chunks[1], state);
    draw_footer(frame, chunks[2], state);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new("Flow 初期化ウィザード")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, area);
}

fn draw_content(frame: &mut Frame, area: Rect, state: &InitWizardState) {
    match state.step {
        WizardStep::Welcome => draw_welcome(frame, area),
        WizardStep::SelectTemplate => draw_template_selection(frame, area, state),
        WizardStep::SelectPath => draw_path_selection(frame, area, state),
        WizardStep::Confirm => draw_confirmation(frame, area, state),
    }
}

fn draw_welcome(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Flowへようこそ！",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("設定ファイルが見つかりませんでした。"),
        Line::from("新しい設定ファイルを作成しましょう。"),
        Line::from(""),
        Line::from(Span::styled(
            "Enterキーを押して続ける",
            Style::default().fg(Color::Yellow),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_template_selection(frame: &mut Frame, area: Rect, state: &InitWizardState) {
    let items: Vec<ListItem> = state
        .templates
        .iter()
        .enumerate()
        .map(|(i, template)| {
            let style = if i == state.selected_template {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let content = vec![
                Line::from(Span::styled(&template.name, style)),
                Line::from(Span::styled(
                    format!("  {}", template.description),
                    Style::default().fg(Color::Gray),
                )),
            ];
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title("テンプレートを選択 (↑↓で移動)")
            .borders(Borders::ALL),
    );

    frame.render_widget(list, area);
}

fn draw_path_selection(frame: &mut Frame, area: Rect, state: &InitWizardState) {
    let paths = [
        ("./flow.kdl", "カレントディレクトリ (推奨)"),
        ("./.fleetflow/flow.kdl", ".fleetflowディレクトリ内"),
        ("~/.config/fleetflow/flow.kdl", "グローバル設定"),
    ];

    let items: Vec<ListItem> = paths
        .iter()
        .map(|(path, desc)| {
            let is_selected = *path == state.get_config_path_string();
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let content = vec![
                Line::from(Span::styled(*path, style)),
                Line::from(Span::styled(
                    format!("  {}", desc),
                    Style::default().fg(Color::Gray),
                )),
            ];
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title("保存先を選択 (Tabで切り替え)")
            .borders(Borders::ALL),
    );

    frame.render_widget(list, area);
}

fn draw_confirmation(frame: &mut Frame, area: Rect, state: &InitWizardState) {
    let template = &state.templates[state.selected_template];
    let path = state.get_config_path_string();

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "以下の設定で作成します：",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("テンプレート: "),
            Span::styled(&template.name, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::raw("保存先: "),
            Span::styled(path, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enterで確定、Backspaceで戻る",
            Style::default().fg(Color::Yellow),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("確認"));
    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &InitWizardState) {
    let help_text = match state.step {
        WizardStep::Welcome => "Enter: 次へ | q/Esc: 終了",
        WizardStep::SelectTemplate => "↑↓/jk: 選択 | Enter: 次へ | Backspace: 戻る | q/Esc: 終了",
        WizardStep::SelectPath => "Tab: 切り替え | Enter: 次へ | Backspace: 戻る | q/Esc: 終了",
        WizardStep::Confirm => "Enter: 確定 | Backspace: 戻る | q/Esc: 終了",
    };

    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, area);
}
