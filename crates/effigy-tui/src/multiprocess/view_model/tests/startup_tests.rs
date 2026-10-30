use std::time::{Duration, Instant};

use crate::multiprocess::state::{ProcessStartupState, SessionState};
use crate::multiprocess::view_model::build_active_view_model;

#[test]
fn a_started_entry_ages_from_its_own_start_not_from_the_session() {
    let mut state = SessionState::new(
        ".".into(),
        vec!["api".to_owned(), "web".to_owned()],
        2000,
        240,
        8000,
    );
    // web queued for a minute behind api before its own start.
    state
        .process_started_at
        .insert("web".to_owned(), Instant::now() - Duration::from_secs(60));
    state.active_index = 1;
    let queued = build_active_view_model(&mut state, 20, 80, false);
    assert!(queued.active_elapsed >= Duration::from_secs(60));

    // The real production transition: the supervisor reports Starting.
    state.record_startup_state("web", ProcessStartupState::Starting);
    let starting = build_active_view_model(&mut state, 20, 80, false);
    assert!(
        starting.active_elapsed < Duration::from_secs(1),
        "the pane draws the start attempt's own clock: {:?}",
        starting.active_elapsed
    );
}

#[test]
fn a_started_entry_then_ages_from_the_run_it_became() {
    let mut state = SessionState::new(".".into(), vec!["api".to_owned()], 2000, 240, 8000);
    state
        .process_started_at
        .insert("api".to_owned(), Instant::now() - Duration::from_secs(60));
    state.record_startup_state("api", ProcessStartupState::Running);
    let running = build_active_view_model(&mut state, 20, 80, false);
    assert!(
        running.active_elapsed < Duration::from_secs(1),
        "a running entry ages from the run that started: {:?}",
        running.active_elapsed
    );
}
