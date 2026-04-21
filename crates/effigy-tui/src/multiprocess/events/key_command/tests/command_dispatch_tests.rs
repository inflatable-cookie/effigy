use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::core::InputMode;
use crate::multiprocess::events::LoopControl;
use crate::multiprocess::state::SessionState;

use super::super::handle_command_key;

#[test]
fn command_key_i_enters_insert_mode_for_non_shell_tabs() {
    let mut state = SessionState::new(
        ".".into(),
        vec!["api".to_owned(), "shell".to_owned()],
        2000,
        240,
        8000,
    );
    state.active_index = 0;
    state.input_mode = InputMode::Command;
    state.show_help = true;
    state.show_options = true;

    let control = handle_command_key(
        &KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        &mut state,
        10,
    )
    .expect("handle command key");

    assert!(matches!(control, LoopControl::Continue));
    assert_eq!(state.input_mode, InputMode::Insert);
    assert!(!state.show_help);
    assert!(!state.show_options);
}

#[test]
fn command_key_end_enables_follow_and_jumps_to_max_offset() {
    let mut state = SessionState::new(".".into(), vec!["api".to_owned()], 2000, 240, 8000);
    state.set_follow_for("api", false);
    state.set_scroll_offset_for("api", 2);

    let control = handle_command_key(
        &KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
        &mut state,
        42,
    )
    .expect("handle command key");

    assert!(matches!(control, LoopControl::Continue));
    assert!(state.follow_for("api"));
    assert_eq!(state.scroll_offset_for("api"), 42);
}
