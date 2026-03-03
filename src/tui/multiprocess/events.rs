use crossterm::event::KeyEvent;

use crate::process_manager::{ProcessEventKind, ProcessSupervisor};
use crate::tui::core::{LogEntry, LogEntryKind, ProcessExitState};

use super::config::{EVENT_DRAIN_WAIT, VT_PARSER_COLS, VT_PARSER_ROWS, VT_PARSER_SCROLLBACK};
use super::diagnostics::RuntimeDiagnostics;
use super::state::SessionState;
use super::terminal_text::{
    ingest_log_payload, is_expected_shutdown_diagnostic, push_entry, sanitize_log_text,
};
use super::{MultiProcessTuiError, MultiProcessTuiOptions};

mod input;
mod key_command;
mod key_insert;
mod options;
mod process;
use key_command::{handle_command_key, handle_pre_dispatch_key, handle_shell_shortcuts};
use key_insert::handle_insert_key;
use process::{payload_line_count, should_skip_plain_output_due_to_vt};

pub(super) enum LoopControl {
    Continue,
    Quit,
}

pub(super) struct KeyEventContext<'a> {
    pub(super) supervisor: &'a ProcessSupervisor,
    pub(super) state: &'a mut SessionState,
    pub(super) diagnostics: &'a mut RuntimeDiagnostics,
    pub(super) options: MultiProcessTuiOptions,
    pub(super) max_offset: usize,
}

pub(super) fn drain_process_events(
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    max_events: usize,
    vt_emulator_enabled: bool,
) {
    let mut drained_events = 0usize;
    while drained_events < max_events {
        let Some(event_item) = supervisor.next_event_timeout(EVENT_DRAIN_WAIT) else {
            break;
        };
        drained_events += 1;
        if !state.logs.contains_key(&event_item.process) {
            continue;
        }
        match event_item.kind {
            ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {
                let had_output = state.mark_process_received_output(&event_item.process);
                if vt_emulator_enabled {
                    if !had_output {
                        state.vt_parsers.insert(
                            event_item.process.clone(),
                            vt100::Parser::new(
                                VT_PARSER_ROWS,
                                VT_PARSER_COLS,
                                VT_PARSER_SCROLLBACK,
                            ),
                        );
                        state.set_vt_saw_chunk_for(&event_item.process, false);
                        diagnostics.record_vt_reset(&event_item.process);
                    }
                    if let Some(chunk) = event_item.chunk.as_ref() {
                        if let Some(parser) = state.vt_parser_mut_for(&event_item.process) {
                            parser.process(chunk);
                            state.set_vt_saw_chunk_for(&event_item.process, true);
                            match event_item.kind {
                                ProcessEventKind::StdoutChunk => diagnostics
                                    .record_stdout_chunk(&event_item.process, chunk.len()),
                                ProcessEventKind::StderrChunk => diagnostics
                                    .record_stderr_chunk(&event_item.process, chunk.len()),
                                _ => {}
                            }
                        }
                    }
                }
            }
            ProcessEventKind::Stdout => {
                if should_skip_plain_output_due_to_vt(
                    state,
                    &event_item.process,
                    vt_emulator_enabled,
                ) {
                    continue;
                }
                state.mark_process_received_output(&event_item.process);
                diagnostics.record_stdout_lines(payload_line_count(&event_item.payload));
                if let Some(buffer) = state.logs.get_mut(&event_item.process) {
                    ingest_log_payload(buffer, LogEntryKind::Stdout, &event_item.payload);
                }
            }
            ProcessEventKind::Stderr => {
                if should_skip_plain_output_due_to_vt(
                    state,
                    &event_item.process,
                    vt_emulator_enabled,
                ) {
                    continue;
                }
                state.mark_process_received_output(&event_item.process);
                diagnostics.record_stderr_lines(payload_line_count(&event_item.payload));
                if let Some(buffer) = state.logs.get_mut(&event_item.process) {
                    ingest_log_payload(buffer, LogEntryKind::Stderr, &event_item.payload);
                }
            }
            ProcessEventKind::Exit => {
                diagnostics.record_exit_event(&event_item.process, &event_item.payload);
                let pending_restart = state.restart_pending_for(&event_item.process);
                if pending_restart
                    && (is_expected_shutdown_diagnostic(&event_item.payload)
                        || event_item.payload.trim() == "exit=0")
                {
                    continue;
                }
                state.clear_restart_pending_for(&event_item.process);
                if event_item.payload.trim() == "exit=0"
                    || is_expected_shutdown_diagnostic(&event_item.payload)
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
        }
    }
}

pub(super) fn handle_key_event(
    key: &KeyEvent,
    context: KeyEventContext<'_>,
) -> Result<LoopControl, MultiProcessTuiError> {
    let KeyEventContext {
        supervisor,
        state,
        diagnostics,
        options,
        max_offset,
    } = context;
    diagnostics.record_keypress(key);
    let active_process = state.active_process().to_owned();
    let active_is_shell = active_process == "shell";

    if let Some(control) =
        handle_shell_shortcuts(key, supervisor, state, &active_process, active_is_shell)?
    {
        return Ok(control);
    }
    if let Some(control) = handle_pre_dispatch_key(key, state, options) {
        return Ok(control);
    }
    if let Some(control) = options::handle_options_overlay_key(key, supervisor, state, max_offset)?
    {
        return Ok(control);
    }
    if state.input_mode == crate::tui::core::InputMode::Insert {
        handle_insert_key(key, supervisor, state)?;
        return Ok(LoopControl::Continue);
    }
    Ok(handle_command_key(key, state, max_offset))
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::tui::core::{next_index, prev_index, toggle_follow_for_active, ProcessExitState};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::input::shell_key_input;
    use super::process::all_processes_exited;

    #[test]
    fn all_processes_exited_requires_full_count() {
        let mut exits = HashMap::new();
        exits.insert("a".to_owned(), ProcessExitState::Success);
        assert!(!all_processes_exited(&exits, 2));
        exits.insert("b".to_owned(), ProcessExitState::Failure);
        assert!(all_processes_exited(&exits, 2));
    }

    #[test]
    fn tab_index_helpers_wrap_correctly() {
        assert_eq!(next_index(0, 4), 1);
        assert_eq!(next_index(3, 4), 0);
        assert_eq!(prev_index(0, 4), 3);
        assert_eq!(prev_index(2, 4), 1);
    }

    #[test]
    fn toggle_follow_updates_mode_and_offset() {
        let mut follow = HashMap::from([("api".to_owned(), false)]);
        let mut offsets = HashMap::from([("api".to_owned(), 1usize)]);
        toggle_follow_for_active(&mut follow, &mut offsets, "api", 42);
        assert_eq!(follow.get("api"), Some(&true));
        assert_eq!(offsets.get("api"), Some(&42usize));

        toggle_follow_for_active(&mut follow, &mut offsets, "api", 99);
        assert_eq!(follow.get("api"), Some(&false));
        assert_eq!(offsets.get("api"), Some(&42usize));
    }

    #[test]
    fn shell_key_input_maps_control_keys() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(shell_key_input(&key), Some("\u{3}".to_owned()));
        let key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(shell_key_input(&key), Some("\u{1b}[D".to_owned()));
    }
}
