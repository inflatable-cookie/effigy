use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::process_manager::ProcessSupervisor;
use crate::tui::core::{next_index, prev_index, InputMode};

use super::super::state::SessionState;
use super::super::{MultiProcessTuiError, MultiProcessTuiOptions};
use super::input::shell_key_input;
use super::process::all_processes_exited;
use super::LoopControl;

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

pub(super) fn handle_command_key(
    key: &KeyEvent,
    state: &mut SessionState,
    max_offset: usize,
) -> LoopControl {
    match key.code {
        KeyCode::Char('i') => {
            if state.process_names[state.active_index] != "shell" {
                state.input_mode = InputMode::Insert;
                state.show_help = false;
                state.show_options = false;
            }
        }
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
