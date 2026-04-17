use crossterm::event::{KeyCode, KeyEvent};

use effigy_process::ProcessSupervisor;

use super::super::render::options_actions;
use super::super::state::{OptionsAction, SessionState};
use super::super::MultiProcessTuiError;
use super::LoopControl;

mod actions;
mod navigation;
#[cfg(test)]
mod tests;

use actions::apply_options_action;
use navigation::{close_options_overlay, move_options_down, move_options_up};

pub(super) fn handle_options_overlay_key(
    key: &KeyEvent,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    max_offset: usize,
) -> Result<Option<LoopControl>, MultiProcessTuiError> {
    if !state.show_options {
        return Ok(None);
    }

    let follow_active = state.follow_for(state.active_process());
    let actions = options_actions(follow_active);

    match key.code {
        KeyCode::Esc | KeyCode::Char('o') => close_options_overlay(state),
        KeyCode::Up => move_options_up(state),
        KeyCode::Down => move_options_down(state, actions.len()),
        KeyCode::Enter => {
            let action = actions
                .get(state.options_index)
                .copied()
                .unwrap_or(OptionsAction::Cancel);
            return dispatch_action(action, supervisor, state, max_offset);
        }
        code => {
            if let Some(action) = action_from_hotkey(code) {
                return dispatch_action(action, supervisor, state, max_offset);
            }
        }
    }

    Ok(Some(LoopControl::Continue))
}

fn dispatch_action(
    action: OptionsAction,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    max_offset: usize,
) -> Result<Option<LoopControl>, MultiProcessTuiError> {
    let active = state.active_process().to_owned();
    if apply_options_action(action, &active, supervisor, state, max_offset)? {
        return Ok(Some(LoopControl::Quit));
    }
    if should_close_overlay(action) {
        close_options_overlay(state);
    }
    Ok(Some(LoopControl::Continue))
}

fn action_from_hotkey(code: KeyCode) -> Option<OptionsAction> {
    match code {
        KeyCode::Char('f') => Some(OptionsAction::ToggleFollow),
        KeyCode::Char('r') => Some(OptionsAction::Restart),
        KeyCode::Char('s') => Some(OptionsAction::Stop),
        KeyCode::Char('q') => Some(OptionsAction::Quit),
        _ => None,
    }
}

fn should_close_overlay(action: OptionsAction) -> bool {
    !matches!(action, OptionsAction::ToggleFollow)
}
