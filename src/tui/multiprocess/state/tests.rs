use super::SessionState;

#[test]
fn new_initializes_process_scoped_maps_and_defaults() {
    let state = SessionState::new(vec!["api".to_owned(), "shell".to_owned()], 2000, 240, 8000);

    assert_eq!(state.active_process(), "api");
    assert_eq!(state.logs.len(), 2);
    assert_eq!(state.scroll_offset_for("api"), 0);
    assert!(state.follow_for("api"));
    assert!(!state.output_seen_for("api"));
    assert!(!state.restart_pending_for("api"));
    assert_eq!(state.restart_count_for("api"), 0);
    assert!(!state.vt_saw_chunk_for("api"));
    assert!(state.process_started_at_for("api").is_some());
    assert!(state.vt_parser_for("api").is_some());
}

#[test]
fn accessor_fallbacks_remain_safe_for_unknown_processes() {
    let mut state = SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);

    assert_eq!(state.scroll_offset_for("missing"), 0);
    assert!(state.follow_for("missing"));
    assert!(!state.output_seen_for("missing"));
    assert!(!state.restart_pending_for("missing"));
    assert_eq!(state.restart_count_for("missing"), 0);
    assert!(!state.vt_saw_chunk_for("missing"));
    assert!(state.logs_for("missing").is_none());
    assert!(state.vt_parser_for("missing").is_none());
    assert!(state.vt_parser_mut_for("missing").is_none());

    assert!(!state.mark_process_received_output("missing"));
    assert!(!state.restart_pending_for("missing"));
}
