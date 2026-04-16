use crate::multiprocess::view_model::build_active_view_model;

use super::state_with_logs;

#[test]
fn non_vt_view_clamps_scroll_offset() {
    let mut state = state_with_logs(20);
    state.scroll_offsets.insert("api".to_owned(), 999);
    state.follow_mode.insert("api".to_owned(), false);

    let view = build_active_view_model(&mut state, 5, 80, false);
    assert_eq!(view.max_offset, 15);
    assert_eq!(view.scroll_offset, 15);
    assert_eq!(view.render_scroll_offset, 15);
    assert!(!view.is_follow);
}

#[test]
fn non_vt_follow_mode_tracks_end() {
    let mut state = state_with_logs(12);
    state.scroll_offsets.insert("api".to_owned(), 0);
    state.follow_mode.insert("api".to_owned(), true);

    let view = build_active_view_model(&mut state, 5, 80, false);
    assert_eq!(view.max_offset, 7);
    assert_eq!(view.render_scroll_offset, 7);
    assert_eq!(view.scroll_offset, 0);
    assert!(view.is_follow);
}
