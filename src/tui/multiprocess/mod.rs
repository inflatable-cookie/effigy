use std::io;
use std::path::PathBuf;

use crate::process_manager::{ProcessManagerError, ProcessSpec, ProcessSupervisor};
use crate::ui::UiError;

mod config;
mod diagnostics;
mod events;
mod lifecycle;
mod render;
mod runtime_loop;
mod setup;
mod state;
mod terminal_text;
mod view_model;

use diagnostics::RuntimeDiagnostics;
use lifecycle::{shutdown_and_render_summary, TuiTerminal};
pub(super) use state::OptionsAction;
use state::SessionState;

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

struct SessionRuntime {
    supervisor: ProcessSupervisor,
    terminal: TuiTerminal,
    state: SessionState,
    diagnostics: RuntimeDiagnostics,
    vt_emulator_enabled: bool,
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

    let mut runtime = setup::prepare_runtime_session(repo_root, processes, tab_order)?;
    let result = runtime_loop::run_event_loop(&mut runtime, options);

    let non_zero_exits = shutdown_and_render_summary(
        &mut runtime.terminal,
        &runtime.supervisor,
        std::mem::take(&mut runtime.state.observed_non_zero),
        &runtime.state.process_started_at,
        &runtime.diagnostics,
    )?;

    result?;
    Ok(MultiProcessTuiOutcome { non_zero_exits })
}
