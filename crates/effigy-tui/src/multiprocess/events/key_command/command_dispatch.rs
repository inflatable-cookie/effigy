use crossterm::event::{KeyCode, KeyEvent};

use crate::core::{next_index, prev_index, InputMode};

use super::{LoopControl, SessionState};

pub(super) fn handle_command_key(
    key: &KeyEvent,
    state: &mut SessionState,
    max_offset: usize,
) -> LoopControl {
    match key.code {
        KeyCode::Char('i') if state.process_names[state.active_index] != "shell" => {
            state.input_mode = InputMode::Insert;
            state.show_help = false;
            state.show_options = false;
        }
        KeyCode::Char('i') => {}
        KeyCode::Char('h') => {
            state.show_help = !state.show_help;
            if state.show_help {
                state.show_options = false;
            }
        }
        KeyCode::Char('o') => {
            state.show_options = !state.show_options;
            if state.show_options {
                state.show_help = false;
                state.options_index = 0;
            }
        }
        KeyCode::BackTab => {
            state.shell_capture_mode = false;
            state.input_mode = InputMode::Command;
            state.active_index = prev_index(state.active_index, state.process_names.len());
        }
        KeyCode::Right => {
            state.shell_capture_mode = false;
            state.input_mode = InputMode::Command;
            state.active_index = next_index(state.active_index, state.process_names.len());
        }
        KeyCode::Left => {
            state.shell_capture_mode = false;
            state.input_mode = InputMode::Command;
            state.active_index = prev_index(state.active_index, state.process_names.len());
        }
        KeyCode::Up => {
            let active = state.active_process().to_owned();
            state.set_follow_for(&active, false);
            state
                .set_scroll_offset_for(&active, state.scroll_offset_for(&active).saturating_sub(1));
        }
        KeyCode::Down => {
            let active = state.active_process().to_owned();
            state.set_scroll_offset_for(
                &active,
                state
                    .scroll_offset_for(&active)
                    .saturating_add(1)
                    .min(max_offset),
            );
        }
        KeyCode::PageUp => {
            let active = state.active_process().to_owned();
            state.set_follow_for(&active, false);
            state.set_scroll_offset_for(
                &active,
                state.scroll_offset_for(&active).saturating_sub(10),
            );
        }
        KeyCode::PageDown => {
            let active = state.active_process().to_owned();
            state.set_scroll_offset_for(
                &active,
                state
                    .scroll_offset_for(&active)
                    .saturating_add(10)
                    .min(max_offset),
            );
        }
        KeyCode::Home => {
            let active = state.active_process().to_owned();
            state.set_follow_for(&active, false);
            state.set_scroll_offset_for(&active, 0);
        }
        KeyCode::End => {
            let active = state.active_process().to_owned();
            state.set_follow_for(&active, true);
            state.set_scroll_offset_for(&active, max_offset);
        }
        KeyCode::Esc => {
            state.show_help = false;
            state.show_options = false;
        }
        _ => {}
    }

    LoopControl::Continue
}
