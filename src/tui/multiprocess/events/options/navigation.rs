use super::super::super::state::SessionState;

pub(super) fn close_options_overlay(state: &mut SessionState) {
    state.show_options = false;
}

pub(super) fn move_options_up(state: &mut SessionState) {
    state.options_index = state.options_index.saturating_sub(1);
}

pub(super) fn move_options_down(state: &mut SessionState, actions_len: usize) {
    let max = actions_len.saturating_sub(1);
    state.options_index = (state.options_index + 1).min(max);
}
