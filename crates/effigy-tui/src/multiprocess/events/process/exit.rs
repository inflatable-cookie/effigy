use crate::core::{LogEntry, LogEntryKind, ProcessExitState};
use effigy_process::ProcessEvent;

use super::super::super::diagnostics::RuntimeDiagnostics;
use super::super::super::state::SessionState;
use super::super::super::terminal_text::{push_entry, sanitize_log_text};

pub(super) fn handle_exit_event_impl(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
) {
    diagnostics.record_exit_event(&event_item.process, &event_item.payload);
    // A restart is already replacing this attempt: its exit carries no verdict
    // for the entry, which the restarted process will report.
    if state.restart_pending_for(&event_item.process) {
        return;
    }

    state.clear_restart_pending_for(&event_item.process);
    let payload = event_item.payload.trim();
    // The payload is the entry's own exit fact: only exit=0 is success. A
    // signal is a stop, and anything else is a failure; none of them is
    // repaired into a pass.
    state.exit_states.insert(
        event_item.process.clone(),
        if payload == "exit=0" {
            ProcessExitState::Success
        } else if payload.starts_with("signal=") {
            ProcessExitState::Stopped
        } else {
            ProcessExitState::Failure
        },
    );

    if let Some(buffer) = state.logs.get_mut(&event_item.process) {
        push_entry(
            buffer,
            LogEntry {
                styles: None,
                kind: LogEntryKind::Exit,
                line: sanitize_log_text(&event_item.payload),
            },
        );
    }

    if state
        .shutdown_on_exit_processes
        .contains(&event_item.process)
    {
        state.shutdown_requested = true;
    }
}
