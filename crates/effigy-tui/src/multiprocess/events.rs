use crossterm::event::KeyEvent;

use effigy_process::{ProcessEventKind, ProcessSupervisor};

use super::config::EVENT_DRAIN_WAIT;
use super::diagnostics::RuntimeDiagnostics;
use super::state::SessionState;
use super::{MultiProcessTuiError, MultiProcessTuiOptions};

mod input;
mod key_command;
mod key_insert;
mod options;
mod process;
use key_command::{handle_command_key, handle_pre_dispatch_key, handle_shell_shortcuts};
use key_insert::handle_insert_key;
use process::{handle_chunk_event, handle_exit_event, handle_stderr_event, handle_stdout_event};

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
                handle_chunk_event(&event_item, state, diagnostics, vt_emulator_enabled)
            }
            ProcessEventKind::Stdout => {
                if handle_stdout_event(&event_item, state, diagnostics, vt_emulator_enabled) {
                    continue;
                }
            }
            ProcessEventKind::Stderr => {
                if handle_stderr_event(&event_item, state, diagnostics, vt_emulator_enabled) {
                    continue;
                }
            }
            ProcessEventKind::Exit => handle_exit_event(&event_item, state, diagnostics),
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
    if state.input_mode == crate::core::InputMode::Insert {
        handle_insert_key(key, supervisor, state)?;
        return Ok(LoopControl::Continue);
    }
    Ok(handle_command_key(key, state, max_offset))
}
#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;
