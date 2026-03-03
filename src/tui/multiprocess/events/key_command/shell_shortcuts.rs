use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::process_manager::ProcessSupervisor;
use crate::tui::core::InputMode;

use super::super::input::shell_key_input;
use super::{LoopControl, MultiProcessTuiError, SessionState};

pub(super) fn handle_shell_shortcuts(
    key: &KeyEvent,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    active_process: &str,
    active_is_shell: bool,
) -> Result<Option<LoopControl>, MultiProcessTuiError> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        if active_is_shell && state.shell_capture_mode && !state.show_help && !state.show_options {
            supervisor.send_input(active_process, "\u{3}")?;
            return Ok(Some(LoopControl::Continue));
        }
        return Ok(Some(LoopControl::Quit));
    }
    if active_is_shell
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('g'))
    {
        state.shell_capture_mode = !state.shell_capture_mode;
        state.input_mode = InputMode::Command;
        return Ok(Some(LoopControl::Continue));
    }
    if active_is_shell && state.shell_capture_mode && !state.show_help && !state.show_options {
        if let Some(input) = shell_key_input(key) {
            supervisor.send_input(active_process, &input)?;
        }
        return Ok(Some(LoopControl::Continue));
    }
    Ok(None)
}
