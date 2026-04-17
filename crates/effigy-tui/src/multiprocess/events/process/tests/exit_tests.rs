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
fn exit_event_ignores_expected_success_while_restart_pending() {
    let mut state = state_with_process("api");
    state.restart_pending.insert("api".to_owned(), true);
    let mut diagnostics = diagnostics();
    let event = process_event("api", ProcessEventKind::Exit, "exit=0", None);

    handle_exit_event(&event, &mut state, &mut diagnostics);

    assert!(state.restart_pending_for("api"));
    assert!(!state.exit_states.contains_key("api"));
    assert!(!state.observed_non_zero.contains_key("api"));
    assert!(state.logs_for("api").expect("api logs").is_empty());
}

#[test]
fn exit_event_marks_failure_and_sanitizes_rendered_log_line() {
    let mut state = state_with_process("api");
    let mut diagnostics = diagnostics();
    let event = process_event("api", ProcessEventKind::Exit, "exit=1\u{0000}", None);

    handle_exit_event(&event, &mut state, &mut diagnostics);

    assert!(!state.restart_pending_for("api"));
    assert_eq!(
        state.observed_non_zero.get("api"),
        Some(&"exit=1\u{0000}".to_owned())
    );
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
fn exit_event_marks_success_and_clears_observed_non_zero() {
    let mut state = state_with_process("api");
    state
        .observed_non_zero
        .insert("api".to_owned(), "exit=9".to_owned());
    let mut diagnostics = diagnostics();
    let event = process_event("api", ProcessEventKind::Exit, "signal=15", None);

    handle_exit_event(&event, &mut state, &mut diagnostics);

    assert_eq!(
        state.exit_states.get("api"),
        Some(&ProcessExitState::Success)
    );
    assert!(!state.observed_non_zero.contains_key("api"));
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
