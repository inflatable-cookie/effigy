use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::Instant;

use effigy_process::ProcessSupervisor;

use super::diagnostics::RuntimeDiagnostics;
use super::MultiProcessTuiError;
use crate::core::LogEntry;

mod progress;
mod summary;
mod terminal;

pub(super) use terminal::{init_terminal, TuiTerminal};

pub(super) fn shutdown_and_render_summary(
    terminal: &mut TuiTerminal,
    supervisor: &ProcessSupervisor,
    observed_non_zero: HashMap<String, String>,
    process_logs: &HashMap<String, VecDeque<LogEntry>>,
    process_started_at: &HashMap<String, Instant>,
    diagnostics: &RuntimeDiagnostics,
) -> Result<Vec<(String, String)>, MultiProcessTuiError> {
    progress::run_shutdown_with_progress(terminal, supervisor);

    let process_diagnostics = supervisor.exit_diagnostics();
    let non_zero_exits =
        summary::collect_non_zero_exits(observed_non_zero, process_diagnostics.clone());

    terminal::restore_terminal(terminal)?;
    summary::render_process_summary(
        process_diagnostics,
        process_logs,
        process_started_at,
        diagnostics,
    )?;

    Ok(non_zero_exits)
}
