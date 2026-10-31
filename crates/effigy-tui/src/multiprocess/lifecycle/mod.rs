use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::multiprocess::{SessionEntry, SessionProcesses};

use super::diagnostics::RuntimeDiagnostics;
use super::MultiProcessTuiError;
use crate::core::LogEntry;

mod progress;
mod summary;
mod terminal;

pub(super) use terminal::{init_terminal, TuiTerminal};

pub(super) fn restore_terminal(terminal: &mut TuiTerminal) -> Result<(), std::io::Error> {
    terminal::restore_terminal(terminal)
}

pub(super) fn shutdown_and_render_summary(
    terminal: &mut TuiTerminal,
    supervisor: &dyn SessionProcesses,
    process_logs: &HashMap<String, VecDeque<LogEntry>>,
    process_started_at: &HashMap<String, Instant>,
    diagnostics: &RuntimeDiagnostics,
) -> Result<Vec<SessionEntry>, MultiProcessTuiError> {
    progress::run_shutdown_with_progress(terminal, supervisor);

    // The supervisor's own final state for every entry it supervised is the
    // session's one outcome authority: the same facts the summary renders are
    // the facts a caller projects, so no caller re-derives a pass from the
    // absence of a failure.
    let entries = summary::collect_entry_states(&supervisor.exit_diagnostics());

    terminal::restore_terminal(terminal)?;
    summary::render_process_summary(&entries, process_logs, process_started_at, diagnostics)?;

    Ok(entries)
}
