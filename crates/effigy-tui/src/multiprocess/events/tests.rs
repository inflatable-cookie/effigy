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
    let mut follow = HashMap::from([("api".to_owned(), false), ("admin".to_owned(), true)]);
    let mut offsets = HashMap::from([("api".to_owned(), 1usize), ("admin".to_owned(), 9usize)]);
    toggle_follow_for_active(&mut follow, &mut offsets, "api", 42);
    assert_eq!(follow.get("api"), Some(&true));
    assert_eq!(offsets.get("api"), Some(&42usize));

    toggle_follow_for_active(&mut follow, &mut offsets, "api", 99);
    assert_eq!(follow.get("api"), Some(&false));
    assert_eq!(offsets.get("api"), Some(&42usize));
    assert_eq!(follow.get("admin"), Some(&true));
    assert_eq!(offsets.get("admin"), Some(&9usize));
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

#[test]
fn fake_hybrid_progress_reaches_the_services_frame_during_startup() {
    use super::process::handle_stderr_event;
    use crate::multiprocess::diagnostics::RuntimeDiagnostics;
    use crate::multiprocess::render::{render_snapshot_frame, FrameSnapshot};
    use crate::multiprocess::state::SessionState;
    use effigy_process::{ProcessEvent, ProcessEventKind};

    let names = ["front", "admin", "api", "jobs", "services"].map(str::to_owned);
    let mut state = SessionState::new(".".into(), names.to_vec(), 40, 140, 500);
    let mut diagnostics = RuntimeDiagnostics::from_env();
    let mut emit = |line: &str| {
        let event = ProcessEvent {
            process: "services".into(),
            kind: ProcessEventKind::Stderr,
            payload: line.into(),
            chunk: None,
        };
        handle_stderr_event(&event, &mut state, &mut diagnostics, false);
    };
    emit("Started: running dev:hybrid step 1/2: container hybrid up --detach; waiting for step `1` to exit; 0s elapsed");
    let before_logs: Vec<_> = state
        .logs_for("services")
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let before = render_snapshot_frame(FrameSnapshot {
        process_names: &names,
        active: "services",
        entry_waits: &[],
        logs: &before_logs,
        width: 140,
        height: 16,
    })
    .unwrap();
    for line in [
        "Started: resolving container policy",
        "✓ container config resolved",
        "Started: applying compose configuration",
        "✓ containers healthy",
        "[host process] listener `api` is still starting after 30s; last child output: compiling api",
        "[host process] listener `api` ready; route published",
    ] {
        let event = ProcessEvent { process: "services".into(), kind: ProcessEventKind::Stderr, payload: line.into(), chunk: None };
        handle_stderr_event(&event, &mut state, &mut diagnostics, false);
    }
    let after_logs: Vec<_> = state
        .logs_for("services")
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let after = render_snapshot_frame(FrameSnapshot {
        process_names: &names,
        active: "services",
        entry_waits: &[],
        logs: &after_logs,
        width: 140,
        height: 16,
    })
    .unwrap();
    assert!(after.contains("container config resolved"), "{after}");
    assert!(after.contains("applying compose"), "{after}");
    assert!(
        after.contains("listener `api` is still starting after 30s"),
        "{after}"
    );
    assert!(
        after.contains("last child output: compiling api"),
        "{after}"
    );
    assert!(after.contains("route published"), "{after}");
    println!("BEFORE SERVICES FRAME\n{before}\nAFTER SERVICES FRAME\n{after}");
}
