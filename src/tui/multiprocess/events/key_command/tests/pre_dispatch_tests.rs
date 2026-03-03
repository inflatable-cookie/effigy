use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::core::ProcessExitState;
use crate::tui::multiprocess::events::{LoopControl, MultiProcessTuiOptions};
use crate::tui::multiprocess::state::SessionState;

use super::super::handle_pre_dispatch_key;

#[test]
fn pre_dispatch_esc_quits_only_when_all_processes_exited() {
    let mut state = SessionState::new(vec!["api".to_owned(), "db".to_owned()], 2000, 240, 8000);
    state
        .exit_states
        .insert("api".to_owned(), ProcessExitState::Success);
    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

    let control = handle_pre_dispatch_key(
        &key,
        &mut state,
        MultiProcessTuiOptions {
            esc_quit_on_complete: true,
        },
    );
    assert!(control.is_none());

    state
        .exit_states
        .insert("db".to_owned(), ProcessExitState::Failure);
    let control = handle_pre_dispatch_key(
        &key,
        &mut state,
        MultiProcessTuiOptions {
            esc_quit_on_complete: true,
        },
    );
    assert!(matches!(control, Some(LoopControl::Quit)));
}
