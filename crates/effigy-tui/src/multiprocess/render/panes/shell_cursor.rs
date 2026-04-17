use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::core::ProcessExitState;

pub(super) fn should_show_shell_caret(spinner_tick: usize) -> bool {
    (spinner_tick / 10).is_multiple_of(2)
}

pub(super) fn should_show_output_scrollbar(
    active_output_seen: bool,
    exit_states: &HashMap<String, ProcessExitState>,
    process_name: &str,
) -> bool {
    active_output_seen || exit_states.contains_key(process_name)
}

pub(super) fn shell_cursor_position(
    area: Rect,
    shell_cursor: Option<(u16, u16)>,
) -> Option<(u16, u16)> {
    let (row, col) = shell_cursor?;
    let inner_x = area.x.saturating_add(1);
    let inner_y = area.y.saturating_add(1);
    let inner_w = area.width.saturating_sub(2);
    let inner_h = area.height.saturating_sub(2);
    if inner_w == 0 || inner_h == 0 {
        return None;
    }
    let cursor_x = inner_x.saturating_add(col.min(inner_w.saturating_sub(1)));
    let cursor_y = inner_y.saturating_add(row.min(inner_h.saturating_sub(1)));
    Some((cursor_x, cursor_y))
}
