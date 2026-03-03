use std::collections::HashMap;

use crate::process_manager::{ProcessEvent, ProcessEventKind};
use crate::tui::core::{LogEntry, LogEntryKind, ProcessExitState};

use super::super::config::{VT_PARSER_COLS, VT_PARSER_ROWS, VT_PARSER_SCROLLBACK};
use super::super::diagnostics::RuntimeDiagnostics;
use super::super::state::SessionState;
use super::super::terminal_text::{
    ingest_log_payload, is_expected_shutdown_diagnostic, push_entry, sanitize_log_text,
};

pub(super) fn all_processes_exited(
    exit_states: &HashMap<String, ProcessExitState>,
    process_count: usize,
) -> bool {
    process_count > 0 && exit_states.len() >= process_count
}

pub(super) fn payload_line_count(raw: &str) -> usize {
    raw.lines().count().max(1)
}

pub(super) fn should_skip_plain_output_due_to_vt(
    state: &SessionState,
    process: &str,
    vt_emulator_enabled: bool,
) -> bool {
    vt_emulator_enabled && state.vt_saw_chunk_for(process)
}

pub(super) fn handle_chunk_event(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    vt_emulator_enabled: bool,
) {
    let had_output = state.mark_process_received_output(&event_item.process);
    if !vt_emulator_enabled {
        return;
    }
    if !had_output {
        state.vt_parsers.insert(
            event_item.process.clone(),
            vt100::Parser::new(VT_PARSER_ROWS, VT_PARSER_COLS, VT_PARSER_SCROLLBACK),
        );
        state.set_vt_saw_chunk_for(&event_item.process, false);
        diagnostics.record_vt_reset(&event_item.process);
    }
    let Some(chunk) = event_item.chunk.as_ref() else {
        return;
    };
    let Some(parser) = state.vt_parser_mut_for(&event_item.process) else {
        return;
    };
    parser.process(chunk);
    state.set_vt_saw_chunk_for(&event_item.process, true);
    match event_item.kind {
        ProcessEventKind::StdoutChunk => {
            diagnostics.record_stdout_chunk(&event_item.process, chunk.len())
        }
        ProcessEventKind::StderrChunk => {
            diagnostics.record_stderr_chunk(&event_item.process, chunk.len())
        }
        _ => {}
    }
}

pub(super) fn handle_stdout_event(
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

pub(super) fn handle_stderr_event(
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

pub(super) fn handle_exit_event(
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
