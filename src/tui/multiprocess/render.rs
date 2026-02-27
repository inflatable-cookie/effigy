use std::collections::HashMap;
use std::time::Duration;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use super::{InputMode, LogEntry, OptionsAction, ProcessExitState};

mod footer;
mod header;
mod help_overlay;
mod panes;

use footer::render_footer;
use header::render_tabs;
use help_overlay::{render_help_overlay, render_options_overlay};
use panes::{render_input_pane, render_output_pane};

pub(super) fn options_actions(follow_enabled: bool) -> Vec<OptionsAction> {
    help_overlay::options_actions(follow_enabled)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_ui(
    frame: &mut Frame<'_>,
    process_names: &[String],
    active_index: usize,
    active_logs: &[LogEntry],
    scroll_offset: usize,
    max_offset: usize,
    render_scroll_offset: usize,
    scrollbar_total: usize,
    follow: bool,
    active_process: &str,
    input_line: &str,
    input_mode: InputMode,
    shell_capture_mode: bool,
    exit_states: &HashMap<String, ProcessExitState>,
    show_help: bool,
    show_options: bool,
    options_index: usize,
    active_output_seen: bool,
    spinner_tick: usize,
    active_elapsed: Duration,
    active_restart_count: usize,
    shell_cursor: Option<(u16, u16)>,
) {
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
            active_logs,
            scroll_offset,
            max_offset,
            render_scroll_offset,
            scrollbar_total,
            active_process,
            &process_names[active_index],
            shell_capture_mode,
            active_output_seen,
            spinner_tick,
            active_elapsed,
            active_restart_count,
            exit_states,
            shell_cursor,
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
    );
}
