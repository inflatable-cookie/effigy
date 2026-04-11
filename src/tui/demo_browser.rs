use std::fs;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::runner::{run_command, RunnerError};
use crate::{Command, DemoArgs, DemoListGroupBy, DemoListQuery, DemoSubcommand};

type BrowserTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn run_demo_browser_tui(
    repo_root: PathBuf,
    initial_group_by: Option<DemoListGroupBy>,
) -> Result<(), RunnerError> {
    let mut terminal = init_browser_terminal()?;
    let mut app = DemoBrowserApp::new(repo_root, initial_group_by);
    let result = app.run(&mut terminal);
    restore_browser_terminal(&mut terminal)?;
    result
}

fn init_browser_terminal() -> Result<BrowserTerminal, RunnerError> {
    enable_raw_mode().map_err(|error| RunnerError::Ui(error.to_string()))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|error| RunnerError::Ui(error.to_string()))?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(|error| RunnerError::Ui(error.to_string()))
}

fn restore_browser_terminal(terminal: &mut BrowserTerminal) -> Result<(), RunnerError> {
    disable_raw_mode().map_err(|error| RunnerError::Ui(error.to_string()))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|error| RunnerError::Ui(error.to_string()))?;
    terminal
        .show_cursor()
        .map_err(|error| RunnerError::Ui(error.to_string()))
}

struct DemoBrowserApp {
    repo_root: PathBuf,
    group_by: Option<DemoListGroupBy>,
    rows: Vec<BrowserRow>,
    selected_demo_id: Option<String>,
    selected_row_index: usize,
    selected_artifact_index: usize,
    detail: Option<DemoDetail>,
    footer_message: String,
    pending_action: Option<PendingAction>,
    last_refresh: Instant,
}

impl DemoBrowserApp {
    fn new(repo_root: PathBuf, initial_group_by: Option<DemoListGroupBy>) -> Self {
        Self {
            repo_root,
            group_by: initial_group_by,
            rows: Vec::new(),
            selected_demo_id: None,
            selected_row_index: 0,
            selected_artifact_index: 0,
            detail: None,
            footer_message: "Loading demo registry...".to_owned(),
            pending_action: None,
            last_refresh: Instant::now() - Duration::from_secs(5),
        }
    }

    fn run(&mut self, terminal: &mut BrowserTerminal) -> Result<(), RunnerError> {
        self.refresh_state()?;
        loop {
            self.poll_pending_action();
            terminal
                .draw(|frame| self.render(frame))
                .map_err(|error| RunnerError::Ui(error.to_string()))?;

            if event::poll(Duration::from_millis(125))
                .map_err(|error| RunnerError::Ui(error.to_string()))?
            {
                if let Event::Key(key) =
                    event::read().map_err(|error| RunnerError::Ui(error.to_string()))?
                {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if self.handle_key(key.code)? {
                        break;
                    }
                }
            } else if self.last_refresh.elapsed() >= Duration::from_millis(750) {
                self.refresh_state()?;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode) -> Result<bool, RunnerError> {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
            KeyCode::Down | KeyCode::Char('j') => self.select_next_demo(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous_demo(),
            KeyCode::Char('g') => {
                self.group_by = next_group_by(self.group_by);
                self.refresh_state()?;
                self.footer_message = format!(
                    "Grouping demos by {}",
                    self.group_by.map_or("none", DemoListGroupBy::as_str)
                );
            }
            KeyCode::Char('R') => {
                self.refresh_state()?;
                self.footer_message = "Refreshed demo browser state.".to_owned();
            }
            KeyCode::Char('[') => self.select_previous_artifact(),
            KeyCode::Char(']') => self.select_next_artifact(),
            KeyCode::Char('o') => self.dispatch_open_artifact()?,
            KeyCode::Enter | KeyCode::Char('r') => self.dispatch_run_or_rerun()?,
            KeyCode::Char('s') => self.dispatch_stop()?,
            _ => {}
        }
        Ok(false)
    }

    fn selected_detail(&self) -> Option<&DemoDetail> {
        self.detail.as_ref()
    }

    fn selected_demo_id(&self) -> Option<&str> {
        self.selected_demo_id.as_deref()
    }

    fn select_next_demo(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let Some(mut index) = self.current_demo_row_index() else {
            return;
        };
        while index + 1 < self.rows.len() {
            index += 1;
            if let BrowserRow::Demo(summary) = &self.rows[index] {
                self.selected_demo_id = Some(summary.id.clone());
                self.selected_row_index = index;
                self.footer_message = format!("Selected demo `{}`.", summary.id);
                break;
            }
        }
    }

    fn select_previous_demo(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let Some(mut index) = self.current_demo_row_index() else {
            return;
        };
        while index > 0 {
            index -= 1;
            if let BrowserRow::Demo(summary) = &self.rows[index] {
                self.selected_demo_id = Some(summary.id.clone());
                self.selected_row_index = index;
                self.footer_message = format!("Selected demo `{}`.", summary.id);
                break;
            }
        }
    }

    fn current_demo_row_index(&self) -> Option<usize> {
        let selected = self.selected_demo_id()?;
        self.rows.iter().position(|row| match row {
            BrowserRow::Group(_) => false,
            BrowserRow::Demo(summary) => summary.id == selected,
        })
    }

    fn selected_artifact(&self) -> Option<&str> {
        let detail = self.selected_detail()?;
        selected_artifact(detail, self.selected_artifact_index)
    }

    fn select_next_artifact(&mut self) {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return;
        };
        if detail.latest_attempt.artifacts.is_empty() {
            self.footer_message = "The selected demo has no recorded artifacts.".to_owned();
            return;
        }
        self.selected_artifact_index =
            (self.selected_artifact_index + 1) % detail.latest_attempt.artifacts.len();
        if let Some(artifact) = self.selected_artifact() {
            self.footer_message = format!("Selected artifact `{artifact}`.");
        }
    }

    fn select_previous_artifact(&mut self) {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return;
        };
        if detail.latest_attempt.artifacts.is_empty() {
            self.footer_message = "The selected demo has no recorded artifacts.".to_owned();
            return;
        }
        if self.selected_artifact_index == 0 {
            self.selected_artifact_index = detail.latest_attempt.artifacts.len() - 1;
        } else {
            self.selected_artifact_index -= 1;
        }
        if let Some(artifact) = self.selected_artifact() {
            self.footer_message = format!("Selected artifact `{artifact}`.");
        }
    }

    fn dispatch_run_or_rerun(&mut self) -> Result<(), RunnerError> {
        if self.pending_action.is_some() {
            self.footer_message =
                "A demo run or rerun is already in flight. Stop or wait for it first.".to_owned();
            return Ok(());
        }
        let Some(detail) = self.selected_detail().cloned() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(());
        };

        let Some(subcommand) = preferred_run_action(&detail) else {
            self.footer_message =
                "The selected demo cannot be run or rerun in its current state.".to_owned();
            return Ok(());
        };

        let demo_id = detail.id.clone();
        let repo_root = self.repo_root.clone();
        let action_label = match &subcommand {
            DemoSubcommand::Run { .. } => "run",
            DemoSubcommand::Rerun { .. } => "rerun",
            _ => unreachable!(),
        };
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = invoke_demo_json(
                &repo_root,
                DemoArgs {
                    subcommand,
                    repo_override: Some(repo_root.clone()),
                    output_json: true,
                },
            );
            let _ = sender.send(result);
        });

        self.pending_action = Some(PendingAction {
            demo_id: demo_id.clone(),
            label: action_label.to_owned(),
            receiver,
        });
        self.footer_message =
            format!("Started `{action_label}` for demo `{demo_id}` in the background.");
        Ok(())
    }

    fn dispatch_stop(&mut self) -> Result<(), RunnerError> {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(());
        };
        let demo_id = detail.id.clone();
        if !detail.actions.stop.available {
            self.footer_message = detail
                .actions
                .stop
                .reason
                .clone()
                .unwrap_or_else(|| "The selected demo cannot be stopped right now.".to_owned());
            return Ok(());
        }

        let payload = invoke_demo_json(
            &self.repo_root,
            DemoArgs {
                subcommand: DemoSubcommand::Stop {
                    demo_id: demo_id.clone(),
                },
                repo_override: Some(self.repo_root.clone()),
                output_json: true,
            },
        )?;
        self.refresh_state()?;
        self.footer_message = payload_message(&payload)
            .unwrap_or_else(|| format!("Stop requested for demo `{demo_id}`."));
        Ok(())
    }

    fn dispatch_open_artifact(&mut self) -> Result<(), RunnerError> {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(());
        };
        let Some(artifact) = selected_artifact(detail, self.selected_artifact_index) else {
            self.footer_message = "The selected demo has no recorded artifacts to open.".to_owned();
            return Ok(());
        };
        let artifact_path = resolve_artifact_path(&self.repo_root, artifact);
        if !artifact_path.exists() {
            self.footer_message =
                format!("Artifact path is missing: `{}`.", artifact_path.display());
            return Ok(());
        }
        open_artifact_path(&artifact_path)?;
        self.footer_message = format!("Opened artifact `{}`.", artifact_path.display());
        Ok(())
    }

    fn poll_pending_action(&mut self) {
        let Some(pending) = self.pending_action.as_ref() else {
            return;
        };
        match pending.receiver.try_recv() {
            Ok(result) => {
                let label = pending.label.clone();
                let demo_id = pending.demo_id.clone();
                self.pending_action = None;
                match result {
                    Ok(payload) => {
                        let _ = self.refresh_state();
                        self.footer_message = payload_message(&payload).unwrap_or_else(|| {
                            format!(
                                "Demo `{demo_id}` {label} completed and browser state refreshed."
                            )
                        });
                    }
                    Err(error) => {
                        let _ = self.refresh_state();
                        self.footer_message = format!("Demo `{demo_id}` {label} failed: {error}");
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending_action = None;
                self.footer_message =
                    "The background demo action exited without returning a result.".to_owned();
            }
        }
    }

    fn refresh_state(&mut self) -> Result<(), RunnerError> {
        let payload = invoke_demo_json(
            &self.repo_root,
            DemoArgs {
                subcommand: DemoSubcommand::List {
                    query: DemoListQuery {
                        group_by: self.group_by,
                        ..DemoListQuery::default()
                    },
                },
                repo_override: Some(self.repo_root.clone()),
                output_json: true,
            },
        )?;
        let list_payload: DemoListPayload = serde_json::from_value(payload).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "failed to parse demo list payload for browser: {error}"
            ))
        })?;
        self.rows = rows_from_payload(&list_payload);

        let selected_id = self
            .selected_demo_id
            .clone()
            .filter(|demo_id| row_contains_demo(&self.rows, demo_id))
            .or_else(|| first_demo_id(&self.rows));
        self.selected_demo_id = selected_id.clone();
        self.selected_row_index = selected_id
            .as_ref()
            .and_then(|demo_id| {
                self.rows.iter().position(|row| match row {
                    BrowserRow::Group(_) => false,
                    BrowserRow::Demo(summary) => &summary.id == demo_id,
                })
            })
            .unwrap_or(0);

        self.detail = match selected_id {
            Some(demo_id) => {
                let payload = invoke_demo_json(
                    &self.repo_root,
                    DemoArgs {
                        subcommand: DemoSubcommand::Inspect { demo_id },
                        repo_override: Some(self.repo_root.clone()),
                        output_json: true,
                    },
                )?;
                let inspect_payload: DemoInspectPayload =
                    serde_json::from_value(payload).map_err(|error| {
                        RunnerError::TaskInvocation(format!(
                            "failed to parse demo inspect payload for browser: {error}"
                        ))
                    })?;
                Some(inspect_payload.demo)
            }
            None => None,
        };
        self.selected_artifact_index = self.detail.as_ref().map_or(0, |detail| {
            clamp_artifact_index(self.selected_artifact_index, detail)
        });

        self.last_refresh = Instant::now();
        Ok(())
    }

    fn render(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);
        self.render_header(frame, layout[0]);
        self.render_body(frame, layout[1]);
        self.render_footer(frame, layout[2]);
        if self.rows.is_empty() {
            self.render_empty_overlay(frame, area);
        }
    }

    fn render_header(&self, frame: &mut Frame<'_>, area: Rect) {
        let pending = self
            .pending_action
            .as_ref()
            .map_or("idle", |action| action.label.as_str());
        let text = Line::from(vec![
            Span::styled(
                " Demo Browser ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("group:", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                " {}",
                self.group_by.map_or("none", DemoListGroupBy::as_str)
            )),
            Span::raw("  "),
            Span::styled("pending:", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" {pending}")),
            Span::raw("  "),
            Span::styled("repo:", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" {}", self.repo_root.display())),
        ]);
        frame.render_widget(Paragraph::new(text), area);
    }

    fn render_body(&self, frame: &mut Frame<'_>, area: Rect) {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        self.render_list(frame, layout[0]);
        self.render_detail(frame, layout[1]);
    }

    fn render_list(&self, frame: &mut Frame<'_>, area: Rect) {
        let items = self
            .rows
            .iter()
            .map(|row| match row {
                BrowserRow::Group(label) => ListItem::new(Line::from(vec![Span::styled(
                    format!("  {label}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )])),
                BrowserRow::Demo(summary) => {
                    let status = summary.effective_status.as_str();
                    let line = Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            format!("{:<23}", summary.id),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(format!("{:<10}", status), status_style(status)),
                        Span::raw(" "),
                        Span::styled(
                            format!("[{}]", summary.action_summary()),
                            Style::default().fg(Color::Gray),
                        ),
                    ]);
                    ListItem::new(line)
                }
            })
            .collect::<Vec<_>>();

        let mut state = ListState::default();
        if !self.rows.is_empty() {
            state.select(Some(self.selected_row_index));
        }
        let list = List::new(items)
            .block(Block::default().title("Demos").borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">")
            .repeat_highlight_symbol(true);
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_detail(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines = if let Some(detail) = &self.detail {
            detail_lines(&self.repo_root, detail, self.selected_artifact_index)
        } else {
            vec![
                Line::from("No demo selected."),
                Line::from(""),
                Line::from("Use j/k or arrow keys to move through the demo list."),
            ]
        };
        let detail = Paragraph::new(lines)
            .block(Block::default().title("Detail").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(detail, area);
    }

    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let help = Line::from(vec![
            Span::styled(" j/k ", key_style()),
            Span::raw("move  "),
            Span::styled(" g ", key_style()),
            Span::raw("group  "),
            Span::styled(" r ", key_style()),
            Span::raw("run/rerun  "),
            Span::styled(" s ", key_style()),
            Span::raw("stop  "),
            Span::styled(" [ ] ", key_style()),
            Span::raw("artifact  "),
            Span::styled(" o ", key_style()),
            Span::raw("open  "),
            Span::styled(" R ", key_style()),
            Span::raw("refresh  "),
            Span::styled(" q ", key_style()),
            Span::raw("quit"),
        ]);
        let footer = Paragraph::new(vec![help, Line::from(self.footer_message.clone())])
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(footer, area);
    }

    fn render_empty_overlay(&self, frame: &mut Frame<'_>, area: Rect) {
        let overlay = centered_rect(60, 20, area);
        frame.render_widget(Clear, overlay);
        let notice = Paragraph::new(vec![
            Line::from("No demos are declared in the current manifest."),
            Line::from(""),
            Line::from("Add `[demos.<id>]` entries to `effigy.toml` first."),
        ])
        .block(Block::default().title("Demo Browser").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
        frame.render_widget(notice, overlay);
    }
}

fn invoke_demo_json(_repo_root: &Path, args: DemoArgs) -> Result<JsonValue, RunnerError> {
    let result = run_command(Command::Demo(args));
    let rendered = match result {
        Ok(rendered) => rendered,
        Err(RunnerError::CommandJsonFailure { rendered }) => rendered,
        Err(error) => return Err(error),
    };
    serde_json::from_str(&rendered).map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to parse demo json payload: {error}"))
    })
}

fn row_contains_demo(rows: &[BrowserRow], demo_id: &str) -> bool {
    rows.iter().any(|row| match row {
        BrowserRow::Group(_) => false,
        BrowserRow::Demo(summary) => summary.id == demo_id,
    })
}

fn first_demo_id(rows: &[BrowserRow]) -> Option<String> {
    rows.iter().find_map(|row| match row {
        BrowserRow::Group(_) => None,
        BrowserRow::Demo(summary) => Some(summary.id.clone()),
    })
}

fn rows_from_payload(payload: &DemoListPayload) -> Vec<BrowserRow> {
    if let Some(groups) = &payload.groups {
        let mut rows = Vec::new();
        for group in groups {
            rows.push(BrowserRow::Group(format!(
                "{} ({})",
                group.label, group.count
            )));
            for demo in &group.demos {
                rows.push(BrowserRow::Demo(demo.clone()));
            }
        }
        return rows;
    }

    payload
        .demos
        .iter()
        .cloned()
        .map(BrowserRow::Demo)
        .collect()
}

fn preferred_run_action(detail: &DemoDetail) -> Option<DemoSubcommand> {
    if detail.latest_attempt.recorded && detail.actions.rerun.available {
        return Some(DemoSubcommand::Rerun {
            demo_id: detail.id.clone(),
        });
    }
    if detail.actions.run.available {
        return Some(DemoSubcommand::Run {
            demo_id: detail.id.clone(),
        });
    }
    None
}

fn payload_message(payload: &JsonValue) -> Option<String> {
    payload
        .get("message")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .or_else(|| {
            payload
                .get("execution")
                .and_then(|execution| execution.get("summary"))
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
        })
        .or_else(|| {
            payload
                .get("message")
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
        })
}

fn status_style(status: &str) -> Style {
    match status {
        "running" | "running (stop-requested)" => Style::default().fg(Color::Yellow),
        "passed" => Style::default().fg(Color::Green),
        "failed" | "broken" => Style::default().fg(Color::Red),
        "missing" | "planned" => Style::default().fg(Color::Magenta),
        _ => Style::default().fg(Color::Cyan),
    }
}

fn key_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Gray)
        .add_modifier(Modifier::BOLD)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn next_group_by(current: Option<DemoListGroupBy>) -> Option<DemoListGroupBy> {
    match current {
        None => Some(DemoListGroupBy::Owner),
        Some(DemoListGroupBy::Owner) => Some(DemoListGroupBy::Status),
        Some(DemoListGroupBy::Status) => Some(DemoListGroupBy::Gap),
        Some(DemoListGroupBy::Gap) => None,
        Some(DemoListGroupBy::Tag) | Some(DemoListGroupBy::Mode) | Some(DemoListGroupBy::Cover) => {
            Some(DemoListGroupBy::Owner)
        }
    }
}

fn clamp_artifact_index(current: usize, detail: &DemoDetail) -> usize {
    if detail.latest_attempt.artifacts.is_empty() {
        0
    } else {
        current.min(detail.latest_attempt.artifacts.len() - 1)
    }
}

fn selected_artifact(detail: &DemoDetail, selected_index: usize) -> Option<&str> {
    detail
        .latest_attempt
        .artifacts
        .get(clamp_artifact_index(selected_index, detail))
        .map(String::as_str)
}

fn resolve_repo_relative_path(repo_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn resolve_artifact_path(repo_root: &Path, artifact: &str) -> PathBuf {
    resolve_repo_relative_path(repo_root, artifact)
}

fn open_artifact_path(path: &Path) -> Result<(), RunnerError> {
    let mut command = build_open_command(path);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let status = command.status().map_err(|error| {
        RunnerError::Ui(format!(
            "failed to launch artifact opener for `{}`: {error}",
            path.display()
        ))
    })?;
    if !status.success() {
        return Err(RunnerError::Ui(format!(
            "artifact opener exited unsuccessfully for `{}` with status {status}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn build_open_command(path: &Path) -> ProcessCommand {
    let mut command = ProcessCommand::new("open");
    command.arg(path);
    command
}

#[cfg(target_os = "windows")]
fn build_open_command(path: &Path) -> ProcessCommand {
    let mut command = ProcessCommand::new("cmd");
    command.arg("/C").arg("start").arg("").arg(path);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn build_open_command(path: &Path) -> ProcessCommand {
    let mut command = ProcessCommand::new("xdg-open");
    command.arg(path);
    command
}

fn detail_lines(
    repo_root: &Path,
    detail: &DemoDetail,
    selected_artifact_index: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        kv_line("id", &detail.id),
        kv_line("title", &detail.title),
        kv_line("owner", &detail.owner),
        kv_line("mode", &detail.mode),
        kv_line("status", &detail.effective_status),
        kv_line("gap", &detail.gap_class),
        kv_line(
            "entrypoint",
            &format!("{}:{}", detail.entrypoint.kind, detail.entrypoint.value),
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Summary",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(detail.summary.clone()),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Proof",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(detail.proof.clone()),
        Line::from(""),
        kv_line(
            "actions",
            &format!(
                "run={} stop={} rerun={}",
                yes_no(detail.actions.run.available),
                yes_no(detail.actions.stop.available),
                yes_no(detail.actions.rerun.available),
            ),
        ),
        kv_line("active", &detail.active_attempt.state),
        kv_line("latest", &detail.latest_attempt.state),
    ];

    if !detail.covers.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_heading("Coverage"));
        for cover in &detail.covers {
            lines.push(bullet_line(cover));
        }
    }
    if !detail.tags.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_heading("Tags"));
        for tag in &detail.tags {
            lines.push(bullet_line(tag));
        }
    }
    if !detail.latest_attempt.artifacts.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_heading("Artifacts ([ / ] to select, o to open)"));
        let selected_index = clamp_artifact_index(selected_artifact_index, detail);
        for (index, artifact) in detail.latest_attempt.artifacts.iter().enumerate() {
            lines.push(artifact_line(artifact, index == selected_index));
        }
    }
    if let Some(summary) = &detail.latest_attempt.summary {
        lines.push(Line::from(""));
        lines.push(section_heading("Latest Receipt"));
        lines.push(Line::from(summary.clone()));
    }
    lines.extend(recent_output_lines(repo_root, detail));
    lines
}

fn recent_output_lines(repo_root: &Path, detail: &DemoDetail) -> Vec<Line<'static>> {
    let source = if detail.active_attempt.state != "not-active"
        && (detail.active_attempt.stdout_log_path.is_some()
            || detail.active_attempt.stderr_log_path.is_some())
    {
        Some((
            "Recent Output (active attempt)",
            detail.active_attempt.stdout_log_path.as_deref(),
            detail.active_attempt.stderr_log_path.as_deref(),
        ))
    } else if detail.latest_attempt.stdout_log_path.is_some()
        || detail.latest_attempt.stderr_log_path.is_some()
    {
        Some((
            "Recent Output (latest attempt)",
            detail.latest_attempt.stdout_log_path.as_deref(),
            detail.latest_attempt.stderr_log_path.as_deref(),
        ))
    } else {
        None
    };

    let Some((heading, stdout_log, stderr_log)) = source else {
        return Vec::new();
    };

    let mut lines = vec![Line::from(""), section_heading(heading)];
    lines.extend(render_recent_output_stream(repo_root, "stdout", stdout_log));
    lines.extend(render_recent_output_stream(repo_root, "stderr", stderr_log));
    lines
}

fn render_recent_output_stream(
    repo_root: &Path,
    label: &str,
    log_path: Option<&str>,
) -> Vec<Line<'static>> {
    let Some(log_path) = log_path else {
        return vec![kv_line(label, "<unavailable>")];
    };
    let mut lines = vec![kv_line(label, log_path)];
    match read_recent_log_lines(&resolve_repo_relative_path(repo_root, log_path), 8) {
        Ok(log_lines) if log_lines.is_empty() => {
            lines.push(Line::from("  <no output yet>"));
        }
        Ok(log_lines) => {
            for line in log_lines {
                lines.push(Line::from(format!("  {line}")));
            }
        }
        Err(message) => {
            lines.push(Line::from(format!("  <{message}>")));
        }
    }
    lines
}

fn read_recent_log_lines(path: &Path, limit: usize) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut lines = content.lines().map(str::to_owned).collect::<Vec<_>>();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    Ok(lines)
}

fn kv_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_owned()),
    ])
}

fn section_heading(label: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        label.to_owned(),
        Style::default().add_modifier(Modifier::BOLD),
    )])
}

fn bullet_line(value: &str) -> Line<'static> {
    Line::from(format!("• {value}"))
}

fn artifact_line(value: &str, selected: bool) -> Line<'static> {
    if selected {
        Line::from(vec![
            Span::styled(
                "> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                value.to_owned(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        bullet_line(value)
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[derive(Clone)]
enum BrowserRow {
    Group(String),
    Demo(DemoSummary),
}

struct PendingAction {
    demo_id: String,
    label: String,
    receiver: Receiver<Result<JsonValue, RunnerError>>,
}

#[derive(Debug, Deserialize)]
struct DemoListPayload {
    demos: Vec<DemoSummary>,
    #[serde(default)]
    groups: Option<Vec<DemoGroup>>,
}

#[derive(Debug, Deserialize)]
struct DemoGroup {
    label: String,
    count: usize,
    demos: Vec<DemoSummary>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoSummary {
    id: String,
    effective_status: String,
    actions: DemoActionAvailability,
}

impl DemoSummary {
    fn action_summary(&self) -> &'static str {
        if self.actions.stop.available {
            "stop"
        } else if self.actions.run.available && self.actions.rerun.available {
            "run/rerun"
        } else if self.actions.run.available {
            "run"
        } else if self.actions.rerun.available {
            "rerun"
        } else {
            "none"
        }
    }
}

#[derive(Debug, Deserialize)]
struct DemoInspectPayload {
    demo: DemoDetail,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoDetail {
    id: String,
    title: String,
    summary: String,
    proof: String,
    owner: String,
    mode: String,
    effective_status: String,
    gap_class: String,
    covers: Vec<String>,
    tags: Vec<String>,
    entrypoint: DemoEntrypoint,
    actions: DemoActionAvailability,
    active_attempt: DemoActiveAttempt,
    latest_attempt: DemoLatestAttempt,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoEntrypoint {
    kind: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoActionAvailability {
    run: DemoActionState,
    stop: DemoActionState,
    rerun: DemoActionState,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoActionState {
    available: bool,
    reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoActiveAttempt {
    state: String,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoLatestAttempt {
    recorded: bool,
    state: String,
    artifacts: Vec<String>,
    summary: Option<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        clamp_artifact_index, first_demo_id, next_group_by, read_recent_log_lines,
        resolve_artifact_path, resolve_repo_relative_path, row_contains_demo, selected_artifact,
        BrowserRow, DemoDetail, DemoEntrypoint, DemoLatestAttempt, DemoListGroupBy, DemoSummary,
    };

    fn summary(id: &str) -> DemoSummary {
        DemoSummary {
            id: id.to_owned(),
            effective_status: "ready".to_owned(),
            actions: super::DemoActionAvailability {
                run: super::DemoActionState {
                    available: true,
                    reason: None,
                },
                stop: super::DemoActionState {
                    available: false,
                    reason: None,
                },
                rerun: super::DemoActionState {
                    available: true,
                    reason: None,
                },
            },
        }
    }

    fn detail_with_artifacts(artifacts: &[&str]) -> DemoDetail {
        DemoDetail {
            id: "demo".to_owned(),
            title: "Demo".to_owned(),
            summary: "summary".to_owned(),
            proof: "proof".to_owned(),
            owner: "owner".to_owned(),
            mode: "headless".to_owned(),
            effective_status: "ready".to_owned(),
            gap_class: "existing".to_owned(),
            covers: vec![],
            tags: vec![],
            entrypoint: DemoEntrypoint {
                kind: "task".to_owned(),
                value: "demo:task".to_owned(),
            },
            actions: super::DemoActionAvailability {
                run: super::DemoActionState {
                    available: true,
                    reason: None,
                },
                stop: super::DemoActionState {
                    available: false,
                    reason: None,
                },
                rerun: super::DemoActionState {
                    available: true,
                    reason: None,
                },
            },
            active_attempt: super::DemoActiveAttempt {
                state: "idle".to_owned(),
                stdout_log_path: None,
                stderr_log_path: None,
            },
            latest_attempt: DemoLatestAttempt {
                recorded: true,
                state: "passed".to_owned(),
                artifacts: artifacts.iter().map(|value| (*value).to_owned()).collect(),
                summary: None,
                stdout_log_path: None,
                stderr_log_path: None,
            },
        }
    }

    #[test]
    fn browser_group_cycle_is_bounded() {
        assert_eq!(next_group_by(None), Some(DemoListGroupBy::Owner));
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Owner)),
            Some(DemoListGroupBy::Status)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Status)),
            Some(DemoListGroupBy::Gap)
        );
        assert_eq!(next_group_by(Some(DemoListGroupBy::Gap)), None);
    }

    #[test]
    fn browser_row_helpers_ignore_group_headers() {
        let rows = vec![
            BrowserRow::Group("ready".to_owned()),
            BrowserRow::Demo(summary("alpha")),
            BrowserRow::Demo(summary("beta")),
        ];
        assert_eq!(first_demo_id(&rows).as_deref(), Some("alpha"));
        assert!(row_contains_demo(&rows, "beta"));
        assert!(!row_contains_demo(&rows, "missing"));
    }

    #[test]
    fn browser_selected_artifact_clamps_to_available_range() {
        let detail = detail_with_artifacts(&["one", "two"]);
        assert_eq!(clamp_artifact_index(0, &detail), 0);
        assert_eq!(clamp_artifact_index(5, &detail), 1);
        assert_eq!(selected_artifact(&detail, 5), Some("two"));
    }

    #[test]
    fn browser_resolves_relative_artifacts_against_repo_root() {
        let repo_root = Path::new("/tmp/demo-repo");
        assert_eq!(
            resolve_artifact_path(repo_root, ".effigy/demo/report.html"),
            repo_root.join(".effigy/demo/report.html")
        );
    }

    #[test]
    fn browser_resolves_generic_repo_relative_paths() {
        let repo_root = Path::new("/tmp/demo-repo");
        assert_eq!(
            resolve_repo_relative_path(repo_root, ".effigy/demo/logs/demo.stdout.log"),
            repo_root.join(".effigy/demo/logs/demo.stdout.log")
        );
    }

    #[test]
    fn browser_reads_only_recent_log_lines() {
        let temp_path = std::env::temp_dir().join(format!(
            "effigy-demo-browser-log-{}.txt",
            std::process::id()
        ));
        std::fs::write(
            &temp_path,
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
        )
        .expect("write log");
        let lines = read_recent_log_lines(&temp_path, 4).expect("read log");
        let _ = std::fs::remove_file(&temp_path);
        assert_eq!(lines, vec!["six", "seven", "eight", "nine"]);
    }
}
