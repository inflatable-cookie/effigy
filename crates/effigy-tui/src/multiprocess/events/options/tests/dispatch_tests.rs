use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::core::LogEntryKind;

use crate::multiprocess::events::options::handle_options_overlay_key;
use crate::multiprocess::events::LoopControl;

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

/// The restart and stop hotkeys each log their own failure (the stub
/// supervisor refuses the request) and close the overlay.
#[test]
fn restart_and_stop_hotkeys_log_their_failure_and_close_the_overlay() {
    for (key, expected_prefix) in [
        (KeyCode::Char('r'), "[effigy] restart failed:"),
        (KeyCode::Char('s'), "[effigy] stop failed:"),
    ] {
        let supervisor = empty_supervisor();
        let mut state = state_with_processes(&["api"]);
        state.show_options = true;

        let control = handle_options_overlay_key(
            &KeyEvent::new(key, KeyModifiers::NONE),
            &supervisor,
            &mut state,
            0,
        )
        .expect("hotkey result");

        let last = state
            .logs_for("api")
            .and_then(|entries| entries.back())
            .expect("hotkey log entry");
        assert!(matches!(control, Some(LoopControl::Continue)), "{key:?}");
        assert!(matches!(last.kind, LogEntryKind::Stderr), "{key:?}");
        assert!(last.line.contains(expected_prefix), "{}", last.line);
        assert!(!state.show_options, "{key:?}");
    }
}

#[test]
fn escape_and_enter_cancel_close_the_options_overlay() {
    for (key, selected_index) in [(KeyCode::Esc, 0), (KeyCode::Enter, 3)] {
        let supervisor = empty_supervisor();
        let mut state = state_with_processes(&["api"]);
        state.show_options = true;
        state.options_index = selected_index;

        let control = handle_options_overlay_key(
            &KeyEvent::new(key, KeyModifiers::NONE),
            &supervisor,
            &mut state,
            0,
        )
        .expect("cancel result");

        assert!(matches!(control, Some(LoopControl::Continue)), "{key:?}");
        assert!(!state.show_options, "{key:?}");
        assert_eq!(
            state.options_index, selected_index,
            "cancel must not move selection"
        );
    }
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
