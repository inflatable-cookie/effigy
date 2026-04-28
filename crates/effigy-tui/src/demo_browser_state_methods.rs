use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use effigy_cli::{
    DemoArgs, DemoListGap, DemoListGroupBy, DemoListMode, DemoListQuery, DemoListStatus,
    DemoSubcommand,
};
use effigy_demo::browser::{DemoDetail, DemoHistoryAttempt, DemoHistoryPayload};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use serde_json::Value as JsonValue;

use crate::core::{next_index, prev_index, EFFIGY_ACCENT};

use super::render::{
    action_menu_items_for_detail, artifacts_detail_render, browser_body_constraints,
    browser_terminal_key_input, clamp_artifact_index, demo_history_args, demo_input_args,
    demo_inspect_args, demo_list_args, demo_resize_args, demo_stop_args,
    detail_prefers_live_browser_terminal, detail_tab_lines, filter_change_message, first_demo_id,
    history_detail_render, next_gap_filter, next_group_by, next_mode_filter, next_status_filter,
    normalized_prompt_value, overview_detail_render, parse_demo_history_payload,
    parse_demo_inspect_payload, parse_demo_list_payload, preferred_run_action,
    prompt_apply_message, render_browser_empty_overlay, render_browser_footer,
    render_browser_header, render_browser_list, render_browser_overlay, resolve_artifact_path,
    row_contains_demo, rows_from_payload, selected_artifact,
};
use super::terminal::{
    build_live_terminal_view, build_terminal_view, live_terminal_status_lines,
    terminal_detail_render, terminal_status_lines, BrowserLiveTerminalSession,
};
use super::{
    ActionMenuItem, ActionMenuState, BackgroundDemoCommandPlan, BrowserActionMenuPlan,
    BrowserEscapeAction, BrowserFocus, BrowserHostEffect, BrowserKeyAction, BrowserOverlay,
    BrowserRefreshLoadPlan, BrowserRefreshRequests, BrowserResizeRequest, BrowserRow,
    BrowserRuntimeExecutionRequest, BrowserRuntimeLifecycleEvent, BrowserRuntimePlan,
    DemoBrowserRefreshProjection, DemoBrowserState, DetailRender, DetailSelectableItem, DetailTab,
    FilterMenuItem, FilterMenuState, PendingAction, PendingLiveTerminalLaunch, QueryPromptKind,
    QueryPromptState, DEMO_BROWSER_AUTO_REFRESH_INTERVAL,
};

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
