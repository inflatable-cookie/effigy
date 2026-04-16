pub(crate) use effigy_tui::core::*;

#[cfg(test)]
pub(crate) fn toggle_follow_for_active(
    follow_mode: &mut std::collections::HashMap<String, bool>,
    scroll_offsets: &mut std::collections::HashMap<String, usize>,
    active: &str,
    max_offset: usize,
) {
    if let Some(follow) = follow_mode.get_mut(active) {
        *follow = !*follow;
        if *follow {
            if let Some(offset) = scroll_offsets.get_mut(active) {
                *offset = max_offset;
            }
        }
    }
}
