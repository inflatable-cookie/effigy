use std::collections::HashSet;
use std::fs;
use std::io::{Read, Stdout, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command as ProcessCommand, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use effigy_cli::{
    DemoArgs, DemoListGap, DemoListGroupBy, DemoListMode, DemoListQuery, DemoListStatus,
    DemoSubcommand,
};
use effigy_demo::browser::{
    DemoDetail, DemoHistoryAttempt, DemoHistoryPayload, DemoInspectPayload, DemoListPayload,
    DemoSummary,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::line;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;

use crate::core::{
    effigy_panel_block, next_index, prev_index, EFFIGY_ACCENT, EFFIGY_ACCENT_SOFT, EFFIGY_MUTED,
};
use crate::terminal_text::{render_vt_lines, LiveTerminalBuffer};

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

impl DemoBrowserState {
    pub fn new(initial_group_by: Option<DemoListGroupBy>) -> Self {
        Self {
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
            last_rendered_terminal_viewport_size: None,
            result_visible_demo_ids: HashSet::new(),
            footer_message: "Loading demo registry...".to_owned(),
            pending_action: None,
            pending_live_terminal_launch: None,
            last_refresh: Instant::now() - Duration::from_secs(5),
            total_demo_count: 0,
            overlay: None,
        }
    }

    pub fn selected_detail(&self) -> Option<&DemoDetail> {
        self.detail.as_ref()
    }

    pub fn selected_demo_id(&self) -> Option<&str> {
        self.selected_demo_id.as_deref()
    }

    pub fn select_next_demo(&mut self) {
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

    pub fn select_previous_demo(&mut self) {
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

    pub fn current_demo_row_index(&self) -> Option<usize> {
        let selected = self.selected_demo_id()?;
        self.rows.iter().position(|row| match row {
            BrowserRow::Group(_) => false,
            BrowserRow::Demo(summary) => summary.id == selected,
        })
    }

    pub fn resolve_selected_demo_id_for_rows(&self, rows: &[BrowserRow]) -> Option<String> {
        self.selected_demo_id
            .clone()
            .filter(|demo_id| row_contains_demo(rows, demo_id))
            .or_else(|| first_demo_id(rows))
    }

    pub fn plan_refresh_load(&self, rows: &[BrowserRow]) -> BrowserRefreshLoadPlan {
        let selected_demo_id = self.resolve_selected_demo_id_for_rows(rows);
        let load_history_for_demo_id = if matches!(self.detail_tab, DetailTab::History) {
            selected_demo_id.clone()
        } else {
            None
        };
        BrowserRefreshLoadPlan {
            selected_demo_id,
            load_history_for_demo_id,
        }
    }

    pub fn build_refresh_requests(
        &self,
        repo_root: &Path,
        plan: &BrowserRefreshLoadPlan,
    ) -> BrowserRefreshRequests {
        BrowserRefreshRequests {
            list: demo_list_args(repo_root, self.query.clone()),
            inspect: plan
                .selected_demo_id
                .as_ref()
                .map(|demo_id| demo_inspect_args(repo_root, demo_id.clone())),
            history: plan
                .load_history_for_demo_id
                .as_ref()
                .map(|demo_id| demo_history_args(repo_root, demo_id.clone())),
        }
    }

    pub fn apply_refresh_projection(&mut self, projection: DemoBrowserRefreshProjection) {
        let previous_selected_demo_id = self.selected_demo_id.clone();
        self.total_demo_count = projection.total_demo_count;
        self.rows = projection.rows;
        self.selected_demo_id = projection.selected_demo_id.clone();
        self.selected_row_index = projection
            .selected_demo_id
            .as_ref()
            .and_then(|demo_id| {
                self.rows.iter().position(|row| match row {
                    BrowserRow::Group(_) => false,
                    BrowserRow::Demo(summary) => &summary.id == demo_id,
                })
            })
            .unwrap_or(0);
        self.detail = projection.detail;

        if self.selected_demo_id != previous_selected_demo_id {
            self.detail_tab = DetailTab::Overview;
            self.selected_detail_entry_index = 0;
            self.selected_history_attempt_ordinal = None;
            self.terminal_scroll_offset = 0;
            self.terminal_input_mode = false;
            self.history = None;
            self.last_reported_terminal_size = None;
            self.last_rendered_terminal_viewport_size = None;
        } else if let Some(history) = projection.history {
            self.history = Some(history);
        }

        self.selected_artifact_index = self.detail.as_ref().map_or(0, |detail| {
            clamp_artifact_index(self.selected_artifact_index, detail)
        });
        self.sync_selected_detail_entry();
    }

    pub fn mark_refreshed(&mut self) {
        self.last_refresh = Instant::now();
    }

    pub fn apply_detail_tab_change(
        &mut self,
        next_tab: DetailTab,
        history: Option<DemoHistoryPayload>,
    ) -> bool {
        self.terminal_input_mode = false;
        self.terminal_scroll_offset = 0;
        self.detail_tab = next_tab;
        self.selected_detail_entry_index = 0;
        if matches!(self.detail_tab, DetailTab::History) {
            self.history = history;
            self.selected_history_attempt_ordinal = self
                .selected_history_attempt()
                .map(|attempt| attempt.ordinal);
        }
        self.sync_selected_detail_entry();
        self.footer_message = match self.detail_tab {
            DetailTab::Overview => "Viewing Overview tab.".to_owned(),
            DetailTab::History => "Viewing History tab.".to_owned(),
            DetailTab::Terminal => "Viewing Terminal tab.".to_owned(),
            DetailTab::Artifacts => "Viewing Artifacts tab.".to_owned(),
        };
        matches!(self.detail_tab, DetailTab::Terminal)
    }

    pub fn build_history_request_for_tab_change(
        &self,
        repo_root: &Path,
        next_tab: DetailTab,
    ) -> Option<DemoArgs> {
        if !matches!(next_tab, DetailTab::History) {
            return None;
        }
        self.selected_demo_id()
            .map(|demo_id| demo_history_args(repo_root, demo_id.to_owned()))
    }

    pub fn plan_action_menu_item(&mut self, item: ActionMenuItem) -> BrowserActionMenuPlan {
        match item {
            ActionMenuItem::Run | ActionMenuItem::Rerun => {
                BrowserActionMenuPlan::Runtime(self.plan_run_or_rerun())
            }
            ActionMenuItem::Stop => BrowserActionMenuPlan::Runtime(self.plan_stop()),
            ActionMenuItem::Refresh => BrowserActionMenuPlan::RefreshState {
                message: "Refreshed demo browser state.".to_owned(),
            },
        }
    }

    pub fn toggle_terminal_input_mode(&mut self) {
        if self.selected_live_terminal_session().is_some() {
            self.terminal_input_mode = !self.terminal_input_mode;
            self.footer_message = if self.terminal_input_mode {
                "Live terminal input capture enabled. Typed keys go directly to the demo. Esc exits input mode."
                    .to_owned()
            } else {
                "Terminal input capture disabled.".to_owned()
            };
            return;
        }
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return;
        };
        let session = &detail.active_terminal_session;
        if !session.available {
            self.footer_message = "No active terminal session is available for input.".to_owned();
            return;
        }
        if !session.supports_input_forwarding {
            self.footer_message = session
                .input_forwarding_reason
                .clone()
                .unwrap_or_else(|| "Terminal input forwarding is unavailable.".to_owned());
            return;
        }
        self.terminal_input_mode = !self.terminal_input_mode;
        self.footer_message = if self.terminal_input_mode {
            "Terminal input capture enabled. Typed keys go to the demo. Esc exits input mode."
                .to_owned()
        } else {
            "Terminal input capture disabled.".to_owned()
        };
    }

    pub fn forward_terminal_input(
        &mut self,
        repo_root: &Path,
        text: &str,
    ) -> Result<Option<DemoArgs>, String> {
        if let Some(session) = self.selected_live_terminal_session_mut() {
            session.write_input(text.as_bytes())?;
            self.footer_message = format!(
                "Forwarded live terminal input to demo `{}`.",
                session.demo_id
            );
            return Ok(None);
        }
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return Ok(None);
        };
        Ok(Some(demo_input_args(
            repo_root,
            detail.id.clone(),
            text.to_owned(),
        )))
    }

    pub fn build_resize_request(
        &self,
        repo_root: &Path,
        cols: u16,
        rows: u16,
    ) -> Option<BrowserResizeRequest> {
        if !matches!(self.detail_tab, DetailTab::Terminal) {
            return None;
        }
        let detail = self.selected_detail()?;
        let session = &detail.active_terminal_session;
        if !session.available || !session.resize.available {
            return None;
        }
        let next_size = (detail.id.clone(), cols, rows);
        if self.last_reported_terminal_size.as_ref() == Some(&next_size) {
            return None;
        }
        Some(BrowserResizeRequest {
            args: demo_resize_args(repo_root, detail.id.clone(), cols, rows),
            next_size,
        })
    }

    pub fn record_resize_applied(&mut self, next_size: (String, u16, u16)) {
        self.last_reported_terminal_size = Some(next_size);
        self.last_refresh = Instant::now() - Duration::from_secs(5);
    }

    pub fn build_shutdown_request(&self, repo_root: &Path) -> Option<DemoArgs> {
        let session = self.live_terminal_session.as_ref()?;
        if !session.is_running() {
            return None;
        }
        Some(demo_stop_args(repo_root, session.demo_id.clone()))
    }

    pub fn build_runtime_execution_request(
        &self,
        repo_root: &Path,
        plan: BrowserRuntimePlan,
    ) -> BrowserRuntimeExecutionRequest {
        match plan {
            BrowserRuntimePlan::None => BrowserRuntimeExecutionRequest::None,
            BrowserRuntimePlan::BackgroundDemoCommand(plan) => {
                BrowserRuntimeExecutionRequest::BackgroundDemoCommand {
                    demo_id: plan.demo_id,
                    action_label: plan.action_label,
                    args: DemoArgs {
                        subcommand: plan.subcommand,
                        repo_override: Some(repo_root.to_path_buf()),
                        output_json: true,
                    },
                }
            }
            BrowserRuntimePlan::DemoCommand {
                demo_id,
                subcommand,
            } => BrowserRuntimeExecutionRequest::DemoCommand {
                demo_id,
                args: DemoArgs {
                    subcommand,
                    repo_override: Some(repo_root.to_path_buf()),
                    output_json: true,
                },
            },
            BrowserRuntimePlan::RefreshHistory { demo_id } => {
                BrowserRuntimeExecutionRequest::RefreshHistory {
                    args: demo_history_args(repo_root, demo_id),
                }
            }
            BrowserRuntimePlan::OpenArtifact(path) => {
                BrowserRuntimeExecutionRequest::OpenArtifact(path)
            }
        }
    }

    pub fn apply_detail_tab_change_with<F>(
        &mut self,
        repo_root: &Path,
        next_tab: DetailTab,
        mut invoke: F,
    ) -> Result<bool, String>
    where
        F: FnMut(DemoArgs) -> Result<JsonValue, String>,
    {
        let history = self
            .build_history_request_for_tab_change(repo_root, next_tab)
            .map(&mut invoke)
            .transpose()?
            .map(parse_demo_history_payload)
            .transpose()?;
        Ok(self.apply_detail_tab_change(next_tab, history))
    }

    pub fn refresh_with<F>(&mut self, repo_root: &Path, mut invoke: F) -> Result<(), String>
    where
        F: FnMut(DemoArgs) -> Result<JsonValue, String>,
    {
        let list_payload =
            parse_demo_list_payload(invoke(demo_list_args(repo_root, self.query.clone()))?)?;
        let rows = rows_from_payload(&list_payload);
        let refresh_plan = self.plan_refresh_load(&rows);
        let requests = self.build_refresh_requests(repo_root, &refresh_plan);

        let detail = requests
            .inspect
            .map(&mut invoke)
            .transpose()?
            .map(|payload| parse_demo_inspect_payload(payload).map(|inspect| inspect.demo))
            .transpose()?;

        let history = requests
            .history
            .map(&mut invoke)
            .transpose()?
            .map(parse_demo_history_payload)
            .transpose()?;

        self.apply_refresh_projection(DemoBrowserRefreshProjection {
            total_demo_count: list_payload.total_count,
            rows,
            selected_demo_id: refresh_plan.selected_demo_id,
            detail,
            history,
        });
        self.mark_refreshed();
        Ok(())
    }

    pub fn sync_active_terminal_resize_for_viewport_with<F>(
        &mut self,
        repo_root: &Path,
        viewport_size: (u16, u16),
        mut invoke: F,
    ) -> Result<(), String>
    where
        F: FnMut(DemoArgs) -> Result<JsonValue, String>,
    {
        let Some(BrowserResizeRequest { args, next_size }) =
            self.build_resize_request(repo_root, viewport_size.0, viewport_size.1)
        else {
            return Ok(());
        };
        let _ = invoke(args)?;
        self.record_resize_applied(next_size);
        Ok(())
    }

    pub fn resolve_key_host_effect(
        &mut self,
        key: &KeyEvent,
        repo_root: &Path,
    ) -> Result<BrowserHostEffect, String> {
        Ok(match self.handle_key_event(key) {
            BrowserKeyAction::Continue | BrowserKeyAction::Consumed => BrowserHostEffect::None,
            BrowserKeyAction::ExitBrowser => BrowserHostEffect::ExitBrowser,
            BrowserKeyAction::RefreshState { message } => BrowserHostEffect::RefreshState {
                message: Some(message),
            },
            BrowserKeyAction::SetDetailTab(next_tab) => BrowserHostEffect::SetDetailTab {
                next_tab,
                history_request: self.build_history_request_for_tab_change(repo_root, next_tab),
            },
            BrowserKeyAction::ToggleTerminalInputMode => {
                self.toggle_terminal_input_mode();
                BrowserHostEffect::None
            }
            BrowserKeyAction::DispatchSelectedDetailEntry => {
                BrowserHostEffect::ExecuteRuntimePlan(self.plan_selected_detail_entry(repo_root))
            }
            BrowserKeyAction::ForwardTerminalInput(payload) => {
                let args = self.forward_terminal_input(repo_root, &payload)?;
                let forwarded_demo_id = args.as_ref().and_then(|args| match &args.subcommand {
                    DemoSubcommand::Input { demo_id, .. } => Some(demo_id.clone()),
                    _ => None,
                });
                BrowserHostEffect::ForwardTerminalInput {
                    args,
                    forwarded_demo_id,
                }
            }
            BrowserKeyAction::RunActionMenuItem(item) => match self.plan_action_menu_item(item) {
                BrowserActionMenuPlan::RefreshState { message } => {
                    BrowserHostEffect::RefreshState {
                        message: Some(message),
                    }
                }
                BrowserActionMenuPlan::Runtime(plan) => BrowserHostEffect::ExecuteRuntimePlan(plan),
            },
        })
    }

    pub fn handle_pending_action_completed(
        &mut self,
        demo_id: &str,
        label: &str,
        result: Result<Option<String>, String>,
    ) {
        self.pending_action = None;
        self.footer_message = match result {
            Ok(Some(message)) => message,
            Ok(None) => format!("Demo `{demo_id}` {label} completed and browser state refreshed."),
            Err(error) => format!("Demo `{demo_id}` {label} failed: {error}"),
        };
    }

    pub fn handle_pending_action_disconnected(&mut self) {
        self.pending_action = None;
        self.footer_message =
            "The background demo action exited without returning a result.".to_owned();
    }

    pub fn handle_live_terminal_finished(&mut self, demo_id: &str, success: bool) {
        self.terminal_input_mode = false;
        self.footer_message = if success {
            format!("Live terminal session for demo `{demo_id}` completed.")
        } else {
            format!("Live terminal session for demo `{demo_id}` ended.")
        };
        self.last_refresh = Instant::now();
    }

    pub fn handle_escape_key(&mut self) -> BrowserEscapeAction {
        if self.terminal_input_mode {
            self.terminal_input_mode = false;
            self.footer_message = "Terminal input capture disabled.".to_owned();
            return BrowserEscapeAction::StayOpen;
        }
        if matches!(self.detail_tab, DetailTab::Overview) {
            BrowserEscapeAction::ExitBrowser
        } else {
            BrowserEscapeAction::SetOverviewTab
        }
    }

    pub fn handle_down_key(&mut self) {
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

    pub fn handle_up_key(&mut self) {
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

    pub fn handle_key_event(&mut self, key: &KeyEvent) -> BrowserKeyAction {
        if self.overlay.is_some() {
            return self.handle_overlay_key_event(key.code);
        }
        if self.terminal_input_mode {
            return self.handle_terminal_input_key_event(key);
        }
        match key.code {
            KeyCode::Esc => match self.handle_escape_key() {
                BrowserEscapeAction::ExitBrowser => BrowserKeyAction::ExitBrowser,
                BrowserEscapeAction::SetOverviewTab => {
                    BrowserKeyAction::SetDetailTab(DetailTab::Overview)
                }
                BrowserEscapeAction::StayOpen => BrowserKeyAction::Consumed,
            },
            KeyCode::Char('q') => BrowserKeyAction::ExitBrowser,
            KeyCode::Down => {
                self.handle_down_key();
                BrowserKeyAction::Consumed
            }
            KeyCode::Up => {
                self.handle_up_key();
                BrowserKeyAction::Consumed
            }
            KeyCode::Right => self.handle_right_key_event(),
            KeyCode::Left => self.handle_left_key_event(),
            KeyCode::Tab | KeyCode::BackTab => {
                self.toggle_focus_panel();
                BrowserKeyAction::Consumed
            }
            KeyCode::Char('/') => {
                self.open_prompt(QueryPromptKind::Search);
                BrowserKeyAction::Consumed
            }
            KeyCode::Char('f') => {
                self.open_filter_overlay();
                BrowserKeyAction::Consumed
            }
            KeyCode::Enter => self.handle_enter_key_event(),
            KeyCode::Char('R') => BrowserKeyAction::RefreshState {
                message: "Refreshed demo browser state.".to_owned(),
            },
            _ => BrowserKeyAction::Continue,
        }
    }

    fn handle_enter_key_event(&mut self) -> BrowserKeyAction {
        match self.focus {
            BrowserFocus::List => {
                self.open_action_overlay();
                BrowserKeyAction::Consumed
            }
            BrowserFocus::Detail => {
                if matches!(self.detail_tab, DetailTab::Terminal) {
                    BrowserKeyAction::ToggleTerminalInputMode
                } else {
                    BrowserKeyAction::DispatchSelectedDetailEntry
                }
            }
        }
    }

    fn handle_terminal_input_key_event(&mut self, key: &KeyEvent) -> BrowserKeyAction {
        if key.code == KeyCode::Esc {
            self.terminal_input_mode = false;
            self.footer_message = "Terminal input capture disabled.".to_owned();
            return BrowserKeyAction::Consumed;
        }
        let Some(payload) = browser_terminal_key_input(key) else {
            self.footer_message = "That key is not forwarded in terminal input mode.".to_owned();
            return BrowserKeyAction::Consumed;
        };
        BrowserKeyAction::ForwardTerminalInput(payload)
    }

    fn handle_right_key_event(&mut self) -> BrowserKeyAction {
        match self.focus {
            BrowserFocus::List => {
                self.footer_message =
                    "List panel focused. Tab switches to detail. ↑/↓ selects demos.".to_owned();
                BrowserKeyAction::Consumed
            }
            BrowserFocus::Detail => BrowserKeyAction::SetDetailTab(self.detail_tab.next()),
        }
    }

    fn handle_left_key_event(&mut self) -> BrowserKeyAction {
        match self.focus {
            BrowserFocus::List => {
                self.footer_message =
                    "List panel focused. Tab switches to detail. ↑/↓ selects demos.".to_owned();
                BrowserKeyAction::Consumed
            }
            BrowserFocus::Detail => BrowserKeyAction::SetDetailTab(self.detail_tab.previous()),
        }
    }

    fn handle_overlay_key_event(&mut self, code: KeyCode) -> BrowserKeyAction {
        let Some(mut overlay) = self.overlay.take() else {
            return BrowserKeyAction::Continue;
        };
        let mut keep_open = false;
        let action = match &mut overlay {
            BrowserOverlay::Prompt(prompt) => match code {
                KeyCode::Esc => {
                    self.footer_message = "Closed browser prompt.".to_owned();
                    BrowserKeyAction::Consumed
                }
                KeyCode::Enter => {
                    let (label, value) = match prompt.kind {
                        QueryPromptKind::Search => ("search", &mut self.query.search),
                        QueryPromptKind::Owner => ("owner", &mut self.query.owner),
                        QueryPromptKind::Tag => ("tag", &mut self.query.tag),
                        QueryPromptKind::Cover => ("cover", &mut self.query.cover),
                    };
                    *value = normalized_prompt_value(&prompt.value);
                    BrowserKeyAction::RefreshState {
                        message: prompt_apply_message(label, value.as_deref()),
                    }
                }
                KeyCode::Backspace => {
                    prompt.value.pop();
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
                KeyCode::Char(ch) => {
                    if !ch.is_control() {
                        prompt.value.push(ch);
                    }
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
                _ => {
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
            },
            BrowserOverlay::Action(menu) => match code {
                KeyCode::Esc => {
                    self.footer_message = "Closed browser action menu.".to_owned();
                    BrowserKeyAction::Consumed
                }
                KeyCode::Down => {
                    menu.select_next();
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
                KeyCode::Up => {
                    menu.select_previous();
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
                KeyCode::Enter => menu
                    .selected_item()
                    .map(BrowserKeyAction::RunActionMenuItem)
                    .unwrap_or(BrowserKeyAction::Consumed),
                _ => {
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
            },
            BrowserOverlay::Filters(menu) => match code {
                KeyCode::Esc => {
                    self.footer_message = "Closed browser filter menu.".to_owned();
                    BrowserKeyAction::Consumed
                }
                KeyCode::Down => {
                    menu.select_next();
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
                KeyCode::Up => {
                    menu.select_previous();
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
                KeyCode::Enter => {
                    let item = menu.selected_item();
                    let action = self.apply_filter_key_event(item);
                    keep_open = self.overlay.is_none()
                        && !matches!(
                            item,
                            FilterMenuItem::Search
                                | FilterMenuItem::Owner
                                | FilterMenuItem::Tag
                                | FilterMenuItem::Cover
                        );
                    action
                }
                _ => {
                    keep_open = true;
                    BrowserKeyAction::Consumed
                }
            },
        };
        if keep_open && self.overlay.is_none() {
            self.overlay = Some(overlay);
        }
        action
    }

    fn apply_filter_key_event(&mut self, item: FilterMenuItem) -> BrowserKeyAction {
        match item {
            FilterMenuItem::Search => {
                self.open_prompt(QueryPromptKind::Search);
                BrowserKeyAction::Consumed
            }
            FilterMenuItem::Owner => {
                self.open_prompt(QueryPromptKind::Owner);
                BrowserKeyAction::Consumed
            }
            FilterMenuItem::Tag => {
                self.open_prompt(QueryPromptKind::Tag);
                BrowserKeyAction::Consumed
            }
            FilterMenuItem::Mode => {
                self.query.mode = next_mode_filter(self.query.mode);
                BrowserKeyAction::RefreshState {
                    message: filter_change_message(
                        "mode",
                        self.query.mode.map(DemoListMode::as_str),
                    ),
                }
            }
            FilterMenuItem::Cover => {
                self.open_prompt(QueryPromptKind::Cover);
                BrowserKeyAction::Consumed
            }
            FilterMenuItem::Status => {
                self.query.status = next_status_filter(self.query.status);
                BrowserKeyAction::RefreshState {
                    message: filter_change_message(
                        "status",
                        self.query.status.map(DemoListStatus::as_str),
                    ),
                }
            }
            FilterMenuItem::Gap => {
                self.query.gap = next_gap_filter(self.query.gap);
                BrowserKeyAction::RefreshState {
                    message: filter_change_message("gap", self.query.gap.map(DemoListGap::as_str)),
                }
            }
            FilterMenuItem::StaleOnly => {
                self.query.stale_only = !self.query.stale_only;
                BrowserKeyAction::RefreshState {
                    message: if self.query.stale_only {
                        "Enabled stale-only demo filtering.".to_owned()
                    } else {
                        "Disabled stale-only demo filtering.".to_owned()
                    },
                }
            }
            FilterMenuItem::GroupBy => {
                self.group_by = next_group_by(self.group_by);
                self.query.group_by = self.group_by;
                BrowserKeyAction::RefreshState {
                    message: format!(
                        "Grouping demos by {}",
                        self.group_by.map_or("none", DemoListGroupBy::as_str)
                    ),
                }
            }
            FilterMenuItem::ClearAll => {
                self.query = DemoListQuery {
                    group_by: self.group_by,
                    ..DemoListQuery::default()
                };
                BrowserKeyAction::RefreshState {
                    message: "Cleared browser filters.".to_owned(),
                }
            }
        }
    }

    pub fn selected_live_terminal_session(&self) -> Option<&BrowserLiveTerminalSession> {
        let demo_id = self.selected_demo_id()?;
        self.live_terminal_session
            .as_ref()
            .filter(|session| session.demo_id == demo_id)
    }

    pub fn selected_live_terminal_session_mut(
        &mut self,
    ) -> Option<&mut BrowserLiveTerminalSession> {
        let demo_id = self.selected_demo_id()?.to_owned();
        self.live_terminal_session
            .as_mut()
            .filter(|session| session.demo_id == demo_id)
    }

    pub fn selected_artifact(&self) -> Option<&str> {
        let detail = self.selected_detail()?;
        selected_artifact(detail, self.selected_artifact_index)
    }

    pub fn selected_history_attempt(&self) -> Option<&DemoHistoryAttempt> {
        let history = self.history.as_ref()?;
        self.selected_history_attempt_ordinal
            .and_then(|ordinal| {
                history
                    .attempt_history
                    .attempts
                    .iter()
                    .find(|attempt| attempt.ordinal == ordinal)
            })
            .or_else(|| history.attempt_history.attempts.first())
    }

    pub fn detail_render(&self, repo_root: &Path) -> DetailRender {
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
                terminal_detail_render(repo_root, detail, selected_item, detail_focused)
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

    pub fn detail_selectable_items(&self) -> Vec<DetailSelectableItem> {
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

    pub fn selected_detail_item(&self) -> Option<DetailSelectableItem> {
        let items = self.detail_selectable_items();
        items
            .get(self.selected_detail_entry_index)
            .copied()
            .or_else(|| items.first().copied())
    }

    pub fn open_prompt(&mut self, kind: QueryPromptKind) {
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

    pub fn open_action_overlay(&mut self) {
        let items = self.action_menu_items();
        self.overlay = Some(BrowserOverlay::Action(ActionMenuState::new(items)));
        self.footer_message = "Use ↑/↓ to choose an action. Enter applies. Esc closes.".to_owned();
    }

    pub fn open_filter_overlay(&mut self) {
        self.overlay = Some(BrowserOverlay::Filters(FilterMenuState::default()));
        self.footer_message =
            "Use ↑/↓ to choose a filter. Enter edits or cycles. Esc closes.".to_owned();
    }

    pub fn action_menu_items(&self) -> Vec<ActionMenuItem> {
        let Some(detail) = self.selected_detail() else {
            return vec![ActionMenuItem::Refresh];
        };
        action_menu_items_for_detail(detail)
    }

    pub fn focus_list(&mut self) {
        self.focus = BrowserFocus::List;
        self.footer_message =
            "List panel focused. ↑/↓ selects demos. Tab switches to detail. Enter opens actions."
                .to_owned();
    }

    pub fn focus_detail(&mut self) {
        self.focus = BrowserFocus::Detail;
        self.footer_message =
            "Detail panel focused. ←/→ switches views. ↑/↓ selects visible entries. Enter activates the selected option.".to_owned();
    }

    pub fn toggle_focus_panel(&mut self) {
        match self.focus {
            BrowserFocus::List => self.focus_detail(),
            BrowserFocus::Detail => self.focus_list(),
        }
    }

    pub fn select_next_detail_entry(&mut self) {
        let items = self.detail_selectable_items();
        if items.is_empty() {
            self.footer_message = "No demo is currently selected.".to_owned();
            return;
        }
        self.selected_detail_entry_index =
            next_index(self.selected_detail_entry_index, items.len());
        self.sync_selected_detail_entry();
    }

    pub fn select_previous_detail_entry(&mut self) {
        let items = self.detail_selectable_items();
        if items.is_empty() {
            self.footer_message = "No demo is currently selected.".to_owned();
            return;
        }
        self.selected_detail_entry_index =
            prev_index(self.selected_detail_entry_index, items.len());
        self.sync_selected_detail_entry();
    }

    pub fn sync_selected_detail_entry(&mut self) {
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

    pub fn plan_run_or_rerun(&mut self) -> BrowserRuntimePlan {
        if self.pending_action.is_some() {
            self.footer_message =
                "A demo run or rerun is already in flight. Stop or wait for it first.".to_owned();
            return BrowserRuntimePlan::None;
        }
        if self
            .live_terminal_session
            .as_ref()
            .is_some_and(BrowserLiveTerminalSession::is_running)
        {
            self.footer_message =
                "A live browser terminal session is already in flight. Stop or wait for it first."
                    .to_owned();
            return BrowserRuntimePlan::None;
        }
        let Some(detail) = self.selected_detail().cloned() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return BrowserRuntimePlan::None;
        };

        let Some(subcommand) = preferred_run_action(&detail) else {
            self.footer_message =
                "The selected demo cannot be run or rerun in its current state.".to_owned();
            return BrowserRuntimePlan::None;
        };

        let action_label = match &subcommand {
            DemoSubcommand::Run { .. } => "run",
            DemoSubcommand::Rerun { .. } => "rerun",
            _ => unreachable!(),
        };
        if detail_prefers_live_browser_terminal(&detail, &subcommand) {
            self.result_visible_demo_ids.insert(detail.id.clone());
            self.detail_tab = DetailTab::Terminal;
            self.pending_live_terminal_launch = Some(PendingLiveTerminalLaunch {
                demo_id: detail.id.clone(),
                subcommand,
                action_label: action_label.to_owned(),
            });
            self.footer_message = format!(
                "Preparing live terminal {action_label} for demo `{}`.",
                detail.id
            );
            self.last_refresh = Instant::now() - Duration::from_secs(5);
            return BrowserRuntimePlan::None;
        }

        BrowserRuntimePlan::BackgroundDemoCommand(BackgroundDemoCommandPlan {
            demo_id: detail.id,
            subcommand,
            action_label: action_label.to_owned(),
        })
    }

    pub fn register_background_demo_command(
        &mut self,
        demo_id: String,
        label: String,
        receiver: Receiver<Result<serde_json::Value, String>>,
    ) {
        self.pending_action = Some(PendingAction {
            demo_id: demo_id.clone(),
            label: label.clone(),
            receiver,
        });
        self.result_visible_demo_ids.insert(demo_id.clone());
        self.footer_message = format!("Started `{label}` for demo `{demo_id}` in the background.");
    }

    pub fn plan_stop(&mut self) -> BrowserRuntimePlan {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return BrowserRuntimePlan::None;
        };
        let demo_id = detail.id.clone();
        if !detail.actions.stop.available {
            self.footer_message = detail
                .actions
                .stop
                .reason
                .clone()
                .unwrap_or_else(|| "The selected demo cannot be stopped right now.".to_owned());
            return BrowserRuntimePlan::None;
        }
        BrowserRuntimePlan::DemoCommand {
            demo_id: demo_id.clone(),
            subcommand: DemoSubcommand::Stop { demo_id },
        }
    }

    pub fn plan_open_artifact(&mut self, repo_root: &Path) -> BrowserRuntimePlan {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return BrowserRuntimePlan::None;
        };
        let Some(artifact) = selected_artifact(detail, self.selected_artifact_index) else {
            self.footer_message = "The selected demo has no recorded artifacts to open.".to_owned();
            return BrowserRuntimePlan::None;
        };
        let artifact_path = resolve_artifact_path(repo_root, artifact);
        if !artifact_path.exists() {
            self.footer_message =
                format!("Artifact path is missing: `{}`.", artifact_path.display());
            return BrowserRuntimePlan::None;
        }
        BrowserRuntimePlan::OpenArtifact(artifact_path)
    }

    pub fn plan_refresh_history(&mut self) -> BrowserRuntimePlan {
        let Some(detail) = self.selected_detail() else {
            self.footer_message = "No demo is currently selected.".to_owned();
            return BrowserRuntimePlan::None;
        };
        BrowserRuntimePlan::RefreshHistory {
            demo_id: detail.id.clone(),
        }
    }

    pub fn plan_selected_detail_entry(&mut self, repo_root: &Path) -> BrowserRuntimePlan {
        let Some(item) = self.selected_detail_item() else {
            self.footer_message = "No detail action is available for the selected demo.".to_owned();
            return BrowserRuntimePlan::None;
        };
        match item {
            DetailSelectableItem::Action(ActionMenuItem::Run | ActionMenuItem::Rerun) => {
                self.plan_run_or_rerun()
            }
            DetailSelectableItem::Action(ActionMenuItem::Stop) => self.plan_stop(),
            DetailSelectableItem::Action(ActionMenuItem::Refresh) => BrowserRuntimePlan::None,
            DetailSelectableItem::Artifact(index) => {
                self.selected_artifact_index = index;
                self.plan_open_artifact(repo_root)
            }
            DetailSelectableItem::HistoryRefresh => self.plan_refresh_history(),
            DetailSelectableItem::HistoryAttempt(ordinal) => {
                self.selected_history_attempt_ordinal = Some(ordinal);
                self.footer_message =
                    format!("Viewing retained attempt #{ordinal} in the detail pane.");
                BrowserRuntimePlan::None
            }
        }
    }

    pub fn poll_pending_action_event(&mut self) -> BrowserRuntimeLifecycleEvent {
        let Some(pending) = self.pending_action.as_ref() else {
            return BrowserRuntimeLifecycleEvent::None;
        };
        match pending.receiver.try_recv() {
            Ok(result) => BrowserRuntimeLifecycleEvent::PendingActionCompleted {
                demo_id: pending.demo_id.clone(),
                label: pending.label.clone(),
                result,
            },
            Err(mpsc::TryRecvError::Empty) => BrowserRuntimeLifecycleEvent::None,
            Err(mpsc::TryRecvError::Disconnected) => {
                BrowserRuntimeLifecycleEvent::PendingActionDisconnected
            }
        }
    }

    pub fn poll_live_terminal_event(&mut self) -> Result<BrowserRuntimeLifecycleEvent, String> {
        let mut finished = None;
        if let Some(session) = self.live_terminal_session.as_mut() {
            session.drain_output();
            finished = session.poll_exit()?;
        }
        Ok(match finished {
            Some((demo_id, success)) => {
                BrowserRuntimeLifecycleEvent::LiveTerminalFinished { demo_id, success }
            }
            None => BrowserRuntimeLifecycleEvent::None,
        })
    }

    pub fn take_pending_live_terminal_launch(&mut self) -> Option<PendingLiveTerminalLaunch> {
        self.pending_live_terminal_launch.take()
    }

    pub fn restore_pending_live_terminal_launch(&mut self, pending: PendingLiveTerminalLaunch) {
        self.pending_live_terminal_launch = Some(pending);
    }

    pub fn register_live_terminal_session_started(
        &mut self,
        session: BrowserLiveTerminalSession,
        action_label: &str,
        demo_id: &str,
    ) {
        self.live_terminal_session = Some(session);
        self.footer_message = format!("Started live terminal {action_label} for demo `{demo_id}`.");
        self.last_refresh = Instant::now() - Duration::from_secs(5);
    }

    pub fn auto_refresh_due(&self) -> bool {
        self.last_refresh.elapsed() >= DEMO_BROWSER_AUTO_REFRESH_INTERVAL
    }

    pub fn render_browser_screen(&mut self, frame: &mut Frame<'_>, repo_root: &Path) {
        let area = frame.area();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(10),
                Constraint::Length(4),
            ])
            .split(area);
        render_browser_header(frame, layout[0], repo_root);
        self.render_browser_body(frame, layout[1], repo_root);
        render_browser_footer(
            frame,
            layout[2],
            self.focus,
            self.detail_tab,
            self.terminal_input_mode,
            &self.footer_message,
        );
        if self.rows.is_empty() {
            render_browser_empty_overlay(frame, area, self.total_demo_count, &self.query);
        }
        if let Some(overlay_state) = &self.overlay {
            render_browser_overlay(frame, area, overlay_state, &self.query);
        }
    }

    pub fn render_browser_body(&mut self, frame: &mut Frame<'_>, area: Rect, repo_root: &Path) {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(browser_body_constraints())
            .split(area);
        render_browser_list(
            frame,
            layout[0],
            matches!(self.focus, BrowserFocus::List),
            &self.query,
            &self.rows,
            self.selected_row_index,
            self.total_demo_count,
        );
        self.render_browser_detail_panel(frame, layout[1], repo_root);
    }

    pub fn render_browser_detail_panel(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        repo_root: &Path,
    ) {
        if matches!(self.detail_tab, DetailTab::Terminal) {
            self.render_terminal_panel(frame, area, repo_root);
            return;
        }
        let mut render = self.detail_render(repo_root);
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

    pub fn render_terminal_panel(&mut self, frame: &mut Frame<'_>, area: Rect, repo_root: &Path) {
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
        self.last_rendered_terminal_viewport_size =
            Some((layout[2].width.max(1), layout[2].height.max(1)));

        frame.render_widget(Paragraph::new(tab_lines), layout[0]);

        let scroll_offset = self.terminal_scroll_offset;
        let terminal_view = if let Some(session) = self.selected_live_terminal_session_mut() {
            build_live_terminal_view(
                session,
                layout[2].width as usize,
                layout[2].height as usize,
                scroll_offset,
            )
        } else {
            build_terminal_view(
                repo_root,
                &detail,
                layout[2].width as usize,
                layout[2].height as usize,
                scroll_offset,
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
        frame.render_widget(Paragraph::new(terminal_view.lines), layout[2]);
    }
}

pub fn row_contains_demo(rows: &[BrowserRow], demo_id: &str) -> bool {
    rows.iter().any(|row| match row {
        BrowserRow::Group(_) => false,
        BrowserRow::Demo(summary) => summary.id == demo_id,
    })
}

pub fn demo_list_args(repo_root: &Path, query: DemoListQuery) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::List { query },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_inspect_args(repo_root: &Path, demo_id: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Inspect { demo_id },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_history_args(repo_root: &Path, demo_id: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::History {
            demo_id,
            limit: None,
            outcome: None,
            attempt_id: None,
            attempt_ordinal: None,
        },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_input_args(repo_root: &Path, demo_id: String, text: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Input {
            demo_id,
            text,
            append_newline: false,
        },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_stop_args(repo_root: &Path, demo_id: String) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Stop { demo_id },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn demo_resize_args(repo_root: &Path, demo_id: String, cols: u16, rows: u16) -> DemoArgs {
    DemoArgs {
        subcommand: DemoSubcommand::Resize {
            demo_id,
            cols,
            rows,
        },
        repo_override: Some(repo_root.to_path_buf()),
        output_json: true,
    }
}

pub fn parse_demo_payload<T: DeserializeOwned>(
    payload: JsonValue,
    context: &str,
) -> Result<T, String> {
    serde_json::from_value(payload)
        .map_err(|error| format!("failed to parse {context} payload for browser: {error}"))
}

pub fn parse_demo_inspect_payload(payload: JsonValue) -> Result<DemoInspectPayload, String> {
    parse_demo_payload(payload, "demo inspect")
}

pub fn parse_demo_list_payload(payload: JsonValue) -> Result<DemoListPayload, String> {
    parse_demo_payload(payload, "demo list")
}

pub fn parse_demo_history_payload(payload: JsonValue) -> Result<DemoHistoryPayload, String> {
    parse_demo_payload(payload, "demo history")
}

pub fn demo_payload_message(payload: &JsonValue) -> Option<String> {
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

pub fn first_demo_id(rows: &[BrowserRow]) -> Option<String> {
    rows.iter().find_map(|row| match row {
        BrowserRow::Group(_) => None,
        BrowserRow::Demo(summary) => Some(summary.id.clone()),
    })
}

pub fn rows_from_payload(payload: &DemoListPayload) -> Vec<BrowserRow> {
    if let Some(groups) = &payload.groups {
        let mut rows = Vec::new();
        for group in groups {
            rows.push(BrowserRow::Group(format!(
                "{} ({})",
                group.label, group.count
            )));
            for demo in &group.demos {
                rows.push(BrowserRow::Demo(Box::new(demo.clone())));
            }
        }
        return rows;
    }

    payload
        .demos
        .iter()
        .cloned()
        .map(|demo| BrowserRow::Demo(Box::new(demo)))
        .collect()
}

pub fn preferred_run_action(detail: &DemoDetail) -> Option<DemoSubcommand> {
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

pub fn detail_prefers_live_browser_terminal(
    detail: &DemoDetail,
    subcommand: &DemoSubcommand,
) -> bool {
    matches!(
        subcommand,
        DemoSubcommand::Run { .. } | DemoSubcommand::Rerun { .. }
    ) && detail
        .runtime_backend
        .projection_shape
        .live_terminal_eligible
        && !detail.active_terminal_session.nested_tui
        && matches!(detail.mode.as_str(), "interactive" | "hybrid")
}

pub fn status_style(status: &str) -> Style {
    match status {
        "running" | "running (stop-requested)" => Style::default().fg(Color::Yellow),
        "passed" => Style::default().fg(Color::Green),
        "failed" | "broken" => Style::default().fg(Color::Red),
        "missing" | "planned" => Style::default().fg(Color::Magenta),
        _ => Style::default().fg(Color::Cyan),
    }
}

pub fn render_browser_demo_row(summary: &DemoSummary, available_width: usize) -> Line<'static> {
    let status = summary.effective_status.as_str();
    let status_width = status.len().max(5);
    let reserved = 1 + status_width + 1;
    let name_width = available_width.saturating_sub(reserved).max(1);
    let rendered_id = truncate_demo_row_label(&summary.id, name_width);

    Line::from(vec![
        Span::styled(
            format!("{rendered_id:<name_width$}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(format!("{status:>status_width$}"), status_style(status)),
        Span::raw(" "),
    ])
}

fn truncate_demo_row_label(label: &str, max_width: usize) -> String {
    let width = label.chars().count();
    if width <= max_width {
        return label.to_owned();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let keep = max_width - 3;
    let mut truncated = label.chars().take(keep).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn key_style() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

pub fn browser_help_line(
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

pub fn selected_list_highlight_style(list_focused: bool) -> Style {
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

pub fn selected_list_highlight_symbol() -> &'static str {
    "▌"
}

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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

pub fn next_group_by(current: Option<DemoListGroupBy>) -> Option<DemoListGroupBy> {
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

pub fn next_mode_filter(current: Option<DemoListMode>) -> Option<DemoListMode> {
    match current {
        None => Some(DemoListMode::Headless),
        Some(DemoListMode::Headless) => Some(DemoListMode::Interactive),
        Some(DemoListMode::Interactive) => Some(DemoListMode::Hybrid),
        Some(DemoListMode::Hybrid) => None,
    }
}

pub fn clamp_artifact_index(current: usize, detail: &DemoDetail) -> usize {
    if detail.latest_attempt.artifacts.is_empty() {
        0
    } else {
        current.min(detail.latest_attempt.artifacts.len() - 1)
    }
}

pub fn selected_artifact(detail: &DemoDetail, selected_index: usize) -> Option<&str> {
    detail
        .latest_attempt
        .artifacts
        .get(clamp_artifact_index(selected_index, detail))
        .map(String::as_str)
}

pub fn resolve_repo_relative_path(repo_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

pub fn resolve_artifact_path(repo_root: &Path, artifact: &str) -> PathBuf {
    resolve_repo_relative_path(repo_root, artifact)
}

pub fn overview_detail_render(
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

pub fn history_detail_render(
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

pub fn artifacts_detail_render(
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

pub fn browser_body_constraints() -> [Constraint; 2] {
    [Constraint::Percentage(28), Constraint::Percentage(72)]
}

pub fn render_browser_header(frame: &mut Frame<'_>, area: Rect, repo_root: &Path) {
    let lines = browser_header_lines(repo_root);
    frame.render_widget(
        Paragraph::new(lines).block(effigy_panel_block(Some(" EFFIGY "), true, EFFIGY_ACCENT)),
        area,
    );
}

pub fn render_browser_list(
    frame: &mut Frame<'_>,
    area: Rect,
    list_focused: bool,
    query: &DemoListQuery,
    rows: &[BrowserRow],
    selected_row_index: usize,
    total_demo_count: usize,
) {
    let block = effigy_panel_block(
        Some(" Demos "),
        false,
        if list_focused {
            EFFIGY_ACCENT
        } else {
            Color::DarkGray
        },
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(browser_list_summary_lines(
            &query_summary(query),
            rows_demo_count(rows),
            total_demo_count,
        )),
        layout[0],
    );
    let list_row_width = inner
        .width
        .saturating_sub(selected_list_highlight_symbol().chars().count() as u16)
        as usize;
    let items = rows
        .iter()
        .map(|row| match row {
            BrowserRow::Group(label) => ListItem::new(Line::from(vec![Span::styled(
                format!("  {label}"),
                Style::default()
                    .fg(EFFIGY_ACCENT)
                    .add_modifier(Modifier::BOLD),
            )])),
            BrowserRow::Demo(summary) => {
                ListItem::new(render_browser_demo_row(summary, list_row_width))
            }
        })
        .collect::<Vec<_>>();

    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(selected_row_index));
    }
    let list = List::new(items)
        .highlight_style(selected_list_highlight_style(list_focused))
        .highlight_symbol(selected_list_highlight_symbol())
        .repeat_highlight_symbol(true);
    frame.render_stateful_widget(list, layout[1], &mut state);
}

pub fn render_browser_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    focus: BrowserFocus,
    detail_tab: DetailTab,
    terminal_input_mode: bool,
    footer_message: &str,
) {
    let help = browser_help_line(focus, detail_tab, terminal_input_mode);
    let footer = Paragraph::new(vec![help, Line::from(footer_message.to_owned())])
        .block(effigy_panel_block(None, false, Color::DarkGray));
    frame.render_widget(footer, area);
}

pub fn render_browser_empty_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    total_demo_count: usize,
    query: &DemoListQuery,
) {
    let overlay = centered_rect(60, 20, area);
    frame.render_widget(Clear, overlay);
    let lines = if total_demo_count == 0 {
        vec![
            Line::from("No demos are declared in the current manifest."),
            Line::from(""),
            Line::from("Add `[demos.<id>]` entries to `effigy.toml` first."),
        ]
    } else {
        vec![
            Line::from("No demos match the current browser query."),
            Line::from(""),
            Line::from(format!("Active query: {}", query_summary(query))),
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

pub fn render_browser_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    overlay: &BrowserOverlay,
    query: &DemoListQuery,
) {
    match overlay {
        BrowserOverlay::Prompt(prompt) => render_prompt_overlay(frame, area, prompt),
        BrowserOverlay::Action(menu) => render_action_overlay(frame, area, menu),
        BrowserOverlay::Filters(menu) => render_filter_overlay(frame, area, menu, query),
    }
}

pub fn browser_header_lines(repo_root: &Path) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled(
                " Demo Browser ",
                Style::default()
                    .fg(EFFIGY_MUTED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("repo:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", repo_root.display()),
                Style::default().fg(EFFIGY_MUTED),
            ),
        ]),
        Line::from(""),
    ]
}

pub fn browser_list_summary_lines(
    query: &str,
    displayed_count: usize,
    total_count: usize,
) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("query:", Style::default().fg(Color::DarkGray)),
            Span::styled(format!(" {query}"), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("count:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {displayed_count}/{total_count}"),
                Style::default().fg(Color::White),
            ),
        ]),
    ]
}

fn render_prompt_overlay(frame: &mut Frame<'_>, area: Rect, prompt: &QueryPromptState) {
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

fn render_action_overlay(frame: &mut Frame<'_>, area: Rect, menu: &ActionMenuState) {
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

fn render_filter_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    menu: &FilterMenuState,
    query: &DemoListQuery,
) {
    let overlay = centered_rect(62, 40, area);
    frame.render_widget(Clear, overlay);
    let items = menu
        .items()
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let value = filter_menu_value(query, *item);
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

pub fn browser_terminal_key_input(key: &crossterm::event::KeyEvent) -> Option<String> {
    use crossterm::event::{KeyCode, KeyModifiers};

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

pub fn compact_kv_line(label: &str, value: &str) -> Line<'static> {
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

pub fn stacked_kv_lines(label: &str, value: &str) -> [Line<'static>; 2] {
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

pub fn section_heading(label: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        label.to_owned(),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    )])
}

pub fn selectable_detail_line(value: &str, selected: bool, focused: bool) -> Line<'static> {
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

pub fn selected_history_ordinal_from_item(item: Option<DetailSelectableItem>) -> Option<usize> {
    match item {
        Some(DetailSelectableItem::HistoryAttempt(ordinal)) => Some(ordinal),
        _ => None,
    }
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

pub fn action_menu_items_for_detail(detail: &DemoDetail) -> Vec<ActionMenuItem> {
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

pub fn detail_tab_lines(
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

pub fn muted_line(value: String) -> Line<'static> {
    Line::from(vec![Span::styled(
        value,
        Style::default().fg(Color::DarkGray),
    )])
}

pub fn next_status_filter(current: Option<DemoListStatus>) -> Option<DemoListStatus> {
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

pub fn next_gap_filter(current: Option<DemoListGap>) -> Option<DemoListGap> {
    match current {
        None => Some(DemoListGap::Existing),
        Some(DemoListGap::Existing) => Some(DemoListGap::Planned),
        Some(DemoListGap::Planned) => Some(DemoListGap::Missing),
        Some(DemoListGap::Missing) => Some(DemoListGap::Broken),
        Some(DemoListGap::Broken) => Some(DemoListGap::Stale),
        Some(DemoListGap::Stale) => None,
    }
}

pub fn normalized_prompt_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

pub fn query_summary(query: &DemoListQuery) -> String {
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

fn rows_demo_count(rows: &[BrowserRow]) -> usize {
    rows.iter()
        .filter(|row| matches!(row, BrowserRow::Demo(_)))
        .count()
}

fn filter_menu_value(query: &DemoListQuery, group_by: FilterMenuItem) -> String {
    match group_by {
        FilterMenuItem::Search => query.search.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Owner => query.owner.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Tag => query.tag.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Mode => query
            .mode
            .map(DemoListMode::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::Cover => query.cover.clone().unwrap_or_else(|| "none".to_owned()),
        FilterMenuItem::Status => query
            .status
            .map(DemoListStatus::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::Gap => query
            .gap
            .map(DemoListGap::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::StaleOnly => {
            if query.stale_only {
                "on".to_owned()
            } else {
                "off".to_owned()
            }
        }
        FilterMenuItem::GroupBy => query
            .group_by
            .map(DemoListGroupBy::as_str)
            .unwrap_or("none")
            .to_owned(),
        FilterMenuItem::ClearAll => "reset all filters".to_owned(),
    }
}

pub fn filter_change_message(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("Set {label} filter to `{value}`."),
        None => format!("Cleared {label} filter."),
    }
}

pub fn prompt_apply_message(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("Set {label} filter to `{value}`."),
        None => format!("Cleared {label} filter."),
    }
}

pub const DEMO_BROWSER_TERMINAL_COLS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_COLS";
pub const DEMO_BROWSER_TERMINAL_ROWS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_ROWS";
pub const DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK: usize = 2000;
pub const DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT: usize = 8;
pub const DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalView {
    pub lines: Vec<Line<'static>>,
    pub source: TerminalViewSource,
    pub scroll_offset: usize,
    pub stderr_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalViewSource {
    LiveAttached,
    ActiveLogs,
    InspectSnapshot,
    LatestAttemptLogs,
    Empty,
}

impl TerminalViewSource {
    pub fn label(self) -> &'static str {
        match self {
            TerminalViewSource::LiveAttached => "live attached",
            TerminalViewSource::ActiveLogs => "live terminal",
            TerminalViewSource::InspectSnapshot => "inspect snapshot",
            TerminalViewSource::LatestAttemptLogs => "latest attempt logs",
            TerminalViewSource::Empty => "none",
        }
    }
}

struct TerminalStreamSource {
    stdout_bytes: Vec<u8>,
    stderr_lines: Vec<String>,
    source: TerminalViewSource,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
}

pub fn build_terminal_view(
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

pub fn build_live_terminal_view(
    session: &mut BrowserLiveTerminalSession,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> TerminalView {
    let (mut lines, clamped_scroll) = session.terminal.render_lines(width, height, scroll_offset);
    if lines.is_empty() {
        lines.push(muted_line(
            "Terminal output exists, but no visible screen content is available yet.".to_owned(),
        ));
    }
    TerminalView {
        lines,
        source: TerminalViewSource::LiveAttached,
        scroll_offset: clamped_scroll,
        stderr_lines: Vec::new(),
    }
}

pub fn terminal_detail_render(
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
    let mut parser = vt100::Parser::new(
        parser_rows,
        parser_cols,
        DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK,
    );
    if !stdout_bytes.is_empty() {
        parser.process(stdout_bytes);
    }
    let (mut lines, clamped_scroll) = browser_vt_lines(&mut parser, width, height, scroll_offset);
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

pub fn browser_vt_lines(
    parser: &mut vt100::Parser,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> (Vec<Line<'static>>, usize) {
    render_vt_lines(parser, width, height, scroll_offset)
}

pub fn terminal_status_lines(
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

pub fn live_terminal_status_lines(
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

pub fn browser_terminal_viewport_size(total_cols: u16, total_rows: u16) -> (u16, u16) {
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
        .constraints(browser_body_constraints())
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

pub fn read_recent_log_lines(path: &Path, limit: usize) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut lines = content.lines().map(str::to_owned).collect::<Vec<_>>();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    Ok(lines)
}

pub struct BrowserLiveTerminalSession {
    pub demo_id: String,
    pub terminal: LiveTerminalBuffer,
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
    pub fn spawn(
        executable: PathBuf,
        repo_root: PathBuf,
        demo_id: String,
        subcommand: DemoSubcommand,
        viewport_size: Option<(u16, u16)>,
    ) -> Result<Self, String> {
        let mut command = ProcessCommand::new(executable);
        command.current_dir(&repo_root);
        apply_live_terminal_command_env(&mut command, viewport_size);
        match subcommand {
            DemoSubcommand::Run { demo_id } => {
                command.arg("demo").arg("run").arg(demo_id);
            }
            DemoSubcommand::Rerun { demo_id } => {
                command.arg("demo").arg("rerun").arg(demo_id);
            }
            _ => {
                return Err("live browser terminal sessions only support demo run/rerun".to_owned());
            }
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            format!("failed to launch live browser terminal session for `{demo_id}`: {error}")
        })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| {
            format!("live browser terminal session for `{demo_id}` launched without stdout pipe")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            format!("live browser terminal session for `{demo_id}` launched without stderr pipe")
        })?;
        let (sender, receiver) = mpsc::channel();
        spawn_live_terminal_reader(stdout, sender.clone());
        spawn_live_terminal_reader(stderr, sender);
        Ok(Self {
            demo_id,
            terminal: LiveTerminalBuffer::new(),
            child,
            stdin,
            receiver,
            transcript: Vec::new(),
            exit_status: None,
        })
    }

    pub fn is_running(&self) -> bool {
        self.exit_status.is_none()
    }

    pub fn drain_output(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                LiveTerminalEvent::Output(bytes) => self.append_output(&bytes),
            }
        }
    }

    fn append_output(&mut self, bytes: &[u8]) {
        let sanitized = sanitize_live_terminal_bytes(bytes);
        self.terminal.push_chunk(&sanitized);
        self.transcript.extend_from_slice(&sanitized);
        if self.transcript.len() > DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES {
            let excess = self.transcript.len() - DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES;
            self.transcript.drain(..excess);
        }
    }

    pub fn poll_exit(&mut self) -> Result<Option<(String, bool)>, String> {
        if self.exit_status.is_some() {
            return Ok(None);
        }
        let Some(status) = self.child.try_wait().map_err(|error| {
            format!(
                "failed to poll live browser terminal session for `{}`: {error}",
                self.demo_id
            )
        })?
        else {
            return Ok(None);
        };
        self.exit_status = Some(status);
        self.stdin = None;
        self.drain_output();
        self.terminal.finalize();
        Ok(Some((self.demo_id.clone(), status.success())))
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> Result<(), String> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(format!(
                "live terminal session for `{}` no longer accepts input",
                self.demo_id
            ));
        };
        stdin.write_all(bytes).map_err(|error| {
            format!(
                "failed to write live terminal input for `{}`: {error}",
                self.demo_id
            )
        })?;
        stdin.flush().map_err(|error| {
            format!(
                "failed to flush live terminal input for `{}`: {error}",
                self.demo_id
            )
        })
    }

    pub fn finish_after_stop_request(&mut self) -> Result<(), String> {
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
        self.terminal.finalize();
        Ok(())
    }
}

fn spawn_live_terminal_reader<R>(mut reader: R, sender: mpsc::Sender<LiveTerminalEvent>)
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(read) = reader.read(&mut buffer) {
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

fn apply_live_terminal_command_env(
    command: &mut ProcessCommand,
    viewport_size: Option<(u16, u16)>,
) {
    command.env_remove("NO_COLOR").env("EFFIGY_COLOR", "always");
    if let Some((cols, rows)) = viewport_size {
        command
            .env(DEMO_BROWSER_TERMINAL_COLS_ENV, cols.to_string())
            .env(DEMO_BROWSER_TERMINAL_ROWS_ENV, rows.to_string());
    }
}

pub fn sanitize_live_terminal_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut sanitized = Vec::with_capacity(bytes.len());
    let mut cursor = 0usize;
    while let Some(relative) = bytes[cursor..]
        .windows(2)
        .position(|window| window == b"^D")
    {
        let absolute = cursor + relative;
        sanitized.extend_from_slice(&bytes[cursor..absolute]);
        cursor = absolute + 2;
        while cursor < bytes.len() && bytes[cursor] == 0x08 {
            cursor += 1;
        }
    }
    sanitized.extend_from_slice(&bytes[cursor..]);
    normalize_terminal_newlines(&sanitized)
}

fn normalize_terminal_newlines(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' && normalized.last().copied() != Some(b'\r') {
            normalized.push(b'\r');
            normalized.push(b'\n');
        } else {
            normalized.push(byte);
        }
        index += 1;
    }
    normalized
}

pub fn take_complete_terminal_bytes(carry: &mut Vec<u8>, bytes: &[u8]) -> Vec<u8> {
    crate::terminal_text::take_complete_terminal_bytes(carry, bytes)
}

pub fn browser_live_terminal_env(viewport_size: Option<(u16, u16)>) -> Vec<(String, String)> {
    let mut env = vec![("EFFIGY_COLOR".to_owned(), "always".to_owned())];
    if let Some((cols, rows)) = viewport_size {
        env.push((DEMO_BROWSER_TERMINAL_COLS_ENV.to_owned(), cols.to_string()));
        env.push((DEMO_BROWSER_TERMINAL_ROWS_ENV.to_owned(), rows.to_string()));
    }
    env
}
