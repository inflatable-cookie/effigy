use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::core::InputMode;

use super::super::process::all_processes_exited;
use super::{LoopControl, MultiProcessTuiOptions, SessionState};

pub(super) fn handle_pre_dispatch_key(
    key: &KeyEvent,
    state: &mut SessionState,
    options: MultiProcessTuiOptions,
) -> Option<LoopControl> {
    if matches!(key.code, KeyCode::Esc)
        && options.esc_quit_on_complete
        && !state.show_help
        && !state.show_options
        && state.input_mode == InputMode::Command
        && all_processes_exited(&state.exit_states, state.process_names.len())
    {
        return Some(LoopControl::Quit);
    }
    if matches!(key.code, KeyCode::Tab) {
        if state.active_process() == "shell" {
            if !state.shell_capture_mode {
                state.shell_capture_mode = true;
            }
            return Some(LoopControl::Continue);
        }
        state.input_mode = if state.input_mode == InputMode::Insert {
            InputMode::Command
        } else {
            InputMode::Insert
        };
        if state.input_mode == InputMode::Insert {
            state.show_help = false;
            state.show_options = false;
        }
        return Some(LoopControl::Continue);
    }
    None
}
