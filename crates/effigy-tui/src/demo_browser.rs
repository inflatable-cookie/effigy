use std::collections::HashSet;
use std::io::Stdout;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use effigy_cli::{DemoArgs, DemoListGroupBy, DemoListQuery, DemoSubcommand};
use effigy_demo::browser::{DemoDetail, DemoHistoryPayload, DemoSummary};
use ratatui::backend::CrosstermBackend;
use ratatui::text::Line;
use ratatui::{Frame, Terminal};
use serde_json::Value as JsonValue;

use crate::core::{next_index, prev_index};

#[path = "demo_browser_overlay.rs"]
mod overlay;
#[path = "demo_browser_render.rs"]
mod render;
#[path = "demo_browser_state_methods.rs"]
mod state_methods;
#[path = "demo_browser_terminal.rs"]
mod terminal;

use overlay::{render_action_overlay, render_filter_overlay, render_prompt_overlay};
pub use render::{
    action_menu_items_for_detail, artifacts_detail_render, browser_body_constraints,
    browser_header_lines, browser_help_line, browser_list_summary_lines,
    browser_terminal_key_input, centered_rect, clamp_artifact_index, compact_kv_line,
    demo_history_args, demo_input_args, demo_inspect_args, demo_list_args, demo_payload_message,
    demo_resize_args, demo_stop_args, detail_prefers_live_browser_terminal, detail_tab_lines,
    filter_change_message, filter_menu_value, first_demo_id, history_detail_render, muted_line,
    next_gap_filter, next_group_by, next_mode_filter, next_status_filter, normalized_prompt_value,
    overview_detail_render, parse_demo_history_payload, parse_demo_inspect_payload,
    parse_demo_list_payload, preferred_run_action, prompt_apply_message, query_summary,
    render_browser_demo_row, render_browser_empty_overlay, render_browser_footer,
    render_browser_header, render_browser_list, render_browser_overlay, resolve_artifact_path,
    resolve_repo_relative_path, row_contains_demo, rows_from_payload, section_heading,
    selectable_detail_line, selected_artifact, selected_list_highlight_style,
    selected_list_highlight_symbol, stacked_kv_lines, status_style,
};
pub use terminal::{
    browser_live_terminal_env, browser_terminal_viewport_size, browser_vt_lines,
    build_live_terminal_view, build_terminal_view, live_terminal_status_lines,
    read_recent_log_lines, sanitize_live_terminal_bytes, take_complete_terminal_bytes,
    terminal_detail_render, terminal_status_lines, BrowserLiveTerminalSession, TerminalView,
    TerminalViewSource, DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES,
    DEMO_BROWSER_TERMINAL_COLS_ENV, DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK,
    DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT, DEMO_BROWSER_TERMINAL_ROWS_ENV,
};

#[derive(Clone)]
pub enum BrowserRow {
    Group(String),
    Demo(Box<DemoSummary>),
}

#[derive(Clone, Copy)]
pub enum BrowserFocus {
    List,
    Detail,
}

pub struct ActionMenuState {
    pub items: Vec<ActionMenuItem>,
    pub selected_index: usize,
}

impl ActionMenuState {
    pub fn new(items: Vec<ActionMenuItem>) -> Self {
        Self {
            items,
            selected_index: 0,
        }
    }

    pub fn select_next(&mut self) {
        self.selected_index = next_index(self.selected_index, self.items.len());
    }

    pub fn select_previous(&mut self) {
        self.selected_index = prev_index(self.selected_index, self.items.len());
    }

    pub fn selected_item(&self) -> Option<ActionMenuItem> {
        self.items.get(self.selected_index).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionMenuItem {
    Run,
    Rerun,
    Stop,
    Refresh,
}

impl ActionMenuItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Run => "Run demo",
            Self::Rerun => "Rerun demo",
            Self::Stop => "Stop demo",
            Self::Refresh => "Refresh state",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTab {
    Overview,
    History,
    Terminal,
    Artifacts,
}

impl DetailTab {
    pub const ALL: [Self; 4] = [
        Self::Overview,
        Self::History,
        Self::Terminal,
        Self::Artifacts,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::History => "History",
            Self::Terminal => "Terminal",
            Self::Artifacts => "Artifacts",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("current tab exists");
        Self::ALL[next_index(index, Self::ALL.len())]
    }

    pub fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("current tab exists");
        Self::ALL[prev_index(index, Self::ALL.len())]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailSelectableItem {
    Action(ActionMenuItem),
    Artifact(usize),
    HistoryRefresh,
    HistoryAttempt(usize),
}

pub struct DetailRender {
    pub lines: Vec<Line<'static>>,
    pub selected_line_index: Option<usize>,
}

#[derive(Default)]
pub struct FilterMenuState {
    pub selected_index: usize,
}

impl FilterMenuState {
    pub const ITEMS: [FilterMenuItem; 10] = [
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

    pub fn items(&self) -> &'static [FilterMenuItem] {
        &Self::ITEMS
    }

    pub fn select_next(&mut self) {
        self.selected_index = next_index(self.selected_index, Self::ITEMS.len());
    }

    pub fn select_previous(&mut self) {
        self.selected_index = prev_index(self.selected_index, Self::ITEMS.len());
    }

    pub fn selected_item(&self) -> FilterMenuItem {
        Self::ITEMS[self.selected_index]
    }
}

#[derive(Clone, Copy)]
pub enum FilterMenuItem {
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
    pub fn label(self) -> &'static str {
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

pub struct QueryPromptState {
    pub kind: QueryPromptKind,
    pub value: String,
}

impl QueryPromptState {
    pub fn render_value(&self) -> String {
        if self.value.is_empty() {
            "<empty>".to_owned()
        } else {
            self.value.clone()
        }
    }
}

pub enum BrowserOverlay {
    Prompt(QueryPromptState),
    Action(ActionMenuState),
    Filters(FilterMenuState),
}

#[derive(Clone, Copy)]
pub enum QueryPromptKind {
    Search,
    Owner,
    Tag,
    Cover,
}

impl QueryPromptKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Search => "Edit Search Filter",
            Self::Owner => "Edit Owner Filter",
            Self::Tag => "Edit Tag Filter",
            Self::Cover => "Edit Cover Filter",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Self::Search => "Match demo id, title, or summary text.",
            Self::Owner => "Match one exact demo owner.",
            Self::Tag => "Match one exact declared tag.",
            Self::Cover => "Match one declared coverage key.",
        }
    }
}

pub struct PendingAction {
    pub demo_id: String,
    pub label: String,
    pub receiver: Receiver<Result<serde_json::Value, String>>,
}

#[derive(Clone)]
pub struct PendingLiveTerminalLaunch {
    pub demo_id: String,
    pub subcommand: DemoSubcommand,
    pub action_label: String,
}

pub struct BackgroundDemoCommandPlan {
    pub demo_id: String,
    pub subcommand: DemoSubcommand,
    pub action_label: String,
}

pub enum BrowserRuntimePlan {
    None,
    BackgroundDemoCommand(BackgroundDemoCommandPlan),
    DemoCommand {
        demo_id: String,
        subcommand: DemoSubcommand,
    },
    RefreshHistory {
        demo_id: String,
    },
    OpenArtifact(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRefreshLoadPlan {
    pub selected_demo_id: Option<String>,
    pub load_history_for_demo_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRefreshRequests {
    pub list: DemoArgs,
    pub inspect: Option<DemoArgs>,
    pub history: Option<DemoArgs>,
}

pub enum BrowserActionMenuPlan {
    RefreshState { message: String },
    Runtime(BrowserRuntimePlan),
}

pub struct BrowserResizeRequest {
    pub args: DemoArgs,
    pub next_size: (String, u16, u16),
}

pub enum BrowserHostEffect {
    None,
    ExitBrowser,
    RefreshState {
        message: Option<String>,
    },
    SetDetailTab {
        next_tab: DetailTab,
        history_request: Option<DemoArgs>,
    },
    ExecuteRuntimePlan(BrowserRuntimePlan),
    ForwardTerminalInput {
        args: Option<DemoArgs>,
        forwarded_demo_id: Option<String>,
    },
}

pub enum BrowserLoopEvent {
    None,
    Key(KeyEvent),
    Resize(u16, u16),
}

pub enum BrowserRuntimeLifecycleEvent {
    None,
    PendingActionCompleted {
        demo_id: String,
        label: String,
        result: Result<serde_json::Value, String>,
    },
    PendingActionDisconnected,
    LiveTerminalFinished {
        demo_id: String,
        success: bool,
    },
}

pub type BrowserTerminal = Terminal<CrosstermBackend<Stdout>>;

pub enum BrowserRuntimeExecutionRequest {
    None,
    BackgroundDemoCommand {
        demo_id: String,
        action_label: String,
        args: DemoArgs,
    },
    DemoCommand {
        demo_id: String,
        args: DemoArgs,
    },
    RefreshHistory {
        args: DemoArgs,
    },
    OpenArtifact(PathBuf),
}

pub const DEMO_BROWSER_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(125);
pub const DEMO_BROWSER_AUTO_REFRESH_INTERVAL: Duration = Duration::from_millis(750);

pub fn init_browser_terminal() -> Result<BrowserTerminal, String> {
    enable_raw_mode().map_err(|error| error.to_string())?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|error| error.to_string())?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(|error| error.to_string())
}

pub fn restore_browser_terminal(terminal: &mut BrowserTerminal) -> Result<(), String> {
    disable_raw_mode().map_err(|error| error.to_string())?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|error| error.to_string())?;
    terminal.show_cursor().map_err(|error| error.to_string())
}

pub fn poll_browser_loop_event(timeout: Duration) -> Result<BrowserLoopEvent, String> {
    if !event::poll(timeout).map_err(|error| error.to_string())? {
        return Ok(BrowserLoopEvent::None);
    }
    match event::read().map_err(|error| error.to_string())? {
        Event::Key(key) if key.kind == KeyEventKind::Press => Ok(BrowserLoopEvent::Key(key)),
        Event::Key(_) => Ok(BrowserLoopEvent::None),
        Event::Resize(cols, rows) => Ok(BrowserLoopEvent::Resize(cols, rows)),
        _ => Ok(BrowserLoopEvent::None),
    }
}

pub fn open_artifact_path(path: &Path) -> Result<(), String> {
    let mut command = build_open_command(path);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let status = command.status().map_err(|error| {
        format!(
            "failed to launch artifact opener for `{}`: {error}",
            path.display()
        )
    })?;
    if !status.success() {
        return Err(format!(
            "artifact opener exited unsuccessfully for `{}` with status {status}",
            path.display()
        ));
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

pub struct DemoBrowserState {
    pub group_by: Option<DemoListGroupBy>,
    pub query: DemoListQuery,
    pub rows: Vec<BrowserRow>,
    pub focus: BrowserFocus,
    pub selected_demo_id: Option<String>,
    pub selected_row_index: usize,
    pub selected_detail_entry_index: usize,
    pub selected_artifact_index: usize,
    pub selected_history_attempt_ordinal: Option<usize>,
    pub terminal_scroll_offset: usize,
    pub terminal_input_mode: bool,
    pub detail_tab: DetailTab,
    pub detail: Option<DemoDetail>,
    pub history: Option<DemoHistoryPayload>,
    pub live_terminal_session: Option<BrowserLiveTerminalSession>,
    pub last_reported_terminal_size: Option<(String, u16, u16)>,
    pub last_rendered_terminal_viewport_size: Option<(u16, u16)>,
    pub result_visible_demo_ids: HashSet<String>,
    pub footer_message: String,
    pub pending_action: Option<PendingAction>,
    pub pending_live_terminal_launch: Option<PendingLiveTerminalLaunch>,
    pub last_refresh: Instant,
    pub total_demo_count: usize,
    pub overlay: Option<BrowserOverlay>,
}

pub struct DemoBrowserRefreshProjection {
    pub total_demo_count: usize,
    pub rows: Vec<BrowserRow>,
    pub selected_demo_id: Option<String>,
    pub detail: Option<DemoDetail>,
    pub history: Option<DemoHistoryPayload>,
}

pub enum BrowserEscapeAction {
    ExitBrowser,
    SetOverviewTab,
    StayOpen,
}

pub enum BrowserKeyAction {
    Continue,
    Consumed,
    ExitBrowser,
    RefreshState { message: String },
    SetDetailTab(DetailTab),
    ToggleTerminalInputMode,
    DispatchSelectedDetailEntry,
    ForwardTerminalInput(String),
    RunActionMenuItem(ActionMenuItem),
}

pub struct DemoBrowserApp {
    pub repo_root: PathBuf,
    pub state: DemoBrowserState,
}

impl DemoBrowserApp {
    pub fn new(repo_root: PathBuf, initial_group_by: Option<DemoListGroupBy>) -> Self {
        Self {
            repo_root,
            state: DemoBrowserState::new(initial_group_by),
        }
    }

    pub fn run_with<F>(
        &mut self,
        terminal: &mut BrowserTerminal,
        invoke_json: F,
    ) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        self.refresh_state_with(invoke_json.clone())?;
        loop {
            match self.poll_pending_action_event() {
                BrowserRuntimeLifecycleEvent::None => {}
                BrowserRuntimeLifecycleEvent::PendingActionCompleted {
                    demo_id,
                    label,
                    result,
                } => {
                    let message = result.as_ref().ok().and_then(demo_payload_message);
                    let mapped_result = result.map(|_| message);
                    let _ = self.refresh_state_with(invoke_json.clone());
                    self.handle_pending_action_completed(&demo_id, &label, mapped_result);
                }
                BrowserRuntimeLifecycleEvent::PendingActionDisconnected => {
                    self.handle_pending_action_disconnected();
                }
                BrowserRuntimeLifecycleEvent::LiveTerminalFinished { .. } => {}
            }
            match self.poll_live_terminal_event()? {
                BrowserRuntimeLifecycleEvent::None => {}
                BrowserRuntimeLifecycleEvent::LiveTerminalFinished { demo_id, success } => {
                    self.refresh_state_with(invoke_json.clone())?;
                    self.handle_live_terminal_finished(&demo_id, success);
                }
                BrowserRuntimeLifecycleEvent::PendingActionCompleted { .. }
                | BrowserRuntimeLifecycleEvent::PendingActionDisconnected => {}
            }
            terminal
                .draw(|frame| self.render(frame))
                .map_err(|error| error.to_string())?;
            if let Some(pending) = self.take_pending_live_terminal_launch() {
                if let Some(viewport_size) = self.last_rendered_terminal_viewport_size {
                    let executable = std::env::current_exe().map_err(|error| {
                        format!("failed to resolve current effigy executable: {error}")
                    })?;
                    let session = BrowserLiveTerminalSession::spawn(
                        executable,
                        self.repo_root.clone(),
                        pending.demo_id.clone(),
                        pending.subcommand,
                        Some(viewport_size),
                    )?;
                    self.register_live_terminal_session_started(
                        session,
                        &pending.action_label,
                        &pending.demo_id,
                    );
                } else {
                    self.restore_pending_live_terminal_launch(pending);
                }
            }

            match poll_browser_loop_event(DEMO_BROWSER_EVENT_POLL_INTERVAL)? {
                BrowserLoopEvent::Key(key) => {
                    if self.handle_key_with(key, invoke_json.clone())? {
                        break;
                    }
                }
                BrowserLoopEvent::Resize(cols, rows) => {
                    self.handle_resize_event_with(cols, rows, invoke_json.clone())?
                }
                BrowserLoopEvent::None if self.auto_refresh_due() => {
                    self.refresh_state_with(invoke_json.clone())?;
                }
                BrowserLoopEvent::None => {}
            }
        }
        self.shutdown_live_terminal_session_with(invoke_json)?;
        Ok(())
    }

    pub fn handle_key_with<F>(&mut self, key: KeyEvent, invoke_json: F) -> Result<bool, String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        let repo_root = self.repo_root.clone();
        let effect = self.resolve_key_host_effect(&key, &repo_root)?;
        self.execute_host_effect_with(effect, invoke_json)
    }

    pub fn handle_resize_event_with<F>(
        &mut self,
        cols: u16,
        rows: u16,
        invoke_json: F,
    ) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        let repo_root = self.repo_root.clone();
        self.sync_active_terminal_resize_for_viewport_with(
            &repo_root,
            browser_terminal_viewport_size(cols, rows),
            invoke_json,
        )
    }

    pub fn handle_enter_key_with<F>(&mut self, invoke_json: F) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        let repo_root = self.repo_root.clone();
        let effect = self.resolve_key_host_effect(
            &KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
            &repo_root,
        )?;
        let _ = self.execute_host_effect_with(effect, invoke_json)?;
        Ok(())
    }

    pub fn dispatch_run_or_rerun_with<F>(&mut self, invoke_json: F) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        let plan = self.plan_run_or_rerun();
        self.execute_runtime_plan_with(plan, invoke_json)
    }

    pub fn execute_host_effect_with<F>(
        &mut self,
        effect: BrowserHostEffect,
        invoke_json: F,
    ) -> Result<bool, String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        match effect {
            BrowserHostEffect::None => Ok(false),
            BrowserHostEffect::ExitBrowser => Ok(true),
            BrowserHostEffect::RefreshState { message } => {
                self.refresh_state_with(invoke_json.clone())?;
                if let Some(message) = message {
                    self.footer_message = message;
                }
                Ok(false)
            }
            BrowserHostEffect::SetDetailTab {
                next_tab,
                history_request: _,
            } => {
                let repo_root = self.repo_root.clone();
                if self.apply_detail_tab_change_with(&repo_root, next_tab, invoke_json.clone())? {
                    self.sync_active_terminal_resize_for_current_view_with(invoke_json.clone())?;
                }
                Ok(false)
            }
            BrowserHostEffect::ExecuteRuntimePlan(plan) => {
                self.execute_runtime_plan_with(plan, invoke_json)?;
                Ok(false)
            }
            BrowserHostEffect::ForwardTerminalInput {
                args,
                forwarded_demo_id,
            } => {
                if let Some(args) = args {
                    let _ = invoke_json(args)?;
                    if let Some(demo_id) = forwarded_demo_id {
                        self.footer_message =
                            format!("Forwarded terminal input to demo `{demo_id}`.");
                        self.last_refresh = Instant::now() - Duration::from_secs(5);
                    }
                }
                Ok(false)
            }
        }
    }

    pub fn execute_runtime_plan_with<F>(
        &mut self,
        plan: BrowserRuntimePlan,
        invoke_json: F,
    ) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        match self.build_runtime_execution_request(&self.repo_root, plan) {
            BrowserRuntimeExecutionRequest::None => Ok(()),
            BrowserRuntimeExecutionRequest::BackgroundDemoCommand {
                demo_id,
                action_label,
                args,
            } => {
                let (sender, receiver) = mpsc::channel();
                let invoke_json = invoke_json.clone();
                std::thread::spawn(move || {
                    let result = invoke_json(args);
                    let _ = sender.send(result);
                });
                self.register_background_demo_command(demo_id, action_label, receiver);
                Ok(())
            }
            BrowserRuntimeExecutionRequest::DemoCommand { demo_id, args } => {
                let payload = invoke_json(args)?;
                self.refresh_state_with(invoke_json.clone())?;
                self.footer_message = demo_payload_message(&payload)
                    .unwrap_or_else(|| format!("Stop requested for demo `{demo_id}`."));
                Ok(())
            }
            BrowserRuntimeExecutionRequest::RefreshHistory { args } => {
                let history = parse_demo_history_payload(invoke_json(args)?)
                    .map_err(|error| error.to_string())?;
                self.history = Some(history);
                self.sync_selected_detail_entry();
                self.footer_message = "Refreshed retained history in the detail pane.".to_owned();
                Ok(())
            }
            BrowserRuntimeExecutionRequest::OpenArtifact(path) => {
                open_artifact_path(&path)?;
                self.footer_message = format!("Opened artifact `{}`.", path.display());
                Ok(())
            }
        }
    }

    pub fn shutdown_live_terminal_session_with<F>(&mut self, invoke_json: F) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        let Some(mut session) = self.live_terminal_session.take() else {
            return Ok(());
        };
        let repo_root = self.repo_root.clone();
        let stop_request = self.build_shutdown_request(&repo_root);
        if let Some(args) = stop_request {
            let _ = invoke_json(args);
        }
        session.finish_after_stop_request()?;
        Ok(())
    }

    pub fn refresh_state_with<F>(&mut self, invoke_json: F) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        let repo_root = self.repo_root.clone();
        self.refresh_with(&repo_root, invoke_json.clone())?;
        self.sync_active_terminal_resize_for_current_view_with(invoke_json)?;
        Ok(())
    }

    pub fn sync_active_terminal_resize_for_current_view_with<F>(
        &mut self,
        invoke_json: F,
    ) -> Result<(), String>
    where
        F: Fn(DemoArgs) -> Result<JsonValue, String> + Clone + Send + 'static,
    {
        let Ok((cols, rows)) = crossterm::terminal::size() else {
            return Ok(());
        };
        let repo_root = self.repo_root.clone();
        self.sync_active_terminal_resize_for_viewport_with(
            &repo_root,
            browser_terminal_viewport_size(cols, rows),
            invoke_json,
        )
    }

    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let repo_root = self.repo_root.clone();
        self.render_browser_screen(frame, &repo_root);
    }
}

impl Deref for DemoBrowserApp {
    type Target = DemoBrowserState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for DemoBrowserApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}
