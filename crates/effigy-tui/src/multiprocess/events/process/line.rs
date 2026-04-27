use crate::core::LogEntryKind;
use effigy_process::ProcessEvent;

use super::super::super::diagnostics::RuntimeDiagnostics;
use super::super::super::state::SessionState;
use super::super::super::terminal_text::ingest_log_payload;

pub(super) fn handle_stdout_event_impl(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    vt_emulator_enabled: bool,
) -> bool {
    if should_skip_plain_output_due_to_vt(state, &event_item.process, vt_emulator_enabled) {
        return true;
    }
    state.mark_process_received_output(&event_item.process);
    diagnostics.record_stdout_lines(payload_line_count(&event_item.payload));
    if let Some(buffer) = state.logs.get_mut(&event_item.process) {
        ingest_log_payload(buffer, LogEntryKind::Stdout, &event_item.payload);
    }
    false
}

pub(super) fn handle_stderr_event_impl(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    vt_emulator_enabled: bool,
) -> bool {
    if should_skip_plain_output_due_to_vt(state, &event_item.process, vt_emulator_enabled) {
        return true;
    }
    state.mark_process_received_output(&event_item.process);
    diagnostics.record_stderr_lines(payload_line_count(&event_item.payload));
    if let Some(buffer) = state.logs.get_mut(&event_item.process) {
        ingest_log_payload(buffer, LogEntryKind::Stderr, &event_item.payload);
    }
    false
}

fn payload_line_count(raw: &str) -> usize {
    raw.lines().count().max(1)
}

fn should_skip_plain_output_due_to_vt(
    state: &SessionState,
    process: &str,
    vt_emulator_enabled: bool,
) -> bool {
    vt_emulator_enabled && state.vt_enabled_for(process)
}
