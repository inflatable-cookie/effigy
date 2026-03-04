use crate::process_manager::ProcessEventKind;

use super::super::{handle_stderr_event, handle_stdout_event};
use super::{diagnostics, process_event, state_with_process};

#[test]
fn stdout_event_skips_plain_output_when_vt_chunk_already_seen() {
    let mut state = state_with_process("api");
    state.set_vt_saw_chunk_for("api", true);
    let mut diagnostics = diagnostics();
    let event = process_event("api", ProcessEventKind::Stdout, "line one", None);

    let skipped = handle_stdout_event(&event, &mut state, &mut diagnostics, true);

    assert!(skipped);
    let lines = state.logs_for("api").expect("api logs");
    assert!(lines.is_empty());
}

#[test]
fn stdout_and_stderr_events_ingest_payload_lines() {
    let mut state = state_with_process("api");
    let mut diagnostics = diagnostics();
    let stdout_event = process_event("api", ProcessEventKind::Stdout, "one\ntwo", None);
    let stderr_event = process_event("api", ProcessEventKind::Stderr, "err", None);

    let stdout_skipped = handle_stdout_event(&stdout_event, &mut state, &mut diagnostics, false);
    let stderr_skipped = handle_stderr_event(&stderr_event, &mut state, &mut diagnostics, false);

    assert!(!stdout_skipped);
    assert!(!stderr_skipped);
    assert!(state.output_seen_for("api"));
    let lines = state.logs_for("api").expect("api logs");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].line, "one\ntwo");
    assert_eq!(lines[1].line, "err");
}
