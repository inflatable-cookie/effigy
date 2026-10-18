use std::io;
use std::path::PathBuf;

use effigy_process::{ProcessManagerError, ProcessSpec, ProcessSupervisor};
use effigy_ui::UiError;

mod config;
mod diagnostics;
mod events;
mod lifecycle;
mod render;
mod runtime_loop;
mod setup;
mod state;
pub(crate) mod terminal_text;
mod transcript;
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
    Control(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiProcessTuiOutcome {
    pub non_zero_exits: Vec<(String, String)>,
    pub stopped_by_user: bool,
}

/// Host-owned session controls. Shutdown hooks run even when the event loop
/// fails, and the UI always stops its children before reporting hook errors.
pub struct MultiProcessTuiControl<'a> {
    pub poll: &'a dyn Fn(&ProcessSupervisor) -> Result<bool, String>,
    pub before_shutdown: &'a dyn Fn(&ProcessSupervisor) -> Result<(), String>,
    pub after_shutdown: &'a dyn Fn(&ProcessSupervisor) -> Result<(), String>,
}

impl std::fmt::Debug for MultiProcessTuiControl<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MultiProcessTuiControl")
            .finish_non_exhaustive()
    }
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
            MultiProcessTuiError::Control(error) => write!(f, "{error}"),
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

#[cfg(all(test, unix))]
#[path = "startup_visibility.rs"]
mod startup_visibility;

pub fn run_multiprocess_tui(
    repo_root: PathBuf,
    processes: Vec<ProcessSpec>,
    tab_order: Vec<String>,
    options: MultiProcessTuiOptions,
) -> Result<MultiProcessTuiOutcome, MultiProcessTuiError> {
    run_multiprocess_tui_with_control(repo_root, processes, tab_order, options, None)
}

pub fn run_multiprocess_tui_with_control(
    repo_root: PathBuf,
    processes: Vec<ProcessSpec>,
    tab_order: Vec<String>,
    options: MultiProcessTuiOptions,
    control: Option<&MultiProcessTuiControl<'_>>,
) -> Result<MultiProcessTuiOutcome, MultiProcessTuiError> {
    if processes.is_empty() {
        return Err(MultiProcessTuiError::NoProcesses);
    }

    let mut runtime = setup::prepare_runtime_session(repo_root, processes, tab_order)?;
    let result = runtime_loop::run_event_loop_with_control(&mut runtime, options, control);
    runtime.supervisor.cancel_startup();
    let before_shutdown = control.map(|control| (control.before_shutdown)(&runtime.supervisor));
    let startup_error = runtime.supervisor.take_startup_error();

    let summary_result = shutdown_and_render_summary(
        &mut runtime.terminal,
        &runtime.supervisor,
        std::mem::take(&mut runtime.state.observed_non_zero),
        &runtime.state.logs,
        &runtime.state.process_started_at,
        &runtime.diagnostics,
    );

    let after_shutdown = control.map(|control| (control.after_shutdown)(&runtime.supervisor));
    let non_zero_exits = summary_result?;
    let stopped_by_user = result?;
    if let Some(result) = before_shutdown {
        result.map_err(MultiProcessTuiError::Control)?;
    }
    if let Some(result) = after_shutdown {
        result.map_err(MultiProcessTuiError::Control)?;
    }
    if let Some(error) = startup_error.filter(|_| !stopped_by_user) {
        return Err(MultiProcessTuiError::Process(error));
    }
    Ok(MultiProcessTuiOutcome {
        non_zero_exits,
        stopped_by_user,
    })
}
