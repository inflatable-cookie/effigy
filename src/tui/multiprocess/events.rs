use crossterm::event::KeyEvent;

use crate::process_manager::{ProcessEventKind, ProcessSupervisor};

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
    if state.input_mode == crate::tui::core::InputMode::Insert {
        handle_insert_key(key, supervisor, state)?;
        return Ok(LoopControl::Continue);
    }
    Ok(handle_command_key(key, state, max_offset))
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::tui::core::{next_index, prev_index, toggle_follow_for_active};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::input::shell_key_input;

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
