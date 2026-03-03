use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::core::InputMode;
use crate::tui::multiprocess::events::LoopControl;
use crate::tui::multiprocess::state::SessionState;

use super::super::handle_shell_shortcuts;
use super::empty_supervisor;

#[test]
fn shell_shortcut_control_g_toggles_shell_capture_mode() {
    let supervisor = empty_supervisor();
    let mut state = SessionState::new(vec!["shell".to_owned()], 2000, 240, 8000);
    state.shell_capture_mode = false;
    state.input_mode = InputMode::Insert;

    let key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
    let control = handle_shell_shortcuts(&key, &supervisor, &mut state, "shell", true)
        .expect("shortcut handling");

    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(state.shell_capture_mode);
    assert_eq!(state.input_mode, InputMode::Command);
}
