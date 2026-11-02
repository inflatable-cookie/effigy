use crate::core::{LogEntryKind, ProcessExitState};
use effigy_process::ProcessEventKind;

use super::super::{all_processes_exited, handle_exit_event};
use super::{diagnostics, process_event, state_with_process};

#[test]
fn all_processes_exited_requires_full_count() {
    let mut exits = std::collections::HashMap::new();
    exits.insert("a".to_owned(), ProcessExitState::Success);
    assert!(!all_processes_exited(&exits, 2));
    exits.insert("b".to_owned(), ProcessExitState::Failure);
    assert!(all_processes_exited(&exits, 2));
}

#[test]
fn exit_event_ignores_every_exit_while_restart_pending() {
    let mut state = state_with_process("api");
    state.restart_pending.insert("api".to_owned(), true);
    let mut diagnostics = diagnostics();

    for payload in ["exit=0", "signal=15"] {
        let event = process_event("api", ProcessEventKind::Exit, payload, None);
        handle_exit_event(&event, &mut state, &mut diagnostics);
    }

    assert!(state.restart_pending_for("api"));
    assert!(!state.exit_states.contains_key("api"));
    assert!(state.logs_for("api").expect("api logs").is_empty());
}

/// The payload a worker reported decides the recorded state: zero passes, a
/// signal the stop sends is a stop, and any nonzero exit is the suite's own
/// failure — never a cancellation (the run's own stop record decides that).
#[test]
fn exit_event_maps_the_reported_payload_to_the_recorded_state() {
    let cases = [
        ("exit=0", ProcessExitState::Success),
        ("exit=1", ProcessExitState::Failure),
        ("signal=9", ProcessExitState::Stopped),
        ("signal=15", ProcessExitState::Stopped),
    ];
    for (payload, expected) in cases {
        let mut state = state_with_process("api");
        let mut diagnostics = diagnostics();
        let event = process_event("api", ProcessEventKind::Exit, payload, None);

        handle_exit_event(&event, &mut state, &mut diagnostics);

        assert_eq!(
            state.exit_states.get("api"),
            Some(&expected),
            "payload {payload}"
        );
        assert!(!state.restart_pending_for("api"), "{payload}");
    }
}

/// The rendered exit line keeps the report's printable text: control bytes
/// (here a NUL) are stripped before the line reaches the pane.
#[test]
fn exit_event_sanitizes_the_rendered_failure_log_line() {
    let mut state = state_with_process("api");
    let mut diagnostics = diagnostics();
    let event = process_event("api", ProcessEventKind::Exit, "exit=1\u{0000}", None);

    handle_exit_event(&event, &mut state, &mut diagnostics);

    assert_eq!(
        state.exit_states.get("api"),
        Some(&ProcessExitState::Failure)
    );
    let entry = state
        .logs_for("api")
        .and_then(|logs| logs.back())
        .expect("exit log entry");
    assert!(matches!(entry.kind, LogEntryKind::Exit));
    assert_eq!(entry.line, "exit=1");
}

#[test]
fn exit_event_requests_shutdown_for_flagged_process() {
    let mut state = state_with_process("api");
    state.shutdown_on_exit_processes.insert("api".to_owned());
    let mut diagnostics = diagnostics();
    let event = process_event("api", ProcessEventKind::Exit, "exit=0", None);

    handle_exit_event(&event, &mut state, &mut diagnostics);

    assert!(state.shutdown_requested);
}
