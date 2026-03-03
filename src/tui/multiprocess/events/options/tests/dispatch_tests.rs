use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::core::LogEntryKind;
use crate::tui::multiprocess::events::options::handle_options_overlay_key;
use crate::tui::multiprocess::events::LoopControl;

use super::{empty_supervisor, state_with_processes};

#[test]
fn enter_toggle_follow_keeps_overlay_open_and_updates_offset() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;
    state.options_index = 0;
    state.set_follow_for("api", false);
    state.set_scroll_offset_for("api", 1);

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        42,
    )
    .expect("toggle result");

    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(state.follow_for("api"));
    assert_eq!(state.scroll_offset_for("api"), 42);
    assert!(state.show_options);

    let _ = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        99,
    )
    .expect("toggle off result");
    assert!(!state.follow_for("api"));
    assert_eq!(state.scroll_offset_for("api"), 42);
    assert!(state.show_options);
}

#[test]
fn restart_hotkey_logs_failure_and_closes_overlay() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("restart result");

    let last = state
        .logs_for("api")
        .and_then(|entries| entries.back())
        .expect("restart log entry");
    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(matches!(last.kind, LogEntryKind::Stderr));
    assert!(last.line.contains("[effigy] restart failed:"));
    assert!(!state.show_options);
}

#[test]
fn stop_hotkey_logs_failure_and_closes_overlay() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("stop result");

    let last = state
        .logs_for("api")
        .and_then(|entries| entries.back())
        .expect("stop log entry");
    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(matches!(last.kind, LogEntryKind::Stderr));
    assert!(last.line.contains("[effigy] stop failed:"));
    assert!(!state.show_options);
}

#[test]
fn enter_cancel_closes_overlay() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;
    state.options_index = 3;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("cancel result");

    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(!state.show_options);
}

#[test]
fn quit_hotkey_requests_quit() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("quit result");

    assert!(matches!(control, Some(LoopControl::Quit)));
}
