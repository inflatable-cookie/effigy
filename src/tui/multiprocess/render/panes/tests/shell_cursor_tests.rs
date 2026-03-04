use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::tui::core::ProcessExitState;

use super::super::shell_cursor::{
    shell_cursor_position, should_show_output_scrollbar, should_show_shell_caret,
};

#[test]
fn shell_caret_visibility_toggles_in_ten_tick_windows() {
    assert!(should_show_shell_caret(0));
    assert!(should_show_shell_caret(9));
    assert!(!should_show_shell_caret(10));
    assert!(!should_show_shell_caret(19));
    assert!(should_show_shell_caret(20));
}

#[test]
fn shell_cursor_position_clamps_to_inner_output_area() {
    let area = Rect::new(10, 20, 8, 6);
    assert_eq!(shell_cursor_position(area, Some((1, 2))), Some((13, 22)));
    assert_eq!(shell_cursor_position(area, Some((99, 99))), Some((16, 24)));
}

#[test]
fn shell_cursor_position_returns_none_for_missing_cursor_or_tiny_area() {
    let area = Rect::new(0, 0, 10, 5);
    assert_eq!(shell_cursor_position(area, None), None);

    let too_narrow = Rect::new(0, 0, 2, 5);
    assert_eq!(shell_cursor_position(too_narrow, Some((0, 0))), None);

    let too_short = Rect::new(0, 0, 5, 2);
    assert_eq!(shell_cursor_position(too_short, Some((0, 0))), None);
}

#[test]
fn output_scrollbar_visibility_depends_on_seen_output_or_exit_state() {
    let mut exits = HashMap::new();
    assert!(!should_show_output_scrollbar(false, &exits, "api"));
    assert!(should_show_output_scrollbar(true, &exits, "api"));

    exits.insert("api".to_owned(), ProcessExitState::Failure);
    assert!(should_show_output_scrollbar(false, &exits, "api"));
}
