use std::collections::HashMap;

use crate::tui::core::ProcessExitState;

use super::super::state::SessionState;

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
