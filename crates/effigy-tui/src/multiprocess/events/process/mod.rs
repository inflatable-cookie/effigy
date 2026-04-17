use std::collections::HashMap;

use crate::core::ProcessExitState;
use effigy_process::ProcessEvent;

use super::super::diagnostics::RuntimeDiagnostics;
use super::super::state::SessionState;

mod chunk;
mod exit;
mod line;
#[cfg(test)]
mod tests;

pub(super) fn all_processes_exited(
    exit_states: &HashMap<String, ProcessExitState>,
    process_count: usize,
) -> bool {
    process_count > 0 && exit_states.len() >= process_count
}

pub(super) fn handle_chunk_event(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    vt_emulator_enabled: bool,
) {
    chunk::handle_chunk_event_impl(event_item, state, diagnostics, vt_emulator_enabled);
}

pub(super) fn handle_stdout_event(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    vt_emulator_enabled: bool,
) -> bool {
    line::handle_stdout_event_impl(event_item, state, diagnostics, vt_emulator_enabled)
}

pub(super) fn handle_stderr_event(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    vt_emulator_enabled: bool,
) -> bool {
    line::handle_stderr_event_impl(event_item, state, diagnostics, vt_emulator_enabled)
}

pub(super) fn handle_exit_event(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
) {
    exit::handle_exit_event_impl(event_item, state, diagnostics);
}
