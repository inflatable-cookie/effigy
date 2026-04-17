use crossterm::event::{KeyCode, KeyEvent};

use effigy_process::ProcessSupervisor;

use super::super::state::SessionState;
use super::super::MultiProcessTuiError;

pub(super) fn handle_insert_key(
    key: &KeyEvent,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
) -> Result<(), MultiProcessTuiError> {
    match key.code {
        KeyCode::Enter => {
            if !state.input_line.is_empty() {
                let target = &state.process_names[state.active_index];
                let mut payload = state.input_line.clone();
                payload.push('\n');
                supervisor.send_input(target, &payload)?;
                state.input_line.clear();
            }
        }
        KeyCode::Backspace => {
            state.input_line.pop();
        }
        KeyCode::Esc => {
            state.input_mode = crate::core::InputMode::Command;
        }
        KeyCode::Char(c) => {
            state.input_line.push(c);
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "key_insert/tests.rs"]
mod tests;
