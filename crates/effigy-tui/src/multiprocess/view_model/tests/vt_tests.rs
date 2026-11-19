use std::time::Duration;

use crate::multiprocess::view_model::build_active_view_model;

#[test]
fn vt_mode_clamps_scroll_offset_safely() {
    let mut state =
        crate::multiprocess::state::SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);
    let parser = state.vt_parsers.get_mut("api").expect("api vt parser");
    parser.process(b"test-one\r\n");
    parser.process(b"test-two\r\n");
    state.vt_saw_chunk.insert("api".to_owned(), true);
    state.scroll_offsets.insert("api".to_owned(), usize::MAX);
    state.follow_mode.insert("api".to_owned(), false);

    let view = build_active_view_model(&mut state, 3, 80, true);
    assert!(view.scroll_offset <= view.max_offset);
    assert!(view.scrollbar_total >= view.max_offset);
    assert!(view.active_elapsed >= Duration::from_millis(0));
}
