use crate::multiprocess::view_model::build_active_view_model;

#[test]
fn shell_cursor_is_reported_only_for_shell_when_vt_enabled() {
    let mut shell_state = crate::multiprocess::state::SessionState::new(
        ".".into(),
        vec!["shell".to_owned()],
        2000,
        240,
        8000,
    );
    let shell_view = build_active_view_model(&mut shell_state, 5, 80, true);
    assert!(shell_view.shell_cursor.is_some());

    let mut api_state = crate::multiprocess::state::SessionState::new(
        ".".into(),
        vec!["api".to_owned()],
        2000,
        240,
        8000,
    );
    let api_view = build_active_view_model(&mut api_state, 5, 80, true);
    assert!(api_view.shell_cursor.is_none());

    let no_vt_view = build_active_view_model(&mut shell_state, 5, 80, false);
    assert!(no_vt_view.shell_cursor.is_none());
}
