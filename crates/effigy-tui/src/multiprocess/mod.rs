use std::io;
use std::path::PathBuf;

use effigy_process::{
    ProcessEvent, ProcessManagerError, ProcessSpec, ProcessSupervisor, ShutdownProgress,
};
use effigy_ui::UiError;

mod config;
mod diagnostics;
mod events;
mod lifecycle;
mod render;
pub use render::{render_snapshot_frame, FrameSnapshot};
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

/// What the supervised session recorded for one entry when it ended. The
/// session reports the facts it observed; a caller decides what they mean for
/// its own result (a stopped test suite, for example, is a cancelled check).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEntryState {
    /// The entry reported a successful exit.
    Passed,
    /// The entry reported a non-zero exit, or a final state that proves no
    /// successful completion (a wait error, no exit at all).
    Failed { detail: String },
    /// The session ended the entry: it never reported an exit status of its
    /// own.
    Stopped { detail: String },
    /// The entry never started, so it never reported anything.
    NotStarted,
}

impl SessionEntryState {
    /// The state a session recorded as one text record: `exit=0` or another
    /// `exit=N`, `signal=N`, `not-tracked`/`not started`, or anything else the
    /// supervisor reported. A session that keeps a typed record reports that
    /// record instead, so a stop it caused is never read as a failure.
    pub fn from_record(record: &str) -> Self {
        let record = record.trim();
        if record == "exit=0" {
            return Self::Passed;
        }
        if let Some(signal) = record
            .strip_prefix("signal=")
            .and_then(|value| value.parse::<i32>().ok())
        {
            // Only a signal a stop sends says the session ended the entry. A
            // process that died from another one crashed, which is a failure.
            return if effigy_process::interrupted_signal(signal) {
                Self::Stopped {
                    detail: record.to_owned(),
                }
            } else {
                Self::Failed {
                    detail: record.to_owned(),
                }
            };
        }
        if record == "not-tracked" || record == "not started" {
            return Self::NotStarted;
        }
        Self::Failed {
            detail: record.to_owned(),
        }
    }
}

/// One supervised entry's recorded result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEntry {
    pub name: String,
    pub state: SessionEntryState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiProcessTuiOutcome {
    /// Every entry the session supervised, with the outcome it recorded.
    pub entries: Vec<SessionEntry>,
    /// The session ended because its operator quit it (or its terminal went
    /// away), not because the entries finished.
    pub stopped_by_user: bool,
}

/// The processes one TUI session renders: children it supervises itself, or
/// the workloads of a run another process supervises. The TUI reads events
/// and sends input and stop requests; it never changes what runs.
pub trait SessionProcesses {
    /// Each entry's own wait, when another process supervises the entries.
    /// A session that supervises its own processes leaves this empty: it has
    /// no resolved ordering graph to derive a wait from.
    fn entry_waits(&self) -> Vec<crate::waits::EntryState> {
        Vec::new()
    }
    fn close_requested(&self) -> bool {
        false
    }
    fn next_event_timeout(&self, timeout: std::time::Duration) -> Option<ProcessEvent>;
    fn send_input(&self, process: &str, input: &str) -> Result<(), ProcessManagerError>;
    fn terminate_process(&self, process: &str) -> Result<(), ProcessManagerError>;
    fn restart_process(&self, process: &str) -> Result<(), ProcessManagerError>;
    fn startup_finished(&self) -> bool;
    fn cancel_startup(&self);
    fn take_startup_error(&self) -> Option<ProcessManagerError>;
    /// Every supervised entry's final state, as this session recorded it. This
    /// is the session's one outcome report: the summary renders it and the
    /// caller projects it, so no caller infers a result from silence.
    fn entry_states(&self) -> Vec<(String, SessionEntryState)>;
    fn terminate_all_graceful_with_progress(
        &self,
        timeout: std::time::Duration,
        on_progress: &mut dyn FnMut(ShutdownProgress),
    );
}

impl SessionProcesses for ProcessSupervisor {
    fn next_event_timeout(&self, timeout: std::time::Duration) -> Option<ProcessEvent> {
        ProcessSupervisor::next_event_timeout(self, timeout)
    }
    fn send_input(&self, process: &str, input: &str) -> Result<(), ProcessManagerError> {
        ProcessSupervisor::send_input(self, process, input)
    }
    fn terminate_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        ProcessSupervisor::terminate_process(self, process)
    }
    fn restart_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        ProcessSupervisor::restart_process(self, process)
    }
    fn startup_finished(&self) -> bool {
        ProcessSupervisor::startup_finished(self)
    }
    fn cancel_startup(&self) {
        ProcessSupervisor::cancel_startup(self)
    }
    fn take_startup_error(&self) -> Option<ProcessManagerError> {
        ProcessSupervisor::take_startup_error(self)
    }
    fn entry_states(&self) -> Vec<(String, SessionEntryState)> {
        ProcessSupervisor::exit_diagnostics(self)
            .into_iter()
            .map(|(name, record)| (name, SessionEntryState::from_record(&record)))
            .collect()
    }
    fn terminate_all_graceful_with_progress(
        &self,
        timeout: std::time::Duration,
        on_progress: &mut dyn FnMut(ShutdownProgress),
    ) {
        ProcessSupervisor::terminate_all_graceful_with_progress(self, timeout, on_progress)
    }
}

/// What a TUI session shows for one process tab when another process
/// supervises it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTab {
    pub name: String,
    pub pty: bool,
    pub shutdown_on_exit: bool,
}

/// Host-owned session controls. Shutdown hooks run even when the event loop
/// fails, and the UI always stops its children before reporting hook errors.
pub struct MultiProcessTuiControl<'a> {
    pub poll: &'a dyn Fn(&dyn SessionProcesses) -> Result<bool, String>,
    pub before_shutdown: &'a dyn Fn(&dyn SessionProcesses) -> Result<(), String>,
    pub after_shutdown: &'a dyn Fn(&dyn SessionProcesses) -> Result<(), String>,
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

struct SessionRuntime<'a> {
    supervisor: Box<dyn SessionProcesses + 'a>,
    terminal: TuiTerminal,
    state: SessionState,
    diagnostics: RuntimeDiagnostics,
    vt_emulator_enabled: bool,
    /// The terminal hung up: nothing can be restored or written to it.
    terminal_lost: bool,
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

    let runtime = setup::prepare_runtime_session(repo_root, processes, tab_order)?;
    run_session(runtime, options, control)
}

/// A TUI over processes `source` supervises; `tabs` name them in order.
pub fn run_multiprocess_tui_with_source<'a>(
    repo_root: PathBuf,
    source: Box<dyn SessionProcesses + 'a>,
    tabs: Vec<SessionTab>,
    tab_order: Vec<String>,
    options: MultiProcessTuiOptions,
    control: Option<&MultiProcessTuiControl<'_>>,
) -> Result<MultiProcessTuiOutcome, MultiProcessTuiError> {
    if tabs.is_empty() {
        return Err(MultiProcessTuiError::NoProcesses);
    }
    let runtime = setup::prepare_source_session(repo_root, source, tabs, tab_order)?;
    run_session(runtime, options, control)
}

fn run_session(
    mut runtime: SessionRuntime<'_>,
    options: MultiProcessTuiOptions,
    control: Option<&MultiProcessTuiControl<'_>>,
) -> Result<MultiProcessTuiOutcome, MultiProcessTuiError> {
    let result = runtime_loop::run_event_loop_with_control(&mut runtime, options, control);
    runtime.supervisor.cancel_startup();
    let before_shutdown =
        control.map(|control| (control.before_shutdown)(runtime.supervisor.as_ref()));
    let startup_error = runtime.supervisor.take_startup_error();

    let summary_result = shutdown_and_render_summary(
        &mut runtime.terminal,
        runtime.supervisor.as_ref(),
        &runtime.state.logs,
        &runtime.state.process_started_at,
        &runtime.diagnostics,
    );

    let after_shutdown =
        control.map(|control| (control.after_shutdown)(runtime.supervisor.as_ref()));
    if runtime.terminal_lost {
        // Dropping the terminal would try to show the cursor on a closed
        // terminal and report the failure on its closed stderr.
        let SessionRuntime { terminal, .. } = runtime;
        std::mem::forget(terminal);
    }
    let entries = summary_result?;
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
        entries,
        stopped_by_user,
    })
}
