use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::multiprocess::events::options::handle_options_overlay_key;
use crate::tui::multiprocess::events::LoopControl;

use super::{empty_supervisor, state_with_processes};

#[test]
fn hidden_overlay_does_not_consume_keys() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = false;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("options key result");

    assert!(control.is_none());
}

#[test]
fn esc_closes_overlay() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("options key result");

    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(!state.show_options);
}

#[test]
fn up_and_down_keys_clamp_selection() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let _ = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("up result");
    assert_eq!(state.options_index, 0);

    for _ in 0..10 {
        let _ = handle_options_overlay_key(
            &KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &supervisor,
            &mut state,
            0,
        )
        .expect("down result");
    }
    assert_eq!(state.options_index, 4);
}
