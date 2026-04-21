use std::collections::HashMap;
use std::time::Duration;

use crate::core::{InputMode, LogEntry, ProcessExitState};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use super::OptionsAction;

mod footer;
mod header;
mod help_overlay;
mod panes;

use footer::render_footer;
use header::render_tabs;
use help_overlay::{render_help_overlay, render_options_overlay};
use panes::{render_input_pane, render_output_pane, OutputPaneRenderArgs};

pub(super) fn options_actions(follow_enabled: bool) -> Vec<OptionsAction> {
    help_overlay::options_actions(follow_enabled)
}

pub(super) struct RenderUiState<'a> {
    pub(super) process_names: &'a [String],
    pub(super) active_index: usize,
    pub(super) active_logs: &'a [LogEntry],
    pub(super) scroll_offset: usize,
    pub(super) max_offset: usize,
    pub(super) render_scroll_offset: usize,
    pub(super) scrollbar_total: usize,
    pub(super) follow: bool,
    pub(super) active_process: &'a str,
    pub(super) input_line: &'a str,
    pub(super) input_mode: InputMode,
    pub(super) shell_capture_mode: bool,
    pub(super) exit_states: &'a HashMap<String, ProcessExitState>,
    pub(super) show_help: bool,
    pub(super) show_options: bool,
    pub(super) options_index: usize,
    pub(super) footer_message: Option<&'a str>,
    pub(super) active_output_seen: bool,
    pub(super) spinner_tick: usize,
    pub(super) active_elapsed: Duration,
    pub(super) active_restart_count: usize,
    pub(super) shell_cursor: Option<(u16, u16)>,
}

pub(super) fn render_ui(frame: &mut Frame<'_>, state: RenderUiState<'_>) {
    let RenderUiState {
        process_names,
        active_index,
        active_logs,
        scroll_offset,
        max_offset,
        render_scroll_offset,
        scrollbar_total,
        follow,
        active_process,
        input_line,
        input_mode,
        shell_capture_mode,
        exit_states,
        show_help,
        show_options,
        options_index,
        footer_message,
        active_output_seen,
        spinner_tick,
        active_elapsed,
        active_restart_count,
        shell_cursor,
    } = state;
    let active_is_shell = active_process == "shell";
    let input_height = if active_is_shell {
        0
    } else if input_mode == InputMode::Insert {
        3
    } else {
        0
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_tabs(
        frame,
        chunks[0],
        process_names,
        active_index,
        shell_capture_mode,
        exit_states,
    );

    if show_help {
        render_help_overlay(frame, chunks[1]);
    } else {
        render_output_pane(
            frame,
            chunks[1],
            OutputPaneRenderArgs {
                active_logs,
                scroll_offset,
                max_offset,
                render_scroll_offset,
                scrollbar_total,
                active_process,
                process_name: &process_names[active_index],
                shell_capture_mode,
                active_output_seen,
                spinner_tick,
                active_elapsed,
                active_restart_count,
                exit_states,
                shell_cursor,
            },
        );
    }

    if show_options {
        render_options_overlay(
            frame,
            process_names[active_index].as_str(),
            options_index,
            follow,
        );
    }

    render_input_pane(frame, chunks[2], active_is_shell, input_mode, input_line);
    render_footer(
        frame,
        chunks[3],
        input_mode,
        active_is_shell,
        shell_capture_mode,
        show_help,
        show_options,
        footer_message,
    );
}
