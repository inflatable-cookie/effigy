use std::io::{self, Stdout, Write};
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
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::runner::{run_command, RunnerError};
use crate::tui::core::{effigy_panel_block, next_index, prev_index, EFFIGY_ACCENT, EFFIGY_MUTED};
use crate::{
    Command, DemoArgs, DemoListGap, DemoListGroupBy, DemoListMode, DemoListQuery, DemoListStatus,
    DemoSubcommand,
};

type BrowserTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn run_demo_browser_tui(
    repo_root: PathBuf,
    initial_group_by: Option<DemoListGroupBy>,
) -> Result<(), RunnerError> {
    let mut terminal = init_browser_terminal()?;
    let mut app = DemoBrowserApp::new(repo_root, initial_group_by);
    let result = app.run(&mut terminal);
    restore_browser_terminal(&mut terminal)?;
    match result? {
        Some(handoff) => run_demo_browser_handoff(&app.repo_root, handoff),
        None => Ok(()),
    }
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
    query: DemoListQuery,
    rows: Vec<BrowserRow>,
    focus: BrowserFocus,
    selected_demo_id: Option<String>,
    selected_row_index: usize,
    selected_artifact_index: usize,
    detail: Option<DemoDetail>,
    footer_message: String,
    pending_action: Option<PendingAction>,
    last_refresh: Instant,
    total_demo_count: usize,
    overlay: Option<BrowserOverlay>,
    handoff: Option<BrowserHandoff>,
}

impl DemoBrowserApp {
    fn new(repo_root: PathBuf, initial_group_by: Option<DemoListGroupBy>) -> Self {
        Self {
            repo_root,
            group_by: initial_group_by,
            query: DemoListQuery {
                group_by: initial_group_by,
                ..DemoListQuery::default()
            },
            rows: Vec::new(),
            focus: BrowserFocus::List,
            selected_demo_id: None,
            selected_row_index: 0,
            selected_artifact_index: 0,
            detail: None,
            footer_message: "Loading demo registry...".to_owned(),
            pending_action: None,
            last_refresh: Instant::now() - Duration::from_secs(5),
            total_demo_count: 0,
            overlay: None,
            handoff: None,
        }
    }

    fn run(
        &mut self,
        terminal: &mut BrowserTerminal,
    ) -> Result<Option<BrowserHandoff>, RunnerError> {
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
            if self.handoff.is_some() {
                break;
            }
        }
        Ok(self.handoff.take())
    }

    fn handle_key(&mut self, code: KeyCode) -> Result<bool, RunnerError> {
        if self.handle_overlay_key(code)? {
            return Ok(false);
        }
        match code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
            KeyCode::Down => self.handle_down_key(),
            KeyCode::Up => self.handle_up_key(),
            KeyCode::Right => self.focus_detail(),
            KeyCode::Left => self.focus_list(),
            KeyCode::Char('/') => self.open_prompt(QueryPromptKind::Search),
            KeyCode::Char('f') => self.open_filter_overlay(),
            KeyCode::Enter => self.handle_enter_key()?,
            KeyCode::Char('R') => {
                self.refresh_state()?;
                self.footer_message = "Refreshed demo browser state.".to_owned();
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_down_key(&mut self) {
        match self.focus {
            BrowserFocus::List => self.select_next_demo(),
            BrowserFocus::Detail => self.select_next_artifact(),
        }
    }

    fn handle_up_key(&mut self) {
        match self.focus {
            BrowserFocus::List => self.select_previous_demo(),
            BrowserFocus::Detail => self.select_previous_artifact(),
        }
    }

    fn handle_enter_key(&mut self) -> Result<(), RunnerError> {
        match self.focus {
            BrowserFocus::List => {
                self.open_action_overlay();
                Ok(())
            }
            BrowserFocus::Detail => self.dispatch_open_artifact(),
        }
    }

    fn handle_overlay_key(&mut self, code: KeyCode) -> Result<bool, RunnerError> {
        let Some(mut overlay) = self.overlay.take() else {
            return Ok(false);
        };
        let mut keep_open = false;
        match &mut overlay {
            BrowserOverlay::Prompt(prompt) => match code {
                KeyCode::Esc => {
                    self.footer_message = "Closed browser prompt.".to_owned();
                }
                KeyCode::Enter => match prompt.kind {
                    QueryPromptKind::Search => {
                        self.query.search = normalized_prompt_value(&prompt.value);
                        self.refresh_state()?;
                        self.footer_message =
                            prompt_apply_message("search", self.query.search.as_deref());
                    }
                    QueryPromptKind::Owner => {
                        self.query.owner = normalized_prompt_value(&prompt.value);
                        self.refresh_state()?;
                        self.footer_message =
                            prompt_apply_message("owner", self.query.owner.as_deref());
                    }
                    QueryPromptKind::Tag => {
                        self.query.tag = normalized_prompt_value(&prompt.value);
                        self.refresh_state()?;
                        self.footer_message =
                            prompt_apply_message("tag", self.query.tag.as_deref());
                    }
                    QueryPromptKind::Cover => {
                        self.query.cover = normalized_prompt_value(&prompt.value);
                        self.refresh_state()?;
                        self.footer_message =
                            prompt_apply_message("cover", self.query.cover.as_deref());
                    }
                },
                KeyCode::Backspace => {
                    prompt.value.pop();
                    keep_open = true;
                }
                KeyCode::Char(ch) => {
                    if !ch.is_control() {
                        prompt.value.push(ch);
                    }
                    keep_open = true;
                }
                _ => keep_open = true,
            },
            BrowserOverlay::Action(menu) => match code {
                KeyCode::Esc => {
                    self.footer_message = "Closed browser action menu.".to_owned();
                }
                KeyCode::Down => {
                    menu.select_next();
                    keep_open = true;
                }
                KeyCode::Up => {
                    menu.select_previous();
                    keep_open = true;
                }
                KeyCode::Enter => {
                    if let Some(item) = menu.selected_item() {
                        self.run_action_menu_item(item)?;
                    }
                }
                _ => keep_open = true,
            },
            BrowserOverlay::Filters(menu) => match code {
                KeyCode::Esc => {
                    self.footer_message = "Closed browser filter menu.".to_owned();
                }
                KeyCode::Down => {
                    menu.select_next();
                    keep_open = true;
                }
                KeyCode::Up => {
                    menu.select_previous();
                    keep_open = true;
                }
                KeyCode::Enter => {
                    let item = menu.selected_item();
                    self.apply_filter_menu_item(item)?;
                    keep_open = self.overlay.is_none()
                        && !matches!(
                            item,
                            FilterMenuItem::Search
                                | FilterMenuItem::Owner
                                | FilterMenuItem::Tag
                                | FilterMenuItem::Cover
                        );
                }
                _ => keep_open = true,
            },
        }
        if keep_open && self.overlay.is_none() {
            self.overlay = Some(overlay);
        }
        Ok(true)
    }

    fn open_prompt(&mut self, kind: QueryPromptKind) {
        let current = match kind {
            QueryPromptKind::Search => self.query.search.clone(),
            QueryPromptKind::Owner => self.query.owner.clone(),
            QueryPromptKind::Tag => self.query.tag.clone(),
            QueryPromptKind::Cover => self.query.cover.clone(),
        }
        .unwrap_or_default();
        self.overlay = Some(BrowserOverlay::Prompt(QueryPromptState {
            kind,
            value: current,
        }));
        self.footer_message = match kind {
            QueryPromptKind::Search => {
                "Editing search filter. Enter applies, Esc cancels.".to_owned()
            }
            QueryPromptKind::Owner => {
                "Editing owner filter. Enter applies, Esc cancels.".to_owned()
            }
            QueryPromptKind::Tag => "Editing tag filter. Enter applies, Esc cancels.".to_owned(),
            QueryPromptKind::Cover => {
                "Editing cover filter. Enter applies, Esc cancels.".to_owned()
            }
        };
    }

    fn open_action_overlay(&mut self) {
        let items = self.action_menu_items();
        self.overlay = Some(BrowserOverlay::Action(ActionMenuState::new(items)));
        self.footer_message = "Use ↑/↓ to choose an action. Enter applies. Esc closes.".to_owned();
    }

    fn open_filter_overlay(&mut self) {
        self.overlay = Some(BrowserOverlay::Filters(FilterMenuState::default()));
        self.footer_message =
            "Use ↑/↓ to choose a filter. Enter edits or cycles. Esc closes.".to_owned();
    }

    fn action_menu_items(&self) -> Vec<ActionMenuItem> {
        let Some(detail) = self.selected_detail() else {
            return vec![ActionMenuItem::Refresh];
        };
        let mut items = Vec::new();
        if let Some(subcommand) = preferred_run_action(detail) {
            items.push(match subcommand {
                DemoSubcommand::Run { .. } => ActionMenuItem::Run,
                DemoSubcommand::Rerun { .. } => ActionMenuItem::Rerun,
                _ => unreachable!(),
            });
        }
        if detail.actions.stop.available {
            items.push(ActionMenuItem::Stop);
        }
        if !detail.latest_attempt.artifacts.is_empty() {
            items.push(ActionMenuItem::OpenArtifact);
        }
        items.push(ActionMenuItem::OpenHistory);
        items.push(ActionMenuItem::Refresh);
        items
    }

    fn run_action_menu_item(&mut self, item: ActionMenuItem) -> Result<(), RunnerError> {
        match item {
            ActionMenuItem::Run | ActionMenuItem::Rerun => self.dispatch_run_or_rerun(),
            ActionMenuItem::Stop => self.dispatch_stop(),
            ActionMenuItem::OpenArtifact => self.dispatch_open_artifact(),
            ActionMenuItem::OpenHistory => self.dispatch_open_history(),
            ActionMenuItem::Refresh => {
                self.refresh_state()?;
                self.footer_message = "Refreshed demo browser state.".to_owned();
                Ok(())
            }
        }
    }

    fn apply_filter_menu_item(&mut self, item: FilterMenuItem) -> Result<(), RunnerError> {
        match item {
            FilterMenuItem::Search => self.open_prompt(QueryPromptKind::Search),
            FilterMenuItem::Owner => self.open_prompt(QueryPromptKind::Owner),
            FilterMenuItem::Tag => self.open_prompt(QueryPromptKind::Tag),
            FilterMenuItem::Mode => {
                self.query.mode = next_mode_filter(self.query.mode);
                self.refresh_state()?;
                self.footer_message =
                    filter_change_message("mode", self.query.mode.map(DemoListMode::as_str));
            }
            FilterMenuItem::Cover => self.open_prompt(QueryPromptKind::Cover),
            FilterMenuItem::Status => {
                self.query.status = next_status_filter(self.query.status);
                self.refresh_state()?;
                self.footer_message =
                    filter_change_message("status", self.query.status.map(DemoListStatus::as_str));
            }
            FilterMenuItem::Gap => {
                self.query.gap = next_gap_filter(self.query.gap);
                self.refresh_state()?;
                self.footer_message =
                    filter_change_message("gap", self.query.gap.map(DemoListGap::as_str));
            }
            FilterMenuItem::StaleOnly => {
                self.query.stale_only = !self.query.stale_only;
                self.refresh_state()?;
                self.footer_message = if self.query.stale_only {
                    "Enabled stale-only demo filtering.".to_owned()
                } else {
                    "Disabled stale-only demo filtering.".to_owned()
                };
            }
            FilterMenuItem::GroupBy => {
                self.group_by = next_group_by(self.group_by);
                self.query.group_by = self.group_by;
                self.refresh_state()?;
                self.footer_message = format!(
                    "Grouping demos by {}",
                    self.group_by.map_or("none", DemoListGroupBy::as_str)
                );
            }
            FilterMenuItem::ClearAll => {
                self.query = DemoListQuery {
                    group_by: self.group_by,
                    ..DemoListQuery::default()
                };
                self.refresh_state()?;
                self.footer_message = "Cleared browser filters.".to_owned();
            }
        }
        Ok(())
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

    fn detail_lines(&self) -> Vec<Line<'static>> {
        if let Some(detail) = &self.detail {
            detail_lines(
                &self.repo_root,
                detail,
                self.selected_artifact_index,
                matches!(self.focus, BrowserFocus::Detail),
            )
        } else {
            vec![
                Line::from("No demo selected."),
                Line::from(""),
                Line::from("Use ↑/↓ in the list to select a demo."),
            ]
        }
    }

    fn focus_list(&mut self) {
        self.focus = BrowserFocus::List;
        self.footer_message =
            "List panel focused. ↑/↓ selects demos. Enter opens actions.".to_owned();
    }

    fn focus_detail(&mut self) {
        self.focus = BrowserFocus::Detail;
        self.footer_message =
            "Detail panel focused. ↑/↓ selects artifacts. Enter opens the selected artifact."
                .to_owned();
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

    fn dispatch_open_history(&mut self) -> Result<(), RunnerError> {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(());
        };
        let demo_id = detail.id.clone();
        self.handoff = Some(BrowserHandoff::DemoHistory {
            demo_id: demo_id.clone(),
        });
        self.footer_message = format!(
            "Handing off to `effigy demo history {}` outside the browser.",
            demo_id
        );
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
                    query: self.query.clone(),
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
        self.total_demo_count = list_payload.total_count;
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

    fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(10),
                Constraint::Length(4),
            ])
            .split(area);
        self.render_header(frame, layout[0]);
        self.render_body(frame, layout[1]);
        self.render_footer(frame, layout[2]);
        if self.rows.is_empty() {
            self.render_empty_overlay(frame, area);
        }
        if self.overlay.is_some() {
            self.render_overlay(frame, area);
        }
    }

    fn render_header(&self, frame: &mut Frame<'_>, area: Rect) {
        let pending = self
            .pending_action
            .as_ref()
            .map_or("idle", |action| action.label.as_str());
        let lines = vec![
            Line::from(vec![
                Span::styled(
                    " Demo Browser ",
                    Style::default()
                        .fg(EFFIGY_MUTED)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled("group:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", self.group_by.map_or("none", DemoListGroupBy::as_str)),
                    Style::default().fg(Color::White),
                ),
                Span::raw("  "),
                Span::styled("pending:", Style::default().fg(Color::DarkGray)),
                Span::styled(format!(" {pending}"), Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled("repo:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", self.repo_root.display()),
                    Style::default().fg(EFFIGY_MUTED),
                ),
            ]),
            Line::from(vec![
                Span::styled("query:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", query_summary(&self.query)),
                    Style::default().fg(Color::White),
                ),
                Span::raw("  "),
                Span::styled("count:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}/{}", self.rows_demo_count(), self.total_demo_count),
                    Style::default().fg(Color::White),
                ),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(effigy_panel_block(Some(" EFFIGY "), true, EFFIGY_ACCENT)),
            area,
        );
    }

    fn render_body(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        self.render_list(frame, layout[0]);
        self.render_detail(frame, layout[1]);
    }

    fn render_list(&self, frame: &mut Frame<'_>, area: Rect) {
        let list_focused = matches!(self.focus, BrowserFocus::List);
        let items = self
            .rows
            .iter()
            .map(|row| match row {
                BrowserRow::Group(label) => ListItem::new(Line::from(vec![Span::styled(
                    format!("  {label}"),
                    Style::default()
                        .fg(EFFIGY_ACCENT)
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
                            Style::default().fg(EFFIGY_MUTED),
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
            .block(effigy_panel_block(
                Some(" Demos "),
                false,
                if list_focused {
                    EFFIGY_ACCENT
                } else {
                    Color::DarkGray
                },
            ))
            .highlight_style(if list_focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            })
            .highlight_symbol(if list_focused { "▌" } else { " " })
            .repeat_highlight_symbol(true);
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_detail(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let lines = self.detail_lines();
        let detail = Paragraph::new(lines)
            .block(effigy_panel_block(
                Some(" Detail "),
                false,
                if matches!(self.focus, BrowserFocus::Detail) {
                    EFFIGY_ACCENT
                } else {
                    Color::DarkGray
                },
            ))
            .wrap(Wrap { trim: false });
        frame.render_widget(detail, area);
    }

    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let help = Line::from(vec![
            Span::styled(" ↑↓ ", key_style()),
            Span::raw("move  "),
            Span::styled(" ←→ ", key_style()),
            Span::raw("panel  "),
            Span::styled(" Enter ", key_style()),
            Span::raw("act/open  "),
            Span::styled(" / ", key_style()),
            Span::raw("search  "),
            Span::styled(" f ", key_style()),
            Span::raw("filters  "),
            Span::styled(" Esc ", key_style()),
            Span::raw("back/quit"),
        ]);
        let footer = Paragraph::new(vec![help, Line::from(self.footer_message.clone())])
            .block(effigy_panel_block(None, false, Color::DarkGray));
        frame.render_widget(footer, area);
    }

    fn render_empty_overlay(&self, frame: &mut Frame<'_>, area: Rect) {
        let overlay = centered_rect(60, 20, area);
        frame.render_widget(Clear, overlay);
        let lines = if self.total_demo_count == 0 {
            vec![
                Line::from("No demos are declared in the current manifest."),
                Line::from(""),
                Line::from("Add `[demos.<id>]` entries to `effigy.toml` first."),
            ]
        } else {
            vec![
                Line::from("No demos match the current browser query."),
                Line::from(""),
                Line::from(format!("Active query: {}", query_summary(&self.query))),
                Line::from("Use / to search or f to adjust the current filter set."),
            ]
        };
        let notice = Paragraph::new(lines)
            .block(effigy_panel_block(
                Some(" Demo Browser "),
                false,
                EFFIGY_ACCENT,
            ))
            .wrap(Wrap { trim: true });
        frame.render_widget(notice, overlay);
    }

    fn render_overlay(&self, frame: &mut Frame<'_>, area: Rect) {
        let Some(overlay_state) = &self.overlay else {
            return;
        };
        match overlay_state {
            BrowserOverlay::Prompt(prompt) => self.render_prompt_overlay(frame, area, prompt),
            BrowserOverlay::Action(menu) => self.render_action_overlay(frame, area, menu),
            BrowserOverlay::Filters(menu) => self.render_filter_overlay(frame, area, menu),
        }
    }

    fn render_prompt_overlay(&self, frame: &mut Frame<'_>, area: Rect, prompt: &QueryPromptState) {
        let overlay = centered_rect(68, 22, area);
        frame.render_widget(Clear, overlay);
        let prompt_widget = Paragraph::new(vec![
            Line::from(prompt.kind.title()),
            Line::from(""),
            Line::from(prompt.kind.help()),
            Line::from(""),
            Line::from(vec![
                Span::styled("value: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(prompt.render_value()),
            ]),
            Line::from(""),
            Line::from("Enter applies. Esc closes. Empty clears the filter."),
        ])
        .block(effigy_panel_block(Some(" Query "), false, EFFIGY_ACCENT))
        .wrap(Wrap { trim: true });
        frame.render_widget(prompt_widget, overlay);
    }

    fn render_action_overlay(&self, frame: &mut Frame<'_>, area: Rect, menu: &ActionMenuState) {
        let overlay = centered_rect(42, 28, area);
        frame.render_widget(Clear, overlay);
        let items = menu
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let line = if index == menu.selected_index {
                    Line::from(vec![
                        Span::styled("▌ ", Style::default().fg(EFFIGY_ACCENT)),
                        Span::styled(
                            item.label(),
                            Style::default()
                                .fg(EFFIGY_ACCENT)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])
                } else {
                    Line::from(format!("  {}", item.label()))
                };
                ListItem::new(line)
            })
            .collect::<Vec<_>>();
        let widget =
            List::new(items).block(effigy_panel_block(Some(" Actions "), false, EFFIGY_ACCENT));
        frame.render_widget(widget, overlay);
    }

    fn render_filter_overlay(&self, frame: &mut Frame<'_>, area: Rect, menu: &FilterMenuState) {
        let overlay = centered_rect(62, 40, area);
        frame.render_widget(Clear, overlay);
        let items = menu
            .items()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let value = self.filter_menu_value(*item);
                let line = if index == menu.selected_index {
                    Line::from(vec![
                        Span::styled("▌ ", Style::default().fg(EFFIGY_ACCENT)),
                        Span::styled(
                            format!("{:<12}", item.label()),
                            Style::default()
                                .fg(EFFIGY_ACCENT)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(value),
                    ])
                } else {
                    Line::from(format!("  {:<12}{}", item.label(), value))
                };
                ListItem::new(line)
            })
            .collect::<Vec<_>>();
        let widget =
            List::new(items).block(effigy_panel_block(Some(" Filters "), false, EFFIGY_ACCENT));
        frame.render_widget(widget, overlay);
    }

    fn rows_demo_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row, BrowserRow::Demo(_)))
            .count()
    }

    fn filter_menu_value(&self, item: FilterMenuItem) -> String {
        match item {
            FilterMenuItem::Search => self
                .query
                .search
                .clone()
                .unwrap_or_else(|| "none".to_owned()),
            FilterMenuItem::Owner => self
                .query
                .owner
                .clone()
                .unwrap_or_else(|| "none".to_owned()),
            FilterMenuItem::Tag => self.query.tag.clone().unwrap_or_else(|| "none".to_owned()),
            FilterMenuItem::Mode => self
                .query
                .mode
                .map(DemoListMode::as_str)
                .unwrap_or("none")
                .to_owned(),
            FilterMenuItem::Cover => self
                .query
                .cover
                .clone()
                .unwrap_or_else(|| "none".to_owned()),
            FilterMenuItem::Status => self
                .query
                .status
                .map(DemoListStatus::as_str)
                .unwrap_or("none")
                .to_owned(),
            FilterMenuItem::Gap => self
                .query
                .gap
                .map(DemoListGap::as_str)
                .unwrap_or("none")
                .to_owned(),
            FilterMenuItem::StaleOnly => {
                if self.query.stale_only {
                    "on".to_owned()
                } else {
                    "off".to_owned()
                }
            }
            FilterMenuItem::GroupBy => self
                .group_by
                .map(DemoListGroupBy::as_str)
                .unwrap_or("none")
                .to_owned(),
            FilterMenuItem::ClearAll => "reset all filters".to_owned(),
        }
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

fn run_demo_browser_handoff(repo_root: &Path, handoff: BrowserHandoff) -> Result<(), RunnerError> {
    match handoff {
        BrowserHandoff::DemoHistory { demo_id } => {
            let rendered = run_command(Command::Demo(DemoArgs {
                subcommand: DemoSubcommand::History {
                    demo_id,
                    limit: None,
                    outcome: None,
                    attempt_id: None,
                    attempt_ordinal: None,
                },
                repo_override: Some(repo_root.to_path_buf()),
                output_json: false,
            }))?;
            let mut stdout = io::stdout();
            stdout
                .write_all(rendered.as_bytes())
                .map_err(|error| RunnerError::Ui(error.to_string()))?;
            stdout
                .flush()
                .map_err(|error| RunnerError::Ui(error.to_string()))?;
            Ok(())
        }
    }
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
        .fg(Color::Yellow)
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
        Some(DemoListGroupBy::Owner) => Some(DemoListGroupBy::Tag),
        Some(DemoListGroupBy::Tag) => Some(DemoListGroupBy::Mode),
        Some(DemoListGroupBy::Mode) => Some(DemoListGroupBy::Cover),
        Some(DemoListGroupBy::Cover) => Some(DemoListGroupBy::Status),
        Some(DemoListGroupBy::Status) => Some(DemoListGroupBy::Gap),
        Some(DemoListGroupBy::Gap) => None,
    }
}

fn next_mode_filter(current: Option<DemoListMode>) -> Option<DemoListMode> {
    match current {
        None => Some(DemoListMode::Headless),
        Some(DemoListMode::Headless) => Some(DemoListMode::Interactive),
        Some(DemoListMode::Interactive) => Some(DemoListMode::Hybrid),
        Some(DemoListMode::Hybrid) => None,
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
    _repo_root: &Path,
    detail: &DemoDetail,
    selected_artifact_index: usize,
    detail_focused: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![title_line(&detail.title)];

    if !detail.tags.is_empty() {
        lines.push(muted_line(format!("tags: {}", detail.tags.join(", "))));
    }

    lines.extend([
        Line::from(""),
        section_heading("Summary"),
        Line::from(detail.summary.clone()),
        Line::from(""),
        section_heading("Proof"),
        Line::from(detail.proof.clone()),
    ]);

    if !detail.covers.is_empty() {
        lines.push(Line::from(""));
        lines.push(compact_kv_line("covers", &detail.covers.join(", ")));
    }
    if detail.latest_attempt.recorded {
        lines.push(Line::from(""));
        lines.push(section_heading("Result"));
        lines.push(compact_kv_line("status", &detail.latest_attempt.state));
        if let Some(summary) = &detail.latest_attempt.summary {
            lines.push(Line::from(summary.clone()));
        }
    }
    lines.push(Line::from(""));
    lines.push(section_heading("History"));
    lines.push(compact_kv_line(
        "command",
        &format!("effigy demo history {}", detail.id),
    ));
    lines.push(muted_line(
        "Open Actions and choose Open history to leave the browser and run the dedicated history view."
            .to_owned(),
    ));
    lines.push(Line::from(""));
    lines.push(section_heading("Artifacts"));
    if detail.latest_attempt.artifacts.is_empty() {
        lines.push(muted_line("No recorded artifacts.".to_owned()));
    } else {
        let selected_index = clamp_artifact_index(selected_artifact_index, detail);
        for (index, artifact) in detail.latest_attempt.artifacts.iter().enumerate() {
            let selected = index == selected_index;
            lines.push(artifact_list_line(artifact, selected, detail_focused));
        }
        lines.push(muted_line(if detail_focused {
            "↑/↓ selects artifact  •  Enter opens selection".to_owned()
        } else {
            "→ focuses artifacts".to_owned()
        }));
    }
    lines
}

#[cfg(test)]
use std::fs;

#[cfg(test)]
fn read_recent_log_lines(path: &Path, limit: usize) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut lines = content.lines().map(str::to_owned).collect::<Vec<_>>();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    Ok(lines)
}

fn compact_kv_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_owned()),
    ])
}

fn section_heading(label: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        label.to_owned(),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    )])
}

fn artifact_list_line(value: &str, selected: bool, focused: bool) -> Line<'static> {
    let marker = if selected && focused { "› " } else { "  " };
    let style = if selected && focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    Line::from(vec![
        Span::styled(marker, style),
        Span::styled(value.to_owned(), style),
    ])
}

fn title_line(value: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        value.to_owned(),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )])
}

fn muted_line(value: String) -> Line<'static> {
    Line::from(vec![Span::styled(
        value,
        Style::default().fg(Color::DarkGray),
    )])
}

fn next_status_filter(current: Option<DemoListStatus>) -> Option<DemoListStatus> {
    match current {
        None => Some(DemoListStatus::Planned),
        Some(DemoListStatus::Planned) => Some(DemoListStatus::Ready),
        Some(DemoListStatus::Ready) => Some(DemoListStatus::Running),
        Some(DemoListStatus::Running) => Some(DemoListStatus::Passed),
        Some(DemoListStatus::Passed) => Some(DemoListStatus::Failed),
        Some(DemoListStatus::Failed) => Some(DemoListStatus::Broken),
        Some(DemoListStatus::Broken) => Some(DemoListStatus::Missing),
        Some(DemoListStatus::Missing) => None,
    }
}

fn next_gap_filter(current: Option<DemoListGap>) -> Option<DemoListGap> {
    match current {
        None => Some(DemoListGap::Existing),
        Some(DemoListGap::Existing) => Some(DemoListGap::Planned),
        Some(DemoListGap::Planned) => Some(DemoListGap::Missing),
        Some(DemoListGap::Missing) => Some(DemoListGap::Broken),
        Some(DemoListGap::Broken) => Some(DemoListGap::Stale),
        Some(DemoListGap::Stale) => None,
    }
}

fn normalized_prompt_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn query_summary(query: &DemoListQuery) -> String {
    let mut parts = Vec::new();
    if let Some(search) = &query.search {
        parts.push(format!("search={search}"));
    }
    if let Some(owner) = &query.owner {
        parts.push(format!("owner={owner}"));
    }
    if let Some(tag) = &query.tag {
        parts.push(format!("tag={tag}"));
    }
    if let Some(mode) = query.mode {
        parts.push(format!("mode={}", mode.as_str()));
    }
    if let Some(cover) = &query.cover {
        parts.push(format!("cover={cover}"));
    }
    if let Some(status) = query.status {
        parts.push(format!("status={}", status.as_str()));
    }
    if let Some(gap) = query.gap {
        parts.push(format!("gap={}", gap.as_str()));
    }
    if query.stale_only {
        parts.push("stale-only=true".to_owned());
    }
    if parts.is_empty() {
        "none".to_owned()
    } else {
        parts.join(", ")
    }
}

fn filter_change_message(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("Set {label} filter to `{value}`."),
        None => format!("Cleared {label} filter."),
    }
}

fn prompt_apply_message(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("Set {label} filter to `{value}`."),
        None => format!("Cleared {label} filter."),
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

enum BrowserOverlay {
    Prompt(QueryPromptState),
    Action(ActionMenuState),
    Filters(FilterMenuState),
}

#[derive(Clone, Copy)]
enum BrowserFocus {
    List,
    Detail,
}

struct ActionMenuState {
    items: Vec<ActionMenuItem>,
    selected_index: usize,
}

impl ActionMenuState {
    fn new(items: Vec<ActionMenuItem>) -> Self {
        Self {
            items,
            selected_index: 0,
        }
    }

    fn select_next(&mut self) {
        self.selected_index = next_index(self.selected_index, self.items.len());
    }

    fn select_previous(&mut self) {
        self.selected_index = prev_index(self.selected_index, self.items.len());
    }

    fn selected_item(&self) -> Option<ActionMenuItem> {
        self.items.get(self.selected_index).copied()
    }
}

#[derive(Clone, Copy)]
enum ActionMenuItem {
    Run,
    Rerun,
    Stop,
    OpenArtifact,
    OpenHistory,
    Refresh,
}

impl ActionMenuItem {
    fn label(self) -> &'static str {
        match self {
            Self::Run => "Run demo",
            Self::Rerun => "Rerun demo",
            Self::Stop => "Stop demo",
            Self::OpenArtifact => "Open artifact",
            Self::OpenHistory => "Open history",
            Self::Refresh => "Refresh state",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BrowserHandoff {
    DemoHistory { demo_id: String },
}

#[derive(Default)]
struct FilterMenuState {
    selected_index: usize,
}

impl FilterMenuState {
    const ITEMS: [FilterMenuItem; 10] = [
        FilterMenuItem::Search,
        FilterMenuItem::Owner,
        FilterMenuItem::Tag,
        FilterMenuItem::Mode,
        FilterMenuItem::Cover,
        FilterMenuItem::Status,
        FilterMenuItem::Gap,
        FilterMenuItem::StaleOnly,
        FilterMenuItem::GroupBy,
        FilterMenuItem::ClearAll,
    ];

    fn items(&self) -> &'static [FilterMenuItem] {
        &Self::ITEMS
    }

    fn select_next(&mut self) {
        self.selected_index = next_index(self.selected_index, Self::ITEMS.len());
    }

    fn select_previous(&mut self) {
        self.selected_index = prev_index(self.selected_index, Self::ITEMS.len());
    }

    fn selected_item(&self) -> FilterMenuItem {
        Self::ITEMS[self.selected_index]
    }
}

#[derive(Clone, Copy)]
enum FilterMenuItem {
    Search,
    Owner,
    Tag,
    Mode,
    Cover,
    Status,
    Gap,
    StaleOnly,
    GroupBy,
    ClearAll,
}

impl FilterMenuItem {
    fn label(self) -> &'static str {
        match self {
            Self::Search => "Search",
            Self::Owner => "Owner",
            Self::Tag => "Tag",
            Self::Mode => "Mode",
            Self::Cover => "Cover",
            Self::Status => "Status",
            Self::Gap => "Gap",
            Self::StaleOnly => "Stale",
            Self::GroupBy => "Group by",
            Self::ClearAll => "Clear all",
        }
    }
}

struct QueryPromptState {
    kind: QueryPromptKind,
    value: String,
}

impl QueryPromptState {
    fn render_value(&self) -> String {
        if self.value.is_empty() {
            "<empty>".to_owned()
        } else {
            self.value.clone()
        }
    }
}

#[derive(Clone, Copy)]
enum QueryPromptKind {
    Search,
    Owner,
    Tag,
    Cover,
}

impl QueryPromptKind {
    fn title(self) -> &'static str {
        match self {
            Self::Search => "Edit Search Filter",
            Self::Owner => "Edit Owner Filter",
            Self::Tag => "Edit Tag Filter",
            Self::Cover => "Edit Cover Filter",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::Search => "Match demo id, title, or summary text.",
            Self::Owner => "Match one exact demo owner.",
            Self::Tag => "Match one exact declared tag.",
            Self::Cover => "Match one declared coverage key.",
        }
    }
}

#[derive(Debug, Deserialize)]
struct DemoListPayload {
    #[serde(default)]
    total_count: usize,
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
    #[allow(dead_code)]
    owner: String,
    #[allow(dead_code)]
    mode: String,
    #[allow(dead_code)]
    effective_status: String,
    #[allow(dead_code)]
    gap_class: String,
    covers: Vec<String>,
    tags: Vec<String>,
    actions: DemoActionAvailability,
    #[allow(dead_code)]
    active_attempt: DemoActiveAttempt,
    latest_attempt: DemoLatestAttempt,
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
    #[allow(dead_code)]
    state: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoLatestAttempt {
    recorded: bool,
    state: String,
    artifacts: Vec<String>,
    summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        clamp_artifact_index, detail_lines, first_demo_id, next_gap_filter, next_group_by,
        next_mode_filter, next_status_filter, query_summary, read_recent_log_lines,
        resolve_artifact_path, resolve_repo_relative_path, row_contains_demo, selected_artifact,
        ActionMenuItem, BrowserRow, DemoDetail, DemoLatestAttempt, DemoListGap, DemoListGroupBy,
        DemoListMode, DemoListQuery, DemoListStatus, DemoSummary,
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
            },
            latest_attempt: DemoLatestAttempt {
                recorded: true,
                state: "passed".to_owned(),
                artifacts: artifacts.iter().map(|value| (*value).to_owned()).collect(),
                summary: None,
            },
        }
    }

    #[test]
    fn browser_group_cycle_is_bounded() {
        assert_eq!(next_group_by(None), Some(DemoListGroupBy::Owner));
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Owner)),
            Some(DemoListGroupBy::Tag)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Tag)),
            Some(DemoListGroupBy::Mode)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Mode)),
            Some(DemoListGroupBy::Cover)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Cover)),
            Some(DemoListGroupBy::Status)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Status)),
            Some(DemoListGroupBy::Gap)
        );
        assert_eq!(next_group_by(Some(DemoListGroupBy::Gap)), None);
    }

    #[test]
    fn browser_mode_filter_cycle_is_bounded() {
        assert_eq!(next_mode_filter(None), Some(DemoListMode::Headless));
        assert_eq!(
            next_mode_filter(Some(DemoListMode::Headless)),
            Some(DemoListMode::Interactive)
        );
        assert_eq!(
            next_mode_filter(Some(DemoListMode::Interactive)),
            Some(DemoListMode::Hybrid)
        );
        assert_eq!(next_mode_filter(Some(DemoListMode::Hybrid)), None);
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

    #[test]
    fn browser_status_filter_cycle_is_bounded() {
        assert_eq!(next_status_filter(None), Some(DemoListStatus::Planned));
        assert_eq!(
            next_status_filter(Some(DemoListStatus::Broken)),
            Some(DemoListStatus::Missing)
        );
        assert_eq!(next_status_filter(Some(DemoListStatus::Missing)), None);
    }

    #[test]
    fn browser_gap_filter_cycle_is_bounded() {
        assert_eq!(next_gap_filter(None), Some(DemoListGap::Existing));
        assert_eq!(
            next_gap_filter(Some(DemoListGap::Broken)),
            Some(DemoListGap::Stale)
        );
        assert_eq!(next_gap_filter(Some(DemoListGap::Stale)), None);
    }

    #[test]
    fn browser_query_summary_is_human_readable() {
        let query = DemoListQuery {
            search: Some("auth".to_owned()),
            owner: Some("signal".to_owned()),
            tag: Some("self-hosted".to_owned()),
            mode: Some(DemoListMode::Interactive),
            cover: Some("effigy.demo.lifecycle".to_owned()),
            status: Some(DemoListStatus::Ready),
            gap: Some(DemoListGap::Existing),
            stale_only: true,
            ..DemoListQuery::default()
        };
        assert_eq!(
            query_summary(&query),
            "search=auth, owner=signal, tag=self-hosted, mode=interactive, cover=effigy.demo.lifecycle, status=ready, gap=existing, stale-only=true"
        );
        assert_eq!(query_summary(&DemoListQuery::default()), "none");
    }

    #[test]
    fn browser_detail_lines_use_compact_sections() {
        let mut detail =
            detail_with_artifacts(&[".effigy/demo/artifacts/browser-proof-report/index.html"]);
        detail.id = "browser-proof-report".to_owned();
        detail.title = "Browser Proof Report".to_owned();
        detail.summary = "Generate a human-checkable proof report.".to_owned();
        detail.proof = "Verify the browser-facing proof path stays inspectable.".to_owned();
        detail.owner = "effigy".to_owned();
        detail.covers = vec!["effigy.demo.browser".to_owned()];
        detail.tags = vec!["self-hosted".to_owned(), "proof".to_owned()];
        detail.latest_attempt.summary = Some("Latest attempt wrote a proof report.".to_owned());

        let rendered = detail_lines(Path::new("/tmp/effigy"), &detail, 0, true)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Browser Proof Report"));
        assert!(rendered.contains("Summary"));
        assert!(rendered.contains("Generate a human-checkable proof report."));
        assert!(rendered.contains("tags: self-hosted, proof"));
        assert!(rendered.contains("Result"));
        assert!(rendered.contains("status: passed"));
        assert!(rendered.contains("Latest attempt wrote a proof report."));
        assert!(rendered.contains("History"));
        assert!(rendered.contains("command: effigy demo history browser-proof-report"));
        assert!(rendered.contains("Open Actions and choose Open history"));
        assert!(rendered.contains(".effigy/demo/artifacts/browser-proof-report/index.html"));
        assert!(rendered.contains("covers: effigy.demo.browser"));
        assert!(rendered.contains("↑/↓ selects artifact"));
        assert!(!rendered.contains("Latest Receipt"));
        assert!(!rendered.contains("actions:"));
        assert!(!rendered.contains("attempts:"));
    }

    #[test]
    fn browser_detail_lines_hide_pointer_when_inactive() {
        let detail = detail_with_artifacts(&["one", "two"]);

        let rendered = detail_lines(Path::new("/tmp/effigy"), &detail, 0, false)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("→ focuses artifacts"));
        assert!(!rendered.contains("› one"));
    }

    #[test]
    fn browser_action_menu_exposes_history_handoff_label() {
        assert_eq!(ActionMenuItem::OpenHistory.label(), "Open history");
    }
}
