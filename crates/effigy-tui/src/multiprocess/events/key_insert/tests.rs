use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::core::InputMode;
use effigy_process::ProcessSupervisor;

use super::{handle_insert_key, SessionState};

fn empty_supervisor() -> ProcessSupervisor {
    ProcessSupervisor::spawn(PathBuf::from("."), Vec::new()).expect("spawn empty supervisor")
}

#[test]
fn insert_mode_char_backspace_and_escape_update_input_state() {
    let supervisor = empty_supervisor();
    let mut state = SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);
    state.input_mode = InputMode::Insert;

    handle_insert_key(
        &KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
    )
    .expect("insert char");
    assert_eq!(state.input_line, "x");

    handle_insert_key(
        &KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        &supervisor,
        &mut state,
    )
    .expect("backspace");
    assert!(state.input_line.is_empty());

    state.input_mode = InputMode::Insert;
    handle_insert_key(
        &KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &supervisor,
        &mut state,
    )
    .expect("escape to command");
    assert_eq!(state.input_mode, InputMode::Command);
}

#[test]
fn insert_mode_enter_with_non_empty_line_clears_buffer_after_send() {
    let supervisor = empty_supervisor();
    let mut state = SessionState::new(vec!["api".to_owned()], 2000, 240, 8000);
    state.input_mode = InputMode::Insert;
    state.input_line = "run now".to_owned();

    handle_insert_key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &supervisor,
        &mut state,
    )
    .expect("enter send");

    assert!(state.input_line.is_empty());
}
