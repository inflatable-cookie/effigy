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
        KeyCode::Up => {
            let target = state.active_process().to_owned();
            state.set_follow_for(&target, false);
            state
                .set_scroll_offset_for(&target, state.scroll_offset_for(&target).saturating_sub(1));
        }
        KeyCode::Down => {
            let target = state.active_process().to_owned();
            state.set_follow_for(&target, false);
            state
                .set_scroll_offset_for(&target, state.scroll_offset_for(&target).saturating_add(1));
        }
        KeyCode::PageUp => {
            let target = state.active_process().to_owned();
            state.set_follow_for(&target, false);
            state.set_scroll_offset_for(
                &target,
                state.scroll_offset_for(&target).saturating_sub(10),
            );
        }
        KeyCode::PageDown => {
            let target = state.active_process().to_owned();
            state.set_follow_for(&target, false);
            state.set_scroll_offset_for(
                &target,
                state.scroll_offset_for(&target).saturating_add(10),
            );
        }
        KeyCode::Home => {
            let target = state.active_process().to_owned();
            state.set_follow_for(&target, false);
            state.set_scroll_offset_for(&target, 0);
        }
        KeyCode::End => {
            let target = state.active_process().to_owned();
            state.set_follow_for(&target, true);
        }
        KeyCode::Enter if !state.input_line.is_empty() => {
            let target = &state.process_names[state.active_index];
            let mut payload = state.input_line.clone();
            payload.push('\n');
            supervisor.send_input(target, &payload)?;
            state.input_line.clear();
        }
        KeyCode::Enter => {}
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
