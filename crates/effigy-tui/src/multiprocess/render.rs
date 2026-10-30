use std::collections::HashMap;
use std::time::Duration;

use crate::core::{InputMode, LogEntry, ProcessExitState};
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::{Frame, Terminal};

use super::state::ProcessStartupState;
use super::OptionsAction;
use crate::waits::EntryState;

mod footer;
mod header;
mod help_overlay;
mod panes;

use footer::render_footer;
use header::{render_tabs, TabBar};
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
    pub(super) active_startup_state: ProcessStartupState,
    pub(super) active_vt: bool,
    pub(super) input_line: &'a str,
    pub(super) input_mode: InputMode,
    pub(super) shell_capture_mode: bool,
    pub(super) exit_states: &'a HashMap<String, ProcessExitState>,
    pub(super) startup_states: &'a HashMap<String, ProcessStartupState>,
    /// Each entry's own wait from the session that supervises it, when it
    /// publishes one.
    pub(super) entry_waits: &'a HashMap<String, EntryState>,
    pub(super) active_entry_wait: Option<&'a EntryState>,
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
        active_startup_state,
        active_vt,
        input_line,
        input_mode,
        shell_capture_mode,
        exit_states,
        startup_states,
        entry_waits,
        active_entry_wait,
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
        TabBar {
            process_names,
            active_index,
            shell_capture_mode,
            exit_states,
            startup_states,
            entry_waits,
        },
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
                active_startup_state,
                active_vt,
                process_name: &process_names[active_index],
                shell_capture_mode,
                active_output_seen,
                spinner_tick,
                active_elapsed,
                active_restart_count,
                exit_states,
                shell_cursor,
                active_entry_wait,
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
        footer::FooterState {
            input_mode,
            active_is_shell,
            shell_capture_mode,
            show_help,
            show_options,
            message: footer_message,
        },
    );
}

/// One session frame built from the entry snapshot another process published.
///
/// The interactive loop draws the same renderer from the same state; a
/// supervising process uses this to prove its projections frame by frame, so
/// what a proof asserts is the frame a person sees (effigy#458).
#[derive(Debug, Clone)]
pub struct FrameSnapshot<'a> {
    /// Every tab, in order.
    pub process_names: &'a [String],
    /// The selected tab.
    pub active: &'a str,
    /// The session's published entry waits.
    pub entry_waits: &'a [EntryState],
    /// The active tab's log.
    pub logs: &'a [LogEntry],
    pub width: u16,
    pub height: u16,
}

/// Draws one frame of a session from [`FrameSnapshot`] and returns its text.
pub fn render_snapshot_frame(snapshot: FrameSnapshot<'_>) -> Result<String, String> {
    let mut state = super::state::SessionState::new(
        std::path::PathBuf::new(),
        snapshot.process_names.to_vec(),
        40,
        120,
        500,
    );
    state.apply_entry_waits(snapshot.entry_waits.to_vec());
    let names = snapshot.process_names;
    let active_index = names
        .iter()
        .position(|name| name == snapshot.active)
        .ok_or_else(|| format!("`{}` is not one of the session's tabs", snapshot.active))?;
    let startup_states = names
        .iter()
        .map(|name| (name.clone(), ProcessStartupState::Waiting))
        .collect::<HashMap<_, _>>();
    let active_entry_wait = state.entry_waits.get(snapshot.active);
    let mut terminal = Terminal::new(TestBackend::new(snapshot.width, snapshot.height))
        .map_err(|error| error.to_string())?;
    let logs = snapshot.logs;
    terminal
        .draw(|frame| {
            render_ui(
                frame,
                RenderUiState {
                    process_names: names,
                    active_index,
                    active_logs: logs,
                    scroll_offset: 0,
                    max_offset: 0,
                    render_scroll_offset: 0,
                    scrollbar_total: 0,
                    follow: true,
                    active_process: snapshot.active,
                    active_startup_state: ProcessStartupState::Running,
                    active_vt: false,
                    input_line: "",
                    input_mode: InputMode::Command,
                    shell_capture_mode: false,
                    exit_states: &state.exit_states,
                    startup_states: &startup_states,
                    entry_waits: &state.entry_waits,
                    active_entry_wait,
                    show_help: false,
                    show_options: false,
                    options_index: 0,
                    footer_message: None,
                    active_output_seen: !logs.is_empty(),
                    spinner_tick: 0,
                    active_elapsed: Duration::ZERO,
                    active_restart_count: 0,
                    shell_cursor: None,
                },
            )
        })
        .map_err(|error| error.to_string())?;
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    Ok(text)
}
