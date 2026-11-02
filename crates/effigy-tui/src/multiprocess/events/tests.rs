use std::time::{Duration, Instant};

use std::collections::HashMap;

use crate::core::{next_index, prev_index, toggle_follow_for_active};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{input::shell_key_input, should_stop_draining};

#[test]
fn tab_index_helpers_wrap_correctly() {
    assert_eq!(next_index(0, 4), 1);
    assert_eq!(next_index(3, 4), 0);
    assert_eq!(prev_index(0, 4), 3);
    assert_eq!(prev_index(2, 4), 1);
}

#[test]
fn toggle_follow_updates_mode_and_offset() {
    let mut follow = HashMap::from([("api".to_owned(), false)]);
    let mut offsets = HashMap::from([("api".to_owned(), 1usize)]);
    toggle_follow_for_active(&mut follow, &mut offsets, "api", 42);
    assert_eq!(follow.get("api"), Some(&true));
    assert_eq!(offsets.get("api"), Some(&42usize));

    toggle_follow_for_active(&mut follow, &mut offsets, "api", 99);
    assert_eq!(follow.get("api"), Some(&false));
    assert_eq!(offsets.get("api"), Some(&42usize));
}

#[test]
fn shell_key_input_maps_control_keys() {
    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(shell_key_input(&key), Some("\u{3}".to_owned()));
    let key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(shell_key_input(&key), Some("\u{1b}[D".to_owned()));
}

/// Draining gives a hung-up reader a bounded grace window, but once the
/// budget has elapsed the first seen event ends it: an idle drain keeps
/// waiting, one event stops.
#[test]
fn draining_stops_only_after_an_event_once_the_budget_elapsed() {
    let started_at = Instant::now() - Duration::from_millis(20);

    assert!(
        !should_stop_draining(started_at, 0),
        "an idle drain keeps waiting past the budget"
    );
    assert!(
        should_stop_draining(started_at, 1),
        "the first event after the budget stops the drain"
    );
}
