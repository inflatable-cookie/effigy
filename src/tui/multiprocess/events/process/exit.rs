use crate::process_manager::ProcessEvent;
use crate::tui::core::{LogEntry, LogEntryKind, ProcessExitState};

use super::super::super::diagnostics::RuntimeDiagnostics;
use super::super::super::state::SessionState;
use super::super::super::terminal_text::{
    is_expected_shutdown_diagnostic, push_entry, sanitize_log_text,
};

pub(super) fn handle_exit_event_impl(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
) {
    diagnostics.record_exit_event(&event_item.process, &event_item.payload);
    let pending_restart = state.restart_pending_for(&event_item.process);
    if pending_restart
        && (is_expected_shutdown_diagnostic(&event_item.payload)
            || event_item.payload.trim() == "exit=0")
    {
        return;
    }

    state.clear_restart_pending_for(&event_item.process);
    if event_item.payload.trim() == "exit=0" || is_expected_shutdown_diagnostic(&event_item.payload)
    {
        state.observed_non_zero.remove(&event_item.process);
        state
            .exit_states
            .insert(event_item.process.clone(), ProcessExitState::Success);
    } else {
        state
            .observed_non_zero
            .insert(event_item.process.clone(), event_item.payload.clone());
        state
            .exit_states
            .insert(event_item.process.clone(), ProcessExitState::Failure);
    }

    if let Some(buffer) = state.logs.get_mut(&event_item.process) {
        push_entry(
            buffer,
            LogEntry {
                kind: LogEntryKind::Exit,
                line: sanitize_log_text(&event_item.payload),
            },
        );
    }
}
