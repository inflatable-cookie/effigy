use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command as ProcessCommand, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::line;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use vt100::Parser as VtParser;

use crate::runner::{run_command, RunnerError};
use crate::tui::core::{
    effigy_panel_block, next_index, prev_index, EFFIGY_ACCENT, EFFIGY_ACCENT_SOFT, EFFIGY_MUTED,
};
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
    query: DemoListQuery,
    rows: Vec<BrowserRow>,
    focus: BrowserFocus,
    selected_demo_id: Option<String>,
    selected_row_index: usize,
    selected_detail_entry_index: usize,
    selected_artifact_index: usize,
    selected_history_attempt_ordinal: Option<usize>,
    terminal_scroll_offset: usize,
    terminal_input_mode: bool,
    detail_tab: DetailTab,
    detail: Option<DemoDetail>,
    history: Option<DemoHistoryPayload>,
    live_terminal_session: Option<BrowserLiveTerminalSession>,
    last_reported_terminal_size: Option<(String, u16, u16)>,
    result_visible_demo_ids: HashSet<String>,
    footer_message: String,
    pending_action: Option<PendingAction>,
    last_refresh: Instant,
    total_demo_count: usize,
    overlay: Option<BrowserOverlay>,
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
            selected_detail_entry_index: 0,
            selected_artifact_index: 0,
            selected_history_attempt_ordinal: None,
            terminal_scroll_offset: 0,
            terminal_input_mode: false,
            detail_tab: DetailTab::Overview,
            detail: None,
            history: None,
            live_terminal_session: None,
            last_reported_terminal_size: None,
            result_visible_demo_ids: HashSet::new(),
            footer_message: "Loading demo registry...".to_owned(),
            pending_action: None,
            last_refresh: Instant::now() - Duration::from_secs(5),
            total_demo_count: 0,
            overlay: None,
        }
    }

    fn run(&mut self, terminal: &mut BrowserTerminal) -> Result<(), RunnerError> {
        self.refresh_state()?;
        loop {
            self.poll_pending_action();
            self.poll_live_terminal_session()?;
            terminal
                .draw(|frame| self.render(frame))
                .map_err(|error| RunnerError::Ui(error.to_string()))?;

            if event::poll(Duration::from_millis(125))
                .map_err(|error| RunnerError::Ui(error.to_string()))?
            {
                match event::read().map_err(|error| RunnerError::Ui(error.to_string()))? {
                    Event::Key(key) => {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }
                        if self.handle_key(key)? {
                            break;
                        }
                    }
                    Event::Resize(cols, rows) => self.handle_resize_event(cols, rows)?,
                    _ => {}
                }
            } else if self.last_refresh.elapsed() >= Duration::from_millis(750) {
                self.refresh_state()?;
            }
        }
        self.shutdown_live_terminal_session()?;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool, RunnerError> {
        if self.handle_overlay_key(key.code)? {
            return Ok(false);
        }
        if self.handle_terminal_input_key(&key)? {
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc => return Ok(self.handle_escape_key()),
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Down => self.handle_down_key(),
            KeyCode::Up => self.handle_up_key(),
            KeyCode::Right => self.handle_right_key()?,
            KeyCode::Left => self.handle_left_key()?,
            KeyCode::Tab | KeyCode::BackTab => self.toggle_focus_panel(),
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

    fn handle_escape_key(&mut self) -> bool {
        if self.terminal_input_mode {
            self.terminal_input_mode = false;
            self.footer_message = "Terminal input capture disabled.".to_owned();
            return false;
        }
        if matches!(self.detail_tab, DetailTab::Overview) {
            true
        } else {
            let _ = self.set_detail_tab(DetailTab::Overview);
            false
        }
    }

    fn handle_resize_event(&mut self, cols: u16, rows: u16) -> Result<(), RunnerError> {
        self.sync_active_terminal_resize_for_viewport(browser_terminal_viewport_size(cols, rows))
    }

    fn handle_down_key(&mut self) {
        match self.focus {
            BrowserFocus::List => self.select_next_demo(),
            BrowserFocus::Detail => {
                if matches!(self.detail_tab, DetailTab::Terminal) {
                    self.terminal_scroll_offset = self.terminal_scroll_offset.saturating_add(1);
                    self.footer_message =
                        format!("Terminal scroll offset: {}.", self.terminal_scroll_offset);
                } else {
                    self.select_next_detail_entry();
                }
            }
        }
    }

    fn handle_up_key(&mut self) {
        match self.focus {
            BrowserFocus::List => self.select_previous_demo(),
            BrowserFocus::Detail => {
                if matches!(self.detail_tab, DetailTab::Terminal) {
                    self.terminal_scroll_offset = self.terminal_scroll_offset.saturating_sub(1);
                    self.footer_message =
                        format!("Terminal scroll offset: {}.", self.terminal_scroll_offset);
                } else {
                    self.select_previous_detail_entry();
                }
            }
        }
    }

    fn handle_enter_key(&mut self) -> Result<(), RunnerError> {
        match self.focus {
            BrowserFocus::List => {
                self.open_action_overlay();
                Ok(())
            }
            BrowserFocus::Detail => {
                if matches!(self.detail_tab, DetailTab::Terminal) {
                    self.toggle_terminal_input_mode()
                } else {
                    self.dispatch_selected_detail_entry()
                }
            }
        }
    }

    fn handle_terminal_input_key(&mut self, key: &KeyEvent) -> Result<bool, RunnerError> {
        if !self.terminal_input_mode {
            return Ok(false);
        }
        if key.code == KeyCode::Esc {
            self.terminal_input_mode = false;
            self.footer_message = "Terminal input capture disabled.".to_owned();
            return Ok(true);
        }
        let Some(payload) = browser_terminal_key_input(key) else {
            self.footer_message = "That key is not forwarded in terminal input mode.".to_owned();
            return Ok(true);
        };
        self.forward_terminal_input(&payload)?;
        Ok(true)
    }

    fn handle_right_key(&mut self) -> Result<(), RunnerError> {
        match self.focus {
            BrowserFocus::List => {
                self.footer_message =
                    "List panel focused. Tab switches to detail. ↑/↓ selects demos.".to_owned();
                Ok(())
            }
            BrowserFocus::Detail => self.select_next_detail_tab(),
        }
    }

    fn handle_left_key(&mut self) -> Result<(), RunnerError> {
        match self.focus {
            BrowserFocus::List => {
                self.footer_message =
                    "List panel focused. Tab switches to detail. ↑/↓ selects demos.".to_owned();
                Ok(())
            }
            BrowserFocus::Detail => self.select_previous_detail_tab(),
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
        action_menu_items_for_detail(detail)
    }

    fn run_action_menu_item(&mut self, item: ActionMenuItem) -> Result<(), RunnerError> {
        match item {
            ActionMenuItem::Run | ActionMenuItem::Rerun => self.dispatch_run_or_rerun(),
            ActionMenuItem::Stop => self.dispatch_stop(),
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

    fn selected_history_attempt(&self) -> Option<&DemoHistoryAttempt> {
        let history = self.history.as_ref()?;
        selected_history_attempt(history, self.selected_history_attempt_ordinal)
    }

    fn detail_render(&self) -> DetailRender {
        let detail_focused = matches!(self.focus, BrowserFocus::Detail);
        let selected_item = self.selected_detail_item();
        match (&self.detail, self.detail_tab) {
            (Some(detail), DetailTab::Overview) => overview_detail_render(
                detail,
                selected_item,
                detail_focused,
                self.result_visible_demo_ids.contains(&detail.id),
            ),
            (Some(detail), DetailTab::History) => {
                history_detail_render(detail, self.history.as_ref(), selected_item, detail_focused)
            }
            (Some(detail), DetailTab::Terminal) => {
                terminal_detail_render(&self.repo_root, detail, selected_item, detail_focused)
            }
            (Some(detail), DetailTab::Artifacts) => {
                artifacts_detail_render(detail, selected_item, detail_focused)
            }
            (None, _) => DetailRender {
                lines: vec![
                    Line::from("No demo selected."),
                    Line::from(""),
                    Line::from("Use ↑/↓ in the list to select a demo."),
                ],
                selected_line_index: None,
            },
        }
    }

    fn detail_selectable_items(&self) -> Vec<DetailSelectableItem> {
        let Some(detail) = self.selected_detail() else {
            return Vec::new();
        };

        match self.detail_tab {
            DetailTab::Overview => self
                .action_menu_items()
                .into_iter()
                .map(DetailSelectableItem::Action)
                .collect::<Vec<_>>(),
            DetailTab::History => {
                let mut items = vec![DetailSelectableItem::HistoryRefresh];
                if let Some(history) = &self.history {
                    items.extend(
                        history
                            .attempt_history
                            .attempts
                            .iter()
                            .map(|attempt| DetailSelectableItem::HistoryAttempt(attempt.ordinal)),
                    );
                }
                items
            }
            DetailTab::Terminal => Vec::new(),
            DetailTab::Artifacts => detail
                .latest_attempt
                .artifacts
                .iter()
                .enumerate()
                .map(|(index, _)| DetailSelectableItem::Artifact(index))
                .collect::<Vec<_>>(),
        }
    }

    fn selected_detail_item(&self) -> Option<DetailSelectableItem> {
        let items = self.detail_selectable_items();
        items
            .get(self.selected_detail_entry_index)
            .copied()
            .or_else(|| items.first().copied())
    }

    fn focus_list(&mut self) {
        self.focus = BrowserFocus::List;
        self.footer_message =
            "List panel focused. ↑/↓ selects demos. Tab switches to detail. Enter opens actions."
                .to_owned();
    }

    fn focus_detail(&mut self) {
        self.focus = BrowserFocus::Detail;
        self.footer_message =
            "Detail panel focused. ←/→ switches views. ↑/↓ selects visible entries. Enter activates the selected option.".to_owned();
    }

    fn toggle_focus_panel(&mut self) {
        match self.focus {
            BrowserFocus::List => self.focus_detail(),
            BrowserFocus::Detail => self.focus_list(),
        }
    }

    fn select_next_detail_tab(&mut self) -> Result<(), RunnerError> {
        if self.selected_detail().is_none() {
            return Ok(());
        }
        self.set_detail_tab(self.detail_tab.next())
    }

    fn select_previous_detail_tab(&mut self) -> Result<(), RunnerError> {
        if self.selected_detail().is_none() {
            return Ok(());
        }
        self.set_detail_tab(self.detail_tab.previous())
    }

    fn set_detail_tab(&mut self, next_tab: DetailTab) -> Result<(), RunnerError> {
        self.terminal_input_mode = false;
        self.terminal_scroll_offset = 0;
        self.detail_tab = next_tab;
        self.selected_detail_entry_index = 0;
        if matches!(self.detail_tab, DetailTab::History) {
            if let Some(demo_id) = self.selected_demo_id().map(str::to_owned) {
                self.history = Some(fetch_demo_history(&self.repo_root, &demo_id)?);
                self.selected_history_attempt_ordinal = self
                    .selected_history_attempt()
                    .map(|attempt| attempt.ordinal);
            }
        }
        self.sync_selected_detail_entry();
        self.footer_message = match self.detail_tab {
            DetailTab::Overview => "Viewing Overview tab.".to_owned(),
            DetailTab::History => "Viewing History tab.".to_owned(),
            DetailTab::Terminal => "Viewing Terminal tab.".to_owned(),
            DetailTab::Artifacts => "Viewing Artifacts tab.".to_owned(),
        };
        if matches!(self.detail_tab, DetailTab::Terminal) {
            self.sync_active_terminal_resize_for_current_view()?;
        }
        Ok(())
    }

    fn select_next_detail_entry(&mut self) {
        let items = self.detail_selectable_items();
        if items.is_empty() {
            self.footer_message = "No demo is currently selected.".to_owned();
            return;
        }
        self.selected_detail_entry_index =
            next_index(self.selected_detail_entry_index, items.len());
        self.sync_selected_detail_entry();
    }

    fn select_previous_detail_entry(&mut self) {
        let items = self.detail_selectable_items();
        if items.is_empty() {
            self.footer_message = "No demo is currently selected.".to_owned();
            return;
        }
        self.selected_detail_entry_index =
            prev_index(self.selected_detail_entry_index, items.len());
        self.sync_selected_detail_entry();
    }

    fn dispatch_run_or_rerun(&mut self) -> Result<(), RunnerError> {
        if self.pending_action.is_some() {
            self.footer_message =
                "A demo run or rerun is already in flight. Stop or wait for it first.".to_owned();
            return Ok(());
        }
        if self
            .live_terminal_session
            .as_ref()
            .is_some_and(BrowserLiveTerminalSession::is_running)
        {
            self.footer_message =
                "A live browser terminal session is already in flight. Stop or wait for it first."
                    .to_owned();
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
        if detail_prefers_live_browser_terminal(&detail, &subcommand) {
            let session = BrowserLiveTerminalSession::spawn(
                self.repo_root.clone(),
                detail.id.clone(),
                subcommand,
            )?;
            self.live_terminal_session = Some(session);
            self.result_visible_demo_ids.insert(detail.id.clone());
            self.focus = BrowserFocus::Detail;
            self.set_detail_tab(DetailTab::Terminal)?;
            self.footer_message = format!(
                "Started live terminal {action_label} for demo `{}`.",
                detail.id
            );
            self.last_refresh = Instant::now() - Duration::from_secs(5);
            return Ok(());
        }
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
        self.result_visible_demo_ids.insert(demo_id.clone());
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

    fn refresh_history_mode(&mut self) -> Result<(), RunnerError> {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(());
        };
        let history = fetch_demo_history(&self.repo_root, &detail.id)?;
        self.history = Some(history);
        self.sync_selected_detail_entry();
        self.footer_message = "Refreshed retained history in the detail pane.".to_owned();
        Ok(())
    }

    fn toggle_terminal_input_mode(&mut self) -> Result<(), RunnerError> {
        if self.selected_live_terminal_session().is_some() {
            self.terminal_input_mode = !self.terminal_input_mode;
            self.footer_message = if self.terminal_input_mode {
                "Live terminal input capture enabled. Typed keys go directly to the demo. Esc exits input mode."
                    .to_owned()
            } else {
                "Terminal input capture disabled.".to_owned()
            };
            return Ok(());
        }
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(());
        };
        let session = &detail.active_terminal_session;
        if !session.available {
            self.footer_message = "No active terminal session is available for input.".to_owned();
            return Ok(());
        }
        if !session.supports_input_forwarding {
            self.footer_message = session
                .input_forwarding_reason
                .clone()
                .unwrap_or_else(|| "Terminal input forwarding is unavailable.".to_owned());
            return Ok(());
        }
        self.terminal_input_mode = !self.terminal_input_mode;
        self.footer_message = if self.terminal_input_mode {
            "Terminal input capture enabled. Typed keys go to the demo. Esc exits input mode."
                .to_owned()
        } else {
            "Terminal input capture disabled.".to_owned()
        };
        Ok(())
    }

    fn forward_terminal_input(&mut self, text: &str) -> Result<(), RunnerError> {
        if let Some(session) = self.selected_live_terminal_session_mut() {
            session.write_input(text.as_bytes())?;
            self.footer_message = format!(
                "Forwarded live terminal input to demo `{}`.",
                session.demo_id
            );
            return Ok(());
        }
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(());
        };
        let demo_id = detail.id.clone();
        let _ = invoke_demo_json(
            &self.repo_root,
            DemoArgs {
                subcommand: DemoSubcommand::Input {
                    demo_id: demo_id.clone(),
                    text: text.to_owned(),
                    append_newline: false,
                },
                repo_override: Some(self.repo_root.clone()),
                output_json: true,
            },
        )?;
        self.footer_message = format!("Forwarded terminal input to demo `{demo_id}`.");
        self.last_refresh = Instant::now() - Duration::from_secs(5);
        Ok(())
    }

    fn dispatch_selected_detail_entry(&mut self) -> Result<(), RunnerError> {
        let Some(item) = self.selected_detail_item() else {
            self.footer_message = "No detail action is available for the selected demo.".to_owned();
            return Ok(());
        };
        match item {
            DetailSelectableItem::Action(action) => self.run_action_menu_item(action),
            DetailSelectableItem::Artifact(index) => {
                self.selected_artifact_index = index;
                self.dispatch_open_artifact()
            }
            DetailSelectableItem::HistoryRefresh => self.refresh_history_mode(),
            DetailSelectableItem::HistoryAttempt(ordinal) => {
                self.selected_history_attempt_ordinal = Some(ordinal);
                self.footer_message =
                    format!("Viewing retained attempt #{ordinal} in the detail pane.");
                Ok(())
            }
        }
    }

    fn sync_selected_detail_entry(&mut self) {
        let items = self.detail_selectable_items();
        if items.is_empty() {
            self.selected_detail_entry_index = 0;
            return;
        }
        self.selected_detail_entry_index = self.selected_detail_entry_index.min(items.len() - 1);
        match items[self.selected_detail_entry_index] {
            DetailSelectableItem::Action(action) => {
                self.footer_message = format!("Selected detail action `{}`.", action.label());
            }
            DetailSelectableItem::Artifact(index) => {
                self.selected_artifact_index = index;
                if let Some(artifact) = self.selected_artifact() {
                    self.footer_message = format!("Selected artifact `{artifact}`.");
                }
            }
            DetailSelectableItem::HistoryRefresh => {
                self.footer_message = "Selected history action `Refresh history`.".to_owned();
            }
            DetailSelectableItem::HistoryAttempt(ordinal) => {
                self.selected_history_attempt_ordinal = Some(ordinal);
                self.footer_message =
                    format!("Selected retained attempt #{ordinal} in the detail pane.");
            }
        }
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

    fn poll_live_terminal_session(&mut self) -> Result<(), RunnerError> {
        let mut finished = None;
        if let Some(session) = self.live_terminal_session.as_mut() {
            session.drain_output();
            finished = session.poll_exit()?;
        }
        if let Some((demo_id, success)) = finished {
            self.terminal_input_mode = false;
            self.refresh_state()?;
            self.footer_message = if success {
                format!("Live terminal session for demo `{demo_id}` completed.")
            } else {
                format!("Live terminal session for demo `{demo_id}` ended.")
            };
            self.last_refresh = Instant::now();
        }
        Ok(())
    }

    fn shutdown_live_terminal_session(&mut self) -> Result<(), RunnerError> {
        let Some(mut session) = self.live_terminal_session.take() else {
            return Ok(());
        };
        if !session.is_running() {
            return Ok(());
        }
        let demo_id = session.demo_id.clone();
        let _ = invoke_demo_json(
            &self.repo_root,
            DemoArgs {
                subcommand: DemoSubcommand::Stop {
                    demo_id: demo_id.clone(),
                },
                repo_override: Some(self.repo_root.clone()),
                output_json: true,
            },
        );
        session.finish_after_stop_request()?;
        Ok(())
    }

    fn selected_live_terminal_session(&self) -> Option<&BrowserLiveTerminalSession> {
        let demo_id = self.selected_demo_id()?;
        self.live_terminal_session
            .as_ref()
            .filter(|session| session.demo_id == demo_id)
    }

    fn selected_live_terminal_session_mut(&mut self) -> Option<&mut BrowserLiveTerminalSession> {
        let demo_id = self.selected_demo_id()?.to_owned();
        self.live_terminal_session
            .as_mut()
            .filter(|session| session.demo_id == demo_id)
    }

    fn refresh_state(&mut self) -> Result<(), RunnerError> {
        let previous_selected_demo_id = self.selected_demo_id.clone();
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
        if self.selected_demo_id != previous_selected_demo_id {
            self.detail_tab = DetailTab::Overview;
            self.selected_detail_entry_index = 0;
            self.selected_history_attempt_ordinal = None;
            self.terminal_scroll_offset = 0;
            self.terminal_input_mode = false;
            self.history = None;
            self.last_reported_terminal_size = None;
        } else if matches!(self.detail_tab, DetailTab::History) && self.selected_demo_id.is_some() {
            self.history = Some(fetch_demo_history(
                &self.repo_root,
                self.selected_demo_id
                    .as_deref()
                    .expect("selected demo id exists in history mode"),
            )?);
        }
        self.selected_artifact_index = self.detail.as_ref().map_or(0, |detail| {
            clamp_artifact_index(self.selected_artifact_index, detail)
        });
        self.sync_selected_detail_entry();

        self.last_refresh = Instant::now();
        self.sync_active_terminal_resize_for_current_view()?;
        Ok(())
    }

    fn sync_active_terminal_resize_for_current_view(&mut self) -> Result<(), RunnerError> {
        let Ok((cols, rows)) = crossterm::terminal::size() else {
            return Ok(());
        };
        self.sync_active_terminal_resize_for_viewport(browser_terminal_viewport_size(cols, rows))
    }

    fn sync_active_terminal_resize_for_viewport(
        &mut self,
        (cols, rows): (u16, u16),
    ) -> Result<(), RunnerError> {
        if !matches!(self.detail_tab, DetailTab::Terminal) {
            return Ok(());
        }
        let Some(detail) = self.selected_detail() else {
            return Ok(());
        };
        let session = &detail.active_terminal_session;
        if !session.available || !session.resize.available {
            return Ok(());
        }
        let next = (detail.id.clone(), cols, rows);
        if self.last_reported_terminal_size.as_ref() == Some(&next) {
            return Ok(());
        }
        let demo_id = detail.id.clone();
        let _ = invoke_demo_json(
            &self.repo_root,
            DemoArgs {
                subcommand: DemoSubcommand::Resize {
                    demo_id: demo_id.clone(),
                    cols,
                    rows,
                },
                repo_override: Some(self.repo_root.clone()),
                output_json: true,
            },
        )?;
        self.last_reported_terminal_size = Some(next);
        self.last_refresh = Instant::now() - Duration::from_secs(5);
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
            .highlight_style(selected_list_highlight_style(list_focused))
            .highlight_symbol(selected_list_highlight_symbol())
            .repeat_highlight_symbol(true);
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_detail(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if matches!(self.detail_tab, DetailTab::Terminal) {
            self.render_terminal_detail(frame, area);
            return;
        }
        let mut render = self.detail_render();
        if self.detail.is_some() {
            let tab_lines = detail_tab_lines(
                self.detail_tab,
                matches!(self.focus, BrowserFocus::Detail),
                area.width.saturating_sub(2) as usize,
            );
            let tab_line_count = tab_lines.len();
            render.lines.splice(0..0, tab_lines);
            if let Some(selected_line_index) = &mut render.selected_line_index {
                *selected_line_index += tab_line_count;
            }
        }
        let inner_height = area.height.saturating_sub(2) as usize;
        let scroll = render
            .selected_line_index
            .map(|line_index| {
                if inner_height == 0 {
                    0
                } else {
                    line_index.saturating_sub(inner_height.saturating_sub(1)) as u16
                }
            })
            .unwrap_or(0);
        let panel_title = self
            .selected_detail()
            .map(|detail| format!(" {} ", detail.title))
            .unwrap_or_else(|| " Demo ".to_owned());
        let detail = Paragraph::new(render.lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(ratatui::symbols::border::ROUNDED)
                    .border_style(Style::default().fg(
                        if matches!(self.focus, BrowserFocus::Detail) {
                            EFFIGY_ACCENT
                        } else {
                            Color::DarkGray
                        },
                    ))
                    .title_top(
                        Line::from(Span::styled(
                            panel_title,
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ))
                        .left_aligned(),
                    ),
            )
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(detail, area);
    }

    fn render_terminal_detail(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let panel_title = self
            .selected_detail()
            .map(|detail| format!(" {} ", detail.title))
            .unwrap_or_else(|| " Demo ".to_owned());
        let border_color = if matches!(self.focus, BrowserFocus::Detail) {
            EFFIGY_ACCENT
        } else {
            Color::DarkGray
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ratatui::symbols::border::ROUNDED)
            .border_style(Style::default().fg(border_color))
            .title_top(
                Line::from(Span::styled(
                    panel_title,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
                .left_aligned(),
            );
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let Some(detail) = self.selected_detail().cloned() else {
            frame.render_widget(Paragraph::new(vec![Line::from("No demo selected.")]), inner);
            return;
        };

        let tab_lines = detail_tab_lines(
            self.detail_tab,
            matches!(self.focus, BrowserFocus::Detail),
            inner.width as usize,
        );
        let tab_height = tab_lines.len() as u16;
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(tab_height),
                Constraint::Length(2),
                Constraint::Min(1),
            ])
            .split(inner);

        frame.render_widget(Paragraph::new(tab_lines), layout[0]);

        let terminal_view = if let Some(session) = self.selected_live_terminal_session() {
            build_live_terminal_view(
                session,
                layout[2].width as usize,
                layout[2].height as usize,
                self.terminal_scroll_offset,
            )
        } else {
            build_terminal_view(
                &self.repo_root,
                &detail,
                layout[2].width as usize,
                layout[2].height as usize,
                self.terminal_scroll_offset,
            )
        };
        self.terminal_scroll_offset = terminal_view.scroll_offset;
        let status_lines = if let Some(session) = self.selected_live_terminal_session() {
            live_terminal_status_lines(&detail, &terminal_view, self.terminal_input_mode, session)
        } else {
            terminal_status_lines(&detail, &terminal_view, self.terminal_input_mode)
        };

        frame.render_widget(
            Paragraph::new(status_lines).wrap(Wrap { trim: false }),
            layout[1],
        );
        frame.render_widget(
            Paragraph::new(terminal_view.lines).wrap(Wrap { trim: false }),
            layout[2],
        );
    }

    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let help = browser_help_line(self.focus, self.detail_tab, self.terminal_input_mode);
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

fn fetch_demo_history(repo_root: &Path, demo_id: &str) -> Result<DemoHistoryPayload, RunnerError> {
    let payload = invoke_demo_json(
        repo_root,
        DemoArgs {
            subcommand: DemoSubcommand::History {
                demo_id: demo_id.to_owned(),
                limit: None,
                outcome: None,
                attempt_id: None,
                attempt_ordinal: None,
            },
            repo_override: Some(repo_root.to_path_buf()),
            output_json: true,
        },
    )?;
    serde_json::from_value(payload).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "failed to parse demo history payload for browser: {error}"
        ))
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

fn detail_prefers_live_browser_terminal(detail: &DemoDetail, subcommand: &DemoSubcommand) -> bool {
    matches!(
        subcommand,
        DemoSubcommand::Run { .. } | DemoSubcommand::Rerun { .. }
    ) && detail
        .runtime_backend
        .capabilities
        .iter()
        .any(|capability| capability == "browser-live-attach")
        && !detail.active_terminal_session.nested_tui
        && matches!(detail.mode.as_str(), "interactive" | "hybrid")
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

fn browser_help_line(
    focus: BrowserFocus,
    detail_tab: DetailTab,
    terminal_input_mode: bool,
) -> Line<'static> {
    if terminal_input_mode {
        return Line::from(vec![
            Span::styled(" type ", key_style()),
            Span::raw("send keys  "),
            Span::styled(" Enter ", key_style()),
            Span::raw("newline  "),
            Span::styled(" Esc ", key_style()),
            Span::raw("leave input"),
        ]);
    }

    if matches!(focus, BrowserFocus::Detail) && matches!(detail_tab, DetailTab::Terminal) {
        return Line::from(vec![
            Span::styled(" ↑↓ ", key_style()),
            Span::raw("scroll  "),
            Span::styled(" ←→ ", key_style()),
            Span::raw("tab  "),
            Span::styled(" Enter ", key_style()),
            Span::raw("input mode  "),
            Span::styled(" Tab ", key_style()),
            Span::raw("panel  "),
            Span::styled(" Esc ", key_style()),
            Span::raw("back/quit"),
        ]);
    }

    Line::from(vec![
        Span::styled(" ↑↓ ", key_style()),
        Span::raw("move  "),
        Span::styled(" ←→ ", key_style()),
        Span::raw("view  "),
        Span::styled(" Tab ", key_style()),
        Span::raw("panel  "),
        Span::styled(" Enter ", key_style()),
        Span::raw("act/open  "),
        Span::styled(" / ", key_style()),
        Span::raw("search  "),
        Span::styled(" f ", key_style()),
        Span::raw("filters  "),
        Span::styled(" Esc ", key_style()),
        Span::raw("back/quit"),
    ])
}

fn selected_list_highlight_style(list_focused: bool) -> Style {
    if list_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(EFFIGY_ACCENT_SOFT)
            .add_modifier(Modifier::BOLD)
    }
}

fn selected_list_highlight_symbol() -> &'static str {
    "▌"
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

fn overview_detail_render(
    detail: &DemoDetail,
    selected_item: Option<DetailSelectableItem>,
    detail_focused: bool,
    show_result: bool,
) -> DetailRender {
    let mut lines = Vec::new();
    let mut selected_line_index = None;

    if !detail.tags.is_empty() {
        lines.push(muted_line(format!("tags: {}", detail.tags.join(", "))));
    }
    if !detail.covers.is_empty() {
        lines.push(compact_kv_line("covers", &detail.covers.join(", ")));
    }

    lines.extend([
        Line::from(""),
        section_heading("Summary"),
        Line::from(detail.summary.clone()),
        Line::from(""),
        section_heading("Proof"),
        Line::from(detail.proof.clone()),
    ]);
    lines.push(Line::from(""));
    lines.push(section_heading("Actions"));
    for action in action_menu_items_for_detail(detail) {
        let current_item = DetailSelectableItem::Action(action);
        if selected_item == Some(current_item) {
            selected_line_index = Some(lines.len());
        }
        lines.push(selectable_detail_line(
            action.label(),
            selected_item == Some(current_item),
            detail_focused,
        ));
    }
    if show_result && detail.latest_attempt.recorded {
        lines.push(Line::from(""));
        lines.push(section_heading("Result"));
        lines.push(compact_kv_line("status", &detail.latest_attempt.state));
        if let Some(summary) = &detail.latest_attempt.summary {
            lines.push(Line::from(summary.clone()));
        }
    }
    DetailRender {
        lines,
        selected_line_index,
    }
}

fn history_detail_render(
    _detail: &DemoDetail,
    history: Option<&DemoHistoryPayload>,
    selected_item: Option<DetailSelectableItem>,
    detail_focused: bool,
) -> DetailRender {
    let mut lines = Vec::new();
    let mut selected_line_index = None;

    lines.push(section_heading("Actions"));

    {
        let (label, item) = ("Refresh history", DetailSelectableItem::HistoryRefresh);
        if selected_item == Some(item) {
            selected_line_index = Some(lines.len());
        }
        lines.push(selectable_detail_line(
            label,
            selected_item == Some(item),
            detail_focused,
        ));
    }

    lines.push(Line::from(""));
    lines.push(section_heading("Retained Attempts"));
    match history {
        Some(history) => {
            if let Some(parse_error) = &history.attempt_history.parse_error {
                lines.push(muted_line(format!("history parse error: {parse_error}")));
            }
            if history.attempt_history.attempts.is_empty() {
                lines.push(muted_line("No retained attempts were recorded.".to_owned()));
            } else {
                for attempt in &history.attempt_history.attempts {
                    let item = DetailSelectableItem::HistoryAttempt(attempt.ordinal);
                    if selected_item == Some(item) {
                        selected_line_index = Some(lines.len());
                    }
                    lines.push(selectable_detail_line(
                        &format!(
                            "#{:02}  {:<10} {}",
                            attempt.ordinal,
                            attempt.outcome,
                            attempt
                                .summary
                                .as_deref()
                                .unwrap_or("No retained summary recorded.")
                        ),
                        selected_item == Some(item),
                        detail_focused,
                    ));
                }
            }
        }
        None => lines.push(muted_line("Retained history is not loaded.".to_owned())),
    }

    lines.push(Line::from(""));
    lines.push(section_heading("Selected Attempt"));
    if let Some(attempt) = history.and_then(|history| {
        selected_history_attempt(history, selected_history_ordinal_from_item(selected_item))
    }) {
        lines.extend(stacked_kv_lines("ordinal", &attempt.ordinal.to_string()));
        lines.extend(stacked_kv_lines("attempt", &attempt.attempt_id));
        lines.extend(stacked_kv_lines("outcome", &attempt.outcome));
        if let Some(summary) = &attempt.summary {
            lines.push(Line::from(summary.clone()));
        }
        if let Some(receipt_path) = &attempt.receipt_path {
            lines.extend(stacked_kv_lines("receipt", receipt_path));
        }
        if let Some(stdout_log_path) = &attempt.stdout_log_path {
            lines.extend(stacked_kv_lines("stdout", stdout_log_path));
        }
        if let Some(stderr_log_path) = &attempt.stderr_log_path {
            lines.extend(stacked_kv_lines("stderr", stderr_log_path));
        }
        if let Some(exit_code) = attempt.exit_code {
            lines.extend(stacked_kv_lines("exit", &exit_code.to_string()));
        }
        if attempt.artifacts.is_empty() {
            lines.push(muted_line(
                "No retained artifacts for the selected attempt.".to_owned(),
            ));
        } else {
            lines.extend(stacked_kv_lines("artifacts", &attempt.artifacts.join(", ")));
        }
    } else {
        lines.push(muted_line(
            "Select a retained attempt to inspect its normalized receipt and log references."
                .to_owned(),
        ));
    }

    DetailRender {
        lines,
        selected_line_index,
    }
}

fn terminal_detail_render(
    repo_root: &Path,
    detail: &DemoDetail,
    _selected_item: Option<DetailSelectableItem>,
    _detail_focused: bool,
) -> DetailRender {
    let terminal_view = build_terminal_view(repo_root, detail, 80, 18, 0);
    let mut lines = terminal_status_lines(detail, &terminal_view, false);
    lines.extend(terminal_view.lines);
    DetailRender {
        lines,
        selected_line_index: None,
    }
}

fn artifacts_detail_render(
    detail: &DemoDetail,
    selected_item: Option<DetailSelectableItem>,
    detail_focused: bool,
) -> DetailRender {
    let mut lines = Vec::new();
    let mut selected_line_index = None;

    if detail.latest_attempt.artifacts.is_empty() {
        lines.push(muted_line("No recorded artifacts.".to_owned()));
    } else {
        for (index, artifact) in detail.latest_attempt.artifacts.iter().enumerate() {
            let current_item = DetailSelectableItem::Artifact(index);
            if selected_item == Some(current_item) {
                selected_line_index = Some(lines.len());
            }
            lines.push(selectable_detail_line(
                artifact,
                selected_item == Some(current_item),
                detail_focused,
            ));
        }
    }

    DetailRender {
        lines,
        selected_line_index,
    }
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

const DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK: usize = 2000;
const DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT: usize = 8;
const DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalView {
    lines: Vec<Line<'static>>,
    source: TerminalViewSource,
    scroll_offset: usize,
    stderr_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalViewSource {
    LiveAttached,
    ActiveLogs,
    InspectSnapshot,
    LatestAttemptLogs,
    Empty,
}

impl TerminalViewSource {
    fn label(self) -> &'static str {
        match self {
            TerminalViewSource::LiveAttached => "live attached",
            TerminalViewSource::ActiveLogs => "live terminal",
            TerminalViewSource::InspectSnapshot => "inspect snapshot",
            TerminalViewSource::LatestAttemptLogs => "latest attempt logs",
            TerminalViewSource::Empty => "none",
        }
    }
}

fn build_terminal_view(
    repo_root: &Path,
    detail: &DemoDetail,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> TerminalView {
    let width = width.max(1);
    let height = height.max(1);

    if detail.active_terminal_session.available {
        let session = &detail.active_terminal_session;
        let source = terminal_stream_source(
            repo_root,
            session.stdout_log_path.as_deref(),
            session.stderr_log_path.as_deref(),
            session.recent_output.stdout_lines.clone(),
            session.recent_output.stderr_lines.clone(),
            TerminalViewSource::ActiveLogs,
            session.terminal_size.cols,
            session.terminal_size.rows,
        );
        return render_terminal_view_from_source(source, width, height, scroll_offset);
    }

    if detail.latest_attempt.output_available {
        let source = terminal_stream_source(
            repo_root,
            detail.latest_attempt.stdout_log_path.as_deref(),
            detail.latest_attempt.stderr_log_path.as_deref(),
            Vec::new(),
            Vec::new(),
            TerminalViewSource::LatestAttemptLogs,
            None,
            None,
        );
        return render_terminal_view_from_source(source, width, height, scroll_offset);
    }

    TerminalView {
        lines: vec![muted_line(
            "No active or recorded terminal output is available.".to_owned(),
        )],
        source: TerminalViewSource::Empty,
        scroll_offset: 0,
        stderr_lines: Vec::new(),
    }
}

fn build_live_terminal_view(
    session: &BrowserLiveTerminalSession,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> TerminalView {
    render_terminal_view_from_bytes(
        &session.transcript,
        TerminalViewSource::LiveAttached,
        width,
        height,
        scroll_offset,
        None,
        None,
        Vec::new(),
    )
}

struct TerminalStreamSource {
    stdout_bytes: Vec<u8>,
    stderr_lines: Vec<String>,
    source: TerminalViewSource,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
}

fn terminal_stream_source(
    repo_root: &Path,
    stdout_log_path: Option<&str>,
    stderr_log_path: Option<&str>,
    fallback_stdout_lines: Vec<String>,
    fallback_stderr_lines: Vec<String>,
    source: TerminalViewSource,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
) -> TerminalStreamSource {
    let stdout_bytes = stdout_log_path
        .map(|path| resolve_repo_relative_path(repo_root, path))
        .and_then(|path| fs::read(path).ok())
        .unwrap_or_default();
    let stderr_lines = stderr_log_path
        .map(|path| resolve_repo_relative_path(repo_root, path))
        .and_then(|path| read_recent_log_lines(&path, DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT).ok())
        .unwrap_or_default();
    let using_fallback = stdout_bytes.is_empty() && !fallback_stdout_lines.is_empty()
        || stderr_lines.is_empty() && !fallback_stderr_lines.is_empty();
    let resolved_source = if using_fallback {
        TerminalViewSource::InspectSnapshot
    } else if stdout_log_path.is_some() || stderr_log_path.is_some() {
        source
    } else {
        TerminalViewSource::InspectSnapshot
    };
    TerminalStreamSource {
        stdout_bytes: if stdout_bytes.is_empty() {
            fallback_stdout_lines.join("\n").into_bytes()
        } else {
            stdout_bytes
        },
        stderr_lines: if stderr_lines.is_empty() {
            fallback_stderr_lines
        } else {
            stderr_lines
        },
        source: resolved_source,
        terminal_cols,
        terminal_rows,
    }
}

fn render_terminal_view_from_source(
    source: TerminalStreamSource,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> TerminalView {
    render_terminal_view_from_bytes(
        &source.stdout_bytes,
        source.source,
        width,
        height,
        scroll_offset,
        source.terminal_cols,
        source.terminal_rows,
        source.stderr_lines,
    )
}

fn render_terminal_view_from_bytes(
    stdout_bytes: &[u8],
    source: TerminalViewSource,
    width: usize,
    height: usize,
    scroll_offset: usize,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
    stderr_lines: Vec<String>,
) -> TerminalView {
    let parser_rows = terminal_rows.unwrap_or(height as u16).max(1);
    let parser_cols = terminal_cols.unwrap_or(width as u16).max(1);
    let mut parser = VtParser::new(
        parser_rows,
        parser_cols,
        DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK,
    );
    if !stdout_bytes.is_empty() {
        parser.process(stdout_bytes);
    }
    let max_scroll = parser.screen().scrollback();
    let clamped_scroll = scroll_offset.min(max_scroll);
    parser.set_size(height as u16, width as u16);
    parser.set_scrollback(max_scroll.saturating_sub(clamped_scroll));
    let mut lines = parser
        .screen()
        .rows_formatted(0, width as u16)
        .map(|row| Line::from(String::from_utf8_lossy(&row).into_owned()))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(muted_line(
            "Terminal output exists, but no visible screen content is available yet.".to_owned(),
        ));
    }
    if !stderr_lines.is_empty() {
        lines.push(Line::from(""));
        lines.push(compact_kv_line("stderr", "recent lines"));
        for line in stderr_lines
            .iter()
            .take(DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT)
        {
            lines.push(Line::from(line.clone()));
        }
    }
    TerminalView {
        lines,
        source,
        scroll_offset: clamped_scroll,
        stderr_lines,
    }
}

fn terminal_status_lines(
    detail: &DemoDetail,
    terminal_view: &TerminalView,
    input_mode: bool,
) -> Vec<Line<'static>> {
    let session = &detail.active_terminal_session;
    let transport = if session.available {
        session.transport.as_str()
    } else if detail.latest_attempt.output_available {
        "recorded"
    } else {
        "none"
    };
    let input_label = if input_mode {
        "capturing keys"
    } else if session.available && session.supports_input_forwarding {
        "available"
    } else if session.available {
        session
            .input_forwarding_reason
            .as_deref()
            .unwrap_or("unavailable")
    } else {
        "inactive"
    };
    vec![
        Line::from(vec![
            Span::styled(
                " source: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(terminal_view.source.label()),
            Span::raw("   "),
            Span::styled(
                " transport: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(transport.to_owned()),
            Span::raw("   "),
            Span::styled(
                " size: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(
                session
                    .terminal_size
                    .rendered()
                    .unwrap_or_else(|| "unknown".to_owned()),
            ),
            Span::raw("   "),
            Span::styled(
                " input: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(input_label.to_owned()),
        ]),
        Line::from(""),
    ]
}

fn live_terminal_status_lines(
    detail: &DemoDetail,
    terminal_view: &TerminalView,
    input_mode: bool,
    session: &BrowserLiveTerminalSession,
) -> Vec<Line<'static>> {
    let input_label = if input_mode {
        "capturing keys"
    } else {
        "available"
    };
    let state = if session.is_running() {
        "running"
    } else {
        "complete"
    };
    vec![
        Line::from(vec![
            Span::styled(
                " source: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(terminal_view.source.label()),
            Span::raw("   "),
            Span::styled(
                " state: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(state.to_owned()),
            Span::raw("   "),
            Span::styled(
                " transport: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(if detail.mode == "interactive" || detail.mode == "hybrid" {
                "attached"
            } else {
                "stream"
            }),
            Span::raw("   "),
            Span::styled(
                " input: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(input_label.to_owned()),
        ]),
        Line::from(""),
    ]
}

fn browser_terminal_viewport_size(total_cols: u16, total_rows: u16) -> (u16, u16) {
    let area = Rect::new(0, 0, total_cols, total_rows);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(layout[1]);
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_set(ratatui::symbols::border::ROUNDED)
        .inner(body[1]);
    let tabs = detail_tab_lines(DetailTab::Terminal, true, inner.width as usize);
    let terminal_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(tabs.len() as u16),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(inner);
    (
        terminal_layout[2].width.max(1),
        terminal_layout[2].height.max(1),
    )
}

fn browser_terminal_key_input(key: &KeyEvent) -> Option<String> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(c) = key.code {
            let lower = c.to_ascii_lowercase() as u8;
            if lower.is_ascii_lowercase() {
                let value = lower - b'a' + 1;
                return Some((value as char).to_string());
            }
        }
    }

    let mapped = match key.code {
        KeyCode::Enter => "\n",
        KeyCode::Tab => "\t",
        KeyCode::Backspace => "\u{7f}",
        KeyCode::Left => "\u{1b}[D",
        KeyCode::Right => "\u{1b}[C",
        KeyCode::Up => "\u{1b}[A",
        KeyCode::Down => "\u{1b}[B",
        KeyCode::Home => "\u{1b}[H",
        KeyCode::End => "\u{1b}[F",
        KeyCode::Delete => "\u{1b}[3~",
        KeyCode::Char(c) => return Some(c.to_string()),
        _ => return None,
    };
    Some(mapped.to_owned())
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

fn stacked_kv_lines(label: &str, value: &str) -> [Line<'static>; 2] {
    [
        Line::from(vec![Span::styled(
            format!("{label}:"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("  {value}")),
    ]
}

fn section_heading(label: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        label.to_owned(),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    )])
}

fn selectable_detail_line(value: &str, selected: bool, focused: bool) -> Line<'static> {
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

fn selected_history_attempt(
    history: &DemoHistoryPayload,
    selected_ordinal: Option<usize>,
) -> Option<&DemoHistoryAttempt> {
    if history.attempt_history.attempts.is_empty() {
        return None;
    }
    selected_ordinal
        .and_then(|ordinal| {
            history
                .attempt_history
                .attempts
                .iter()
                .find(|attempt| attempt.ordinal == ordinal)
        })
        .or_else(|| history.attempt_history.attempts.first())
}

fn selected_history_ordinal_from_item(item: Option<DetailSelectableItem>) -> Option<usize> {
    match item {
        Some(DetailSelectableItem::HistoryAttempt(ordinal)) => Some(ordinal),
        _ => None,
    }
}

fn action_menu_items_for_detail(detail: &DemoDetail) -> Vec<ActionMenuItem> {
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
    items.push(ActionMenuItem::Refresh);
    items
}

fn detail_tab_lines(
    current_tab: DetailTab,
    detail_focused: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let mut spans = Vec::new();
    for (index, tab) in DetailTab::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        let style = if *tab == current_tab {
            Style::default()
                .fg(EFFIGY_ACCENT)
                .add_modifier(Modifier::BOLD)
        } else if detail_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(EFFIGY_MUTED)
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
    }

    vec![Line::from(spans), tab_border_line(detail_focused, width)]
}

fn tab_border_line(detail_focused: bool, width: usize) -> Line<'static> {
    let style = Style::default().fg(if detail_focused {
        EFFIGY_ACCENT
    } else {
        EFFIGY_MUTED
    });
    Line::from(vec![Span::styled(
        line::NORMAL.horizontal.repeat(width.max(1)),
        style,
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

struct BrowserLiveTerminalSession {
    demo_id: String,
    child: Child,
    stdin: Option<ChildStdin>,
    receiver: Receiver<LiveTerminalEvent>,
    transcript: Vec<u8>,
    exit_status: Option<ExitStatus>,
}

enum LiveTerminalEvent {
    Output(Vec<u8>),
}

impl BrowserLiveTerminalSession {
    fn spawn(
        repo_root: PathBuf,
        demo_id: String,
        subcommand: DemoSubcommand,
    ) -> Result<Self, RunnerError> {
        let executable = std::env::current_exe().map_err(|error| {
            RunnerError::Ui(format!(
                "failed to resolve current effigy executable: {error}"
            ))
        })?;
        let mut command = ProcessCommand::new(executable);
        command.current_dir(&repo_root).env("NO_COLOR", "1");
        match subcommand {
            DemoSubcommand::Run { demo_id } => {
                command.arg("demo").arg("run").arg(demo_id);
            }
            DemoSubcommand::Rerun { demo_id } => {
                command.arg("demo").arg("rerun").arg(demo_id);
            }
            _ => {
                return Err(RunnerError::Ui(
                    "live browser terminal sessions only support demo run/rerun".to_owned(),
                ))
            }
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            RunnerError::Ui(format!(
                "failed to launch live browser terminal session for `{demo_id}`: {error}"
            ))
        })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| {
            RunnerError::Ui(format!(
                "live browser terminal session for `{demo_id}` launched without stdout pipe"
            ))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            RunnerError::Ui(format!(
                "live browser terminal session for `{demo_id}` launched without stderr pipe"
            ))
        })?;
        let (sender, receiver) = mpsc::channel();
        spawn_live_terminal_reader(stdout, sender.clone());
        spawn_live_terminal_reader(stderr, sender);
        Ok(Self {
            demo_id,
            child,
            stdin,
            receiver,
            transcript: Vec::new(),
            exit_status: None,
        })
    }

    fn is_running(&self) -> bool {
        self.exit_status.is_none()
    }

    fn drain_output(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                LiveTerminalEvent::Output(bytes) => self.append_output(&bytes),
            }
        }
    }

    fn append_output(&mut self, bytes: &[u8]) {
        self.transcript.extend_from_slice(bytes);
        if self.transcript.len() > DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES {
            let excess = self.transcript.len() - DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES;
            self.transcript.drain(..excess);
        }
    }

    fn poll_exit(&mut self) -> Result<Option<(String, bool)>, RunnerError> {
        if self.exit_status.is_some() {
            return Ok(None);
        }
        let Some(status) = self.child.try_wait().map_err(|error| {
            RunnerError::Ui(format!(
                "failed to poll live browser terminal session for `{}`: {error}",
                self.demo_id
            ))
        })?
        else {
            return Ok(None);
        };
        self.exit_status = Some(status);
        self.stdin = None;
        self.drain_output();
        Ok(Some((self.demo_id.clone(), status.success())))
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<(), RunnerError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(RunnerError::Ui(format!(
                "live terminal session for `{}` no longer accepts input",
                self.demo_id
            )));
        };
        stdin.write_all(bytes).map_err(|error| {
            RunnerError::Ui(format!(
                "failed to write live terminal input for `{}`: {error}",
                self.demo_id
            ))
        })?;
        stdin.flush().map_err(|error| {
            RunnerError::Ui(format!(
                "failed to flush live terminal input for `{}`: {error}",
                self.demo_id
            ))
        })
    }

    fn finish_after_stop_request(&mut self) -> Result<(), RunnerError> {
        for _ in 0..20 {
            self.drain_output();
            if self.poll_exit()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stdin = None;
        self.exit_status = None;
        Ok(())
    }
}

fn spawn_live_terminal_reader<R>(mut reader: R, sender: mpsc::Sender<LiveTerminalEvent>)
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            let Ok(read) = reader.read(&mut buffer) else {
                break;
            };
            if read == 0 {
                break;
            }
            if sender
                .send(LiveTerminalEvent::Output(buffer[..read].to_vec()))
                .is_err()
            {
                break;
            }
        }
    });
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionMenuItem {
    Run,
    Rerun,
    Stop,
    Refresh,
}

impl ActionMenuItem {
    fn label(self) -> &'static str {
        match self {
            Self::Run => "Run demo",
            Self::Rerun => "Rerun demo",
            Self::Stop => "Stop demo",
            Self::Refresh => "Refresh state",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailTab {
    Overview,
    History,
    Terminal,
    Artifacts,
}

impl DetailTab {
    const ALL: [Self; 4] = [
        Self::Overview,
        Self::History,
        Self::Terminal,
        Self::Artifacts,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::History => "History",
            Self::Terminal => "Terminal",
            Self::Artifacts => "Artifacts",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("current tab exists");
        Self::ALL[next_index(index, Self::ALL.len())]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("current tab exists");
        Self::ALL[prev_index(index, Self::ALL.len())]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailSelectableItem {
    Action(ActionMenuItem),
    Artifact(usize),
    HistoryRefresh,
    HistoryAttempt(usize),
}

struct DetailRender {
    lines: Vec<Line<'static>>,
    selected_line_index: Option<usize>,
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
}

#[derive(Debug, Deserialize)]
struct DemoInspectPayload {
    demo: DemoDetail,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoHistoryPayload {
    attempt_history: DemoHistoryAttemptHistoryPayload,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoHistoryAttemptHistoryPayload {
    #[allow(dead_code)]
    path: Option<String>,
    #[allow(dead_code)]
    stored_count: usize,
    #[allow(dead_code)]
    filtered_count: usize,
    #[allow(dead_code)]
    displayed_count: usize,
    #[allow(dead_code)]
    count: usize,
    #[allow(dead_code)]
    limit: Option<usize>,
    #[allow(dead_code)]
    outcome: Option<String>,
    parse_error: Option<String>,
    attempts: Vec<DemoHistoryAttempt>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoHistoryAttempt {
    ordinal: usize,
    attempt_id: String,
    outcome: String,
    summary: Option<String>,
    receipt_path: Option<String>,
    artifacts: Vec<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    exit_code: Option<i32>,
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
    #[allow(dead_code)]
    runtime_backend: DemoRuntimeBackend,
    actions: DemoActionAvailability,
    #[allow(dead_code)]
    active_attempt: DemoActiveAttempt,
    active_terminal_session: DemoActiveTerminalSession,
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
    #[allow(dead_code)]
    runtime_backend: Option<DemoRuntimeBackend>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoActiveTerminalSession {
    available: bool,
    #[allow(dead_code)]
    state: String,
    #[allow(dead_code)]
    attempt_id: Option<String>,
    #[allow(dead_code)]
    runtime_backend: Option<DemoRuntimeBackend>,
    transport: String,
    #[allow(dead_code)]
    pty: bool,
    supports_input_forwarding: bool,
    input_forwarding_reason: Option<String>,
    #[allow(dead_code)]
    nested_tui: bool,
    terminal_size: DemoTerminalSize,
    resize: DemoTerminalResize,
    #[allow(dead_code)]
    resize_handoff_path: Option<String>,
    #[allow(dead_code)]
    stdin_input_path: Option<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    #[allow(dead_code)]
    output_available: bool,
    recent_output: DemoTerminalRecentOutput,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoTerminalSize {
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoRuntimeBackend {
    #[allow(dead_code)]
    kind: String,
    #[allow(dead_code)]
    label: String,
    #[allow(dead_code)]
    flattened_projection: bool,
    #[allow(dead_code)]
    capabilities: Vec<String>,
}

impl DemoTerminalSize {
    fn rendered(&self) -> Option<String> {
        Some(format!("{}x{}", self.cols?, self.rows?))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DemoTerminalResize {
    available: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoTerminalRecentOutput {
    stdout_lines: Vec<String>,
    stderr_lines: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoLatestAttempt {
    recorded: bool,
    state: String,
    artifacts: Vec<String>,
    summary: Option<String>,
    stdout_log_path: Option<String>,
    stderr_log_path: Option<String>,
    output_available: bool,
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
    };

    use super::{
        action_menu_items_for_detail, artifacts_detail_render, browser_terminal_key_input,
        clamp_artifact_index, detail_prefers_live_browser_terminal, detail_tab_lines,
        first_demo_id, history_detail_render, next_gap_filter, next_group_by, next_mode_filter,
        next_status_filter, overview_detail_render, query_summary, read_recent_log_lines,
        resolve_artifact_path, resolve_repo_relative_path, row_contains_demo, selected_artifact,
        selected_list_highlight_style, selected_list_highlight_symbol, status_style,
        terminal_detail_render, ActionMenuItem, BrowserRow, DemoBrowserApp, DemoDetail,
        DemoHistoryAttempt, DemoHistoryAttemptHistoryPayload, DemoHistoryPayload,
        DemoLatestAttempt, DemoListGap, DemoListGroupBy, DemoListMode, DemoListQuery,
        DemoListStatus, DemoSummary, DetailSelectableItem, DetailTab,
    };

    fn summary(id: &str) -> DemoSummary {
        DemoSummary {
            id: id.to_owned(),
            effective_status: "ready".to_owned(),
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
            runtime_backend: super::DemoRuntimeBackend {
                kind: "task".to_owned(),
                label: "task-backed".to_owned(),
                flattened_projection: false,
                capabilities: vec![],
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
                runtime_backend: None,
            },
            active_terminal_session: super::DemoActiveTerminalSession {
                available: false,
                state: "idle".to_owned(),
                attempt_id: None,
                runtime_backend: None,
                transport: "none".to_owned(),
                pty: false,
                supports_input_forwarding: false,
                input_forwarding_reason: Some(
                    "Input forwarding is not available for this active demo.".to_owned(),
                ),
                nested_tui: false,
                terminal_size: super::DemoTerminalSize {
                    cols: None,
                    rows: None,
                },
                resize: super::DemoTerminalResize { available: false },
                resize_handoff_path: None,
                stdin_input_path: None,
                stdout_log_path: None,
                stderr_log_path: None,
                output_available: false,
                recent_output: super::DemoTerminalRecentOutput {
                    stdout_lines: vec![],
                    stderr_lines: vec![],
                },
            },
            latest_attempt: DemoLatestAttempt {
                recorded: true,
                state: "passed".to_owned(),
                artifacts: artifacts.iter().map(|value| (*value).to_owned()).collect(),
                summary: None,
                stdout_log_path: None,
                stderr_log_path: None,
                output_available: false,
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
    fn browser_demo_rows_do_not_show_redundant_bracketed_action_summary() {
        let summary = summary("browser-proof-report");
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{:<23}", summary.id),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{:<10}", summary.effective_status),
                status_style(&summary.effective_status),
            ),
        ]);
        let rendered = line.to_string();

        assert!(rendered.contains("browser-proof-report"));
        assert!(rendered.contains("ready"));
        assert!(!rendered.contains('['));
        assert!(!rendered.contains("run/rerun"));
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

        let rendered = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            true,
            false,
        )
        .lines
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

        assert!(rendered.contains("Summary"));
        assert!(rendered.contains("Generate a human-checkable proof report."));
        assert!(rendered.contains("tags: self-hosted, proof"));
        assert!(rendered.contains("Actions"));
        assert!(rendered.contains("Rerun demo"));
        assert!(rendered.contains("covers: effigy.demo.browser"));
        assert!(!rendered.contains("Browser Proof Report"));
        assert!(
            rendered.find("tags: self-hosted, proof")
                < rendered.find("covers: effigy.demo.browser")
        );
        assert!(rendered.find("covers: effigy.demo.browser") < rendered.find("Summary"));
        assert!(!rendered.contains("Result"));
        assert!(!rendered.contains("status: passed"));
        assert!(!rendered.contains("Latest attempt wrote a proof report."));
        assert!(!rendered.contains("Latest Receipt"));
        assert!(!rendered.contains("actions:"));
        assert!(!rendered.contains("attempts:"));
        assert!(!rendered.contains("Artifacts"));
    }

    #[test]
    fn browser_detail_lines_hide_pointer_when_inactive() {
        let detail = detail_with_artifacts(&[]);

        let rendered = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            false,
            false,
        )
        .lines
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

        assert!(!rendered.contains("› Rerun demo"));
    }

    #[test]
    fn browser_tab_line_renders_all_demo_scoped_tabs() {
        let rendered = detail_tab_lines(DetailTab::Terminal, true, 32)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Overview"));
        assert!(rendered.contains("History"));
        assert!(rendered.contains("Terminal"));
        assert!(rendered.contains("Artifacts"));
        assert!(!rendered.contains("tabs:"));
        assert!(rendered.contains(" Terminal "));
        assert!(rendered.contains("─"));
    }

    #[test]
    fn browser_tab_border_matches_requested_width() {
        let lines = detail_tab_lines(DetailTab::Overview, true, 17);
        assert_eq!(lines[1].to_string().chars().count(), 17);
    }

    #[test]
    fn browser_list_selection_style_persists_when_detail_is_focused() {
        let unfocused = selected_list_highlight_style(false);
        let focused = selected_list_highlight_style(true);

        assert_eq!(selected_list_highlight_symbol(), "▌");
        assert_eq!(unfocused.fg, Some(super::EFFIGY_ACCENT_SOFT));
        assert_eq!(focused.fg, Some(Color::Yellow));
        assert!(unfocused.add_modifier.contains(Modifier::BOLD));
        assert!(focused.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn browser_detail_lines_show_result_only_after_session_run_visibility() {
        let mut detail = detail_with_artifacts(&["one"]);
        detail.latest_attempt.summary = Some("Latest attempt wrote a proof report.".to_owned());

        let hidden = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            true,
            false,
        )
        .lines
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        assert!(!hidden.contains("Result"));

        let visible = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            true,
            true,
        )
        .lines
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        assert!(visible.contains("Result"));
        assert!(visible.contains("status: passed"));
        assert!(visible.contains("Latest attempt wrote a proof report."));
        assert!(visible.find("Actions") < visible.find("Result"));
    }

    #[test]
    fn browser_action_menu_keeps_tab_switching_out_of_action_menu() {
        let detail = detail_with_artifacts(&["one", "two"]);
        let items = action_menu_items_for_detail(&detail)
            .into_iter()
            .map(ActionMenuItem::label)
            .collect::<Vec<_>>();

        assert_eq!(items, vec!["Rerun demo", "Refresh state"]);
    }

    #[test]
    fn browser_terminal_view_renders_active_session_output() {
        let mut detail = detail_with_artifacts(&[]);
        detail.id = "browser-proof-report".to_owned();
        detail.tags = vec!["self-hosted".to_owned()];
        detail.covers = vec!["effigy.demo.browser".to_owned()];
        detail.active_terminal_session = super::DemoActiveTerminalSession {
            available: true,
            state: "running".to_owned(),
            attempt_id: Some("demo-123".to_owned()),
            runtime_backend: Some(super::DemoRuntimeBackend {
                kind: "run".to_owned(),
                label: "run-backed".to_owned(),
                flattened_projection: false,
                capabilities: vec![
                    "active-terminal-session".to_owned(),
                    "live-terminal-output".to_owned(),
                    "stop".to_owned(),
                ],
            }),
            transport: "stream".to_owned(),
            pty: false,
            supports_input_forwarding: false,
            input_forwarding_reason: Some(
                "Input forwarding is not available for this active demo.".to_owned(),
            ),
            nested_tui: false,
            terminal_size: super::DemoTerminalSize {
                cols: Some(80),
                rows: Some(24),
            },
            resize: super::DemoTerminalResize { available: false },
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: Some(".effigy/demo/logs/demo-123.stdout.log".to_owned()),
            stderr_log_path: Some(".effigy/demo/logs/demo-123.stderr.log".to_owned()),
            output_available: true,
            recent_output: super::DemoTerminalRecentOutput {
                stdout_lines: vec!["boot".to_owned(), "serve".to_owned()],
                stderr_lines: vec!["warn".to_owned()],
            },
        };

        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-browser-terminal-view-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(repo_root.join(".effigy/demo/logs"));
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-123.stdout.log"),
            "boot\nserve-live\n",
        )
        .expect("write stdout log");
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-123.stderr.log"),
            "warn-live\n",
        )
        .expect("write stderr log");

        let rendered = terminal_detail_render(&repo_root, &detail, None, true)
            .lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        let _ = std::fs::remove_dir_all(&repo_root);

        assert!(rendered.contains("source: live terminal"));
        assert!(rendered.contains("transport: stream"));
        assert!(rendered.contains("input: Input forwarding is not available for this active demo."));
        assert!(rendered.contains("boot"));
        assert!(rendered.contains("serve-live"));
        assert!(rendered.contains("stderr: recent lines"));
        assert!(rendered.contains("warn-live"));
        assert!(!rendered.contains("tags:"));
        assert!(!rendered.contains("covers:"));
    }

    #[test]
    fn browser_terminal_view_reports_unavailable_session_honestly() {
        let detail = detail_with_artifacts(&[]);

        let rendered = terminal_detail_render(Path::new("/tmp/demo-repo"), &detail, None, true)
            .lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("No active or recorded terminal output is available."));
    }

    #[test]
    fn browser_terminal_view_falls_back_to_latest_attempt_output_when_session_is_unavailable() {
        let mut detail = detail_with_artifacts(&[]);
        detail.latest_attempt.stdout_log_path =
            Some(".effigy/demo/logs/demo-latest.stdout.log".to_owned());
        detail.latest_attempt.stderr_log_path =
            Some(".effigy/demo/logs/demo-latest.stderr.log".to_owned());
        detail.latest_attempt.output_available = true;

        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-browser-latest-terminal-view-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(repo_root.join(".effigy/demo/logs"));
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-latest.stdout.log"),
            "latest-out\n",
        )
        .expect("write stdout log");
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-latest.stderr.log"),
            "latest-err\n",
        )
        .expect("write stderr log");

        let rendered = terminal_detail_render(&repo_root, &detail, None, true)
            .lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        let _ = std::fs::remove_dir_all(&repo_root);

        assert!(!rendered.contains("No active terminal session is available for this demo."));
        assert!(rendered.contains("source: latest attempt logs"));
        assert!(rendered.contains("latest-out"));
        assert!(rendered.contains("latest-err"));
    }

    #[test]
    fn browser_terminal_view_falls_back_to_inspect_snapshot_when_logs_are_missing() {
        let mut detail = detail_with_artifacts(&[]);
        detail.active_terminal_session = super::DemoActiveTerminalSession {
            available: true,
            state: "running".to_owned(),
            attempt_id: Some("demo-123".to_owned()),
            runtime_backend: Some(super::DemoRuntimeBackend {
                kind: "run".to_owned(),
                label: "run-backed".to_owned(),
                flattened_projection: false,
                capabilities: vec![
                    "active-terminal-session".to_owned(),
                    "live-terminal-output".to_owned(),
                    "stop".to_owned(),
                    "pty".to_owned(),
                ],
            }),
            transport: "pty".to_owned(),
            pty: true,
            supports_input_forwarding: false,
            input_forwarding_reason: Some(
                "Input forwarding is not available for this active demo.".to_owned(),
            ),
            nested_tui: false,
            terminal_size: super::DemoTerminalSize {
                cols: Some(120),
                rows: Some(32),
            },
            resize: super::DemoTerminalResize { available: false },
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: Some(".effigy/demo/logs/missing.stdout.log".to_owned()),
            stderr_log_path: None,
            output_available: true,
            recent_output: super::DemoTerminalRecentOutput {
                stdout_lines: vec!["snapshot-line".to_owned()],
                stderr_lines: vec![],
            },
        };

        let rendered = terminal_detail_render(Path::new("/tmp/demo-repo"), &detail, None, true)
            .lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("source: inspect snapshot"));
        assert!(rendered.contains("snapshot-line"));
    }

    #[test]
    fn browser_history_view_renders_selected_attempt_details() {
        let mut detail = detail_with_artifacts(&[]);
        detail.tags = vec!["self-hosted".to_owned()];
        let history = DemoHistoryPayload {
            attempt_history: DemoHistoryAttemptHistoryPayload {
                path: None,
                stored_count: 1,
                filtered_count: 1,
                displayed_count: 1,
                count: 1,
                limit: None,
                outcome: None,
                parse_error: None,
                attempts: vec![DemoHistoryAttempt {
                    ordinal: 1,
                    attempt_id: "demo-123".to_owned(),
                    outcome: "failed".to_owned(),
                    summary: Some("Proof artifact was missing.".to_owned()),
                    receipt_path: Some(".effigy/demo/history/demo-123.json".to_owned()),
                    artifacts: vec![".effigy/demo/artifacts/report.html".to_owned()],
                    stdout_log_path: Some(".effigy/demo/logs/demo-123.stdout.log".to_owned()),
                    stderr_log_path: Some(".effigy/demo/logs/demo-123.stderr.log".to_owned()),
                    exit_code: Some(1),
                }],
            },
        };

        let rendered = history_detail_render(
            &detail,
            Some(&history),
            Some(DetailSelectableItem::HistoryAttempt(1)),
            true,
        )
        .lines
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

        assert!(rendered.contains("Refresh history"));
        assert!(rendered.contains("#01"));
        assert!(rendered.contains("Proof artifact was missing."));
        assert!(rendered.contains("receipt:\n  .effigy/demo/history/demo-123.json"));
        assert!(rendered.contains("stdout:\n  .effigy/demo/logs/demo-123.stdout.log"));
        assert!(rendered.contains("stderr:\n  .effigy/demo/logs/demo-123.stderr.log"));
        assert!(rendered.contains("artifacts:\n  .effigy/demo/artifacts/report.html"));
        assert!(!rendered.contains("tags:"));
        assert!(!rendered.contains("Retained attempts for"));
    }

    #[test]
    fn browser_escape_returns_to_overview_tab_before_exiting() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        app.detail_tab = DetailTab::History;
        app.history = Some(DemoHistoryPayload {
            attempt_history: DemoHistoryAttemptHistoryPayload {
                path: None,
                stored_count: 0,
                filtered_count: 0,
                displayed_count: 0,
                count: 0,
                limit: None,
                outcome: None,
                parse_error: None,
                attempts: vec![],
            },
        });

        let should_exit = app
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .expect("escape should succeed");

        assert!(!should_exit);
        assert!(matches!(app.detail_tab, DetailTab::Overview));
        assert_eq!(app.footer_message, "Viewing Overview tab.");
    }

    #[test]
    fn browser_tab_key_switches_between_list_and_detail_panels() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        assert!(matches!(app.focus, super::BrowserFocus::List));

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .expect("tab should succeed");
        assert!(matches!(app.focus, super::BrowserFocus::Detail));

        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
            .expect("shift-tab should succeed");
        assert!(matches!(app.focus, super::BrowserFocus::List));
    }

    #[test]
    fn browser_arrow_keys_switch_demo_views_inside_detail_panel() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        app.focus = super::BrowserFocus::Detail;
        app.detail_tab = DetailTab::Terminal;

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
            .expect("right should succeed");
        assert!(matches!(app.detail_tab, DetailTab::Artifacts));

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
            .expect("left should succeed");
        assert!(matches!(app.detail_tab, DetailTab::Terminal));
    }

    #[test]
    fn browser_terminal_key_input_maps_terminal_controls() {
        assert_eq!(
            browser_terminal_key_input(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some("\n".to_owned())
        );
        assert_eq!(
            browser_terminal_key_input(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some("\u{1b}[D".to_owned())
        );
        assert_eq!(
            browser_terminal_key_input(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some("\u{3}".to_owned())
        );
    }

    #[test]
    fn browser_terminal_enter_toggles_input_mode_when_supported() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        let mut detail = detail_with_artifacts(&[]);
        detail.active_terminal_session.available = true;
        detail.active_terminal_session.supports_input_forwarding = true;
        detail.active_terminal_session.input_forwarding_reason = None;
        app.detail = Some(detail);
        app.selected_demo_id = Some("demo".to_owned());
        app.focus = super::BrowserFocus::Detail;
        app.detail_tab = DetailTab::Terminal;

        app.handle_enter_key()
            .expect("enter should enable input mode");
        assert!(app.terminal_input_mode);

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .expect("escape should leave input mode");
        assert!(!app.terminal_input_mode);
    }

    #[test]
    fn run_backed_interactive_demo_prefers_live_browser_terminal() {
        let mut detail = detail_with_artifacts(&[]);
        detail.mode = "interactive".to_owned();
        detail.runtime_backend.kind = "run".to_owned();
        detail.runtime_backend.capabilities = vec!["browser-live-attach".to_owned()];

        assert!(detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Run {
                demo_id: detail.id.clone()
            }
        ));
        assert!(detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Rerun {
                demo_id: detail.id.clone()
            }
        ));
    }

    #[test]
    fn concurrent_runner_single_process_demo_prefers_live_browser_terminal() {
        let mut detail = detail_with_artifacts(&[]);
        detail.mode = "interactive".to_owned();
        detail.runtime_backend.kind = "concurrent-runner".to_owned();
        detail.runtime_backend.capabilities = vec!["browser-live-attach".to_owned()];

        assert!(detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Run {
                demo_id: detail.id.clone()
            }
        ));
    }

    #[test]
    fn concurrent_runner_without_live_attach_capability_does_not_prefer_live_browser_terminal() {
        let mut detail = detail_with_artifacts(&[]);
        detail.mode = "interactive".to_owned();
        detail.runtime_backend.kind = "concurrent-runner".to_owned();

        assert!(!detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Run {
                demo_id: detail.id.clone()
            }
        ));
    }

    #[test]
    fn browser_terminal_up_down_scroll_when_detail_panel_is_active() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        app.focus = super::BrowserFocus::Detail;
        app.detail_tab = DetailTab::Terminal;

        app.handle_down_key();
        app.handle_down_key();
        assert_eq!(app.terminal_scroll_offset, 2);

        app.handle_up_key();
        assert_eq!(app.terminal_scroll_offset, 1);
    }

    #[test]
    fn browser_artifacts_tab_renders_artifact_entries() {
        let mut detail = detail_with_artifacts(&["one", "two"]);
        detail.tags = vec!["self-hosted".to_owned()];
        detail.covers = vec!["effigy.demo.browser".to_owned()];
        let rendered =
            artifacts_detail_render(&detail, Some(DetailSelectableItem::Artifact(1)), true)
                .lines
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");

        assert!(rendered.contains("one"));
        assert!(rendered.contains("two"));
        assert!(!rendered.contains("Artifacts"));
        assert!(!rendered.contains("↑/↓ selects artifacts"));
        assert!(!rendered.contains("Enter opens selection"));
        assert!(!rendered.contains("tags:"));
        assert!(!rendered.contains("covers:"));
        assert!(!rendered.contains("Recorded artifacts for"));
    }

    #[test]
    fn browser_tab_renders_do_not_repeat_title_chrome() {
        let mut detail = detail_with_artifacts(&["one"]);
        detail.title = "Browser Proof Report".to_owned();

        let history_rendered = history_detail_render(&detail, None, None, true)
            .lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!history_rendered.contains("Browser Proof Report"));
        assert!(!history_rendered.contains("History View"));

        let terminal_rendered =
            terminal_detail_render(Path::new("/tmp/demo-repo"), &detail, None, true)
                .lines
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
        assert!(!terminal_rendered.contains("Browser Proof Report"));
        assert!(!terminal_rendered.contains("Terminal View"));

        let artifacts_rendered = artifacts_detail_render(&detail, None, true)
            .lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!artifacts_rendered.contains("Browser Proof Report"));
        assert!(!artifacts_rendered.contains("Artifacts View"));
    }

    #[test]
    fn browser_escape_exits_from_overview_root_view() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());

        let should_exit = app
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .expect("escape should succeed");

        assert!(should_exit);
    }
}
