use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyEventKind};

use crate::process_manager::{ProcessManagerError, ProcessSpec, ProcessSupervisor};
use crate::ui::UiError;

mod config;
mod diagnostics;
mod events;
mod lifecycle;
mod render;
mod state;
mod terminal_text;
mod view_model;

use config::{
    INPUT_POLL_WAIT, MAX_EVENTS_PER_TICK, VT_PARSER_COLS, VT_PARSER_ROWS, VT_PARSER_SCROLLBACK,
};
use diagnostics::RuntimeDiagnostics;
use events::{drain_process_events, handle_key_event, LoopControl};
use lifecycle::{init_terminal, shutdown_and_render_summary};
use render::render_ui;
pub(super) use state::OptionsAction;
use state::SessionState;
use view_model::build_active_view_model;

#[derive(Debug)]
pub enum MultiProcessTuiError {
    Io(io::Error),
    Ui(UiError),
    Process(ProcessManagerError),
    NoProcesses,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiProcessTuiOutcome {
    pub non_zero_exits: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MultiProcessTuiOptions {
    pub esc_quit_on_complete: bool,
}

impl std::fmt::Display for MultiProcessTuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiProcessTuiError::Io(err) => write!(f, "{err}"),
            MultiProcessTuiError::Ui(err) => write!(f, "{err}"),
            MultiProcessTuiError::Process(err) => write!(f, "{err}"),
            MultiProcessTuiError::NoProcesses => write!(f, "managed TUI session has no processes"),
        }
    }
}

impl std::error::Error for MultiProcessTuiError {}

impl From<io::Error> for MultiProcessTuiError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProcessManagerError> for MultiProcessTuiError {
    fn from(value: ProcessManagerError) -> Self {
        Self::Process(value)
    }
}

impl From<UiError> for MultiProcessTuiError {
    fn from(value: UiError) -> Self {
        Self::Ui(value)
    }
}

pub fn run_multiprocess_tui(
    repo_root: PathBuf,
    processes: Vec<ProcessSpec>,
    tab_order: Vec<String>,
    options: MultiProcessTuiOptions,
) -> Result<MultiProcessTuiOutcome, MultiProcessTuiError> {
    if processes.is_empty() {
        return Err(MultiProcessTuiError::NoProcesses);
    }

    let process_names = processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
    let process_names = if tab_order.is_empty() {
        process_names
    } else {
        tab_order
    };
    let supervisor = ProcessSupervisor::spawn(repo_root, processes)?;
    let mut terminal = init_terminal()?;
    let mut state = SessionState::new(
        process_names,
        VT_PARSER_ROWS,
        VT_PARSER_COLS,
        VT_PARSER_SCROLLBACK,
    );
    let mut diagnostics = RuntimeDiagnostics::from_env();

    let vt_emulator_enabled = std::env::var("EFFIGY_TUI_VT100")
        .ok()
        .is_none_or(|value| value != "0" && !value.eq_ignore_ascii_case("false"));

    let result: Result<(), MultiProcessTuiError> = loop {
        drain_process_events(
            &supervisor,
            &mut state,
            &mut diagnostics,
            MAX_EVENTS_PER_TICK,
            vt_emulator_enabled,
        );
        state.spinner_tick = state.spinner_tick.wrapping_add(1);

        let size = terminal.size()?;
        let output_height = size.height.saturating_sub(9) as usize;
        let output_width = size.width.saturating_sub(4) as usize;
        let active_view =
            build_active_view_model(&mut state, output_height, output_width, vt_emulator_enabled);

        terminal.draw(|frame| {
            render_ui(
                frame,
                &state.process_names,
                state.active_index,
                &active_view.active_logs,
                active_view.scroll_offset,
                active_view.max_offset,
                active_view.render_scroll_offset,
                active_view.scrollbar_total,
                active_view.is_follow,
                &active_view.active_process,
                &state.input_line,
                state.input_mode,
                state.shell_capture_mode,
                &state.exit_states,
                state.show_help,
                state.show_options,
                state.options_index,
                active_view.active_output_seen,
                state.spinner_tick,
                active_view.active_elapsed,
                active_view.active_restart_count,
                active_view.shell_cursor,
            )
        })?;
        diagnostics.record_frame();

        if event::poll(INPUT_POLL_WAIT)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match handle_key_event(
                    &key,
                    &supervisor,
                    &mut state,
                    &mut diagnostics,
                    options,
                    active_view.max_offset,
                )? {
                    LoopControl::Continue => {}
                    LoopControl::Quit => break Ok(()),
                }
            }
        }
    };

    let non_zero_exits = shutdown_and_render_summary(
        &mut terminal,
        &supervisor,
        state.observed_non_zero,
        &state.process_started_at,
        &diagnostics,
    )?;

    result?;
    Ok(MultiProcessTuiOutcome { non_zero_exits })
}
