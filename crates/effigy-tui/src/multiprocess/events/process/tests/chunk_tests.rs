use effigy_process::ProcessEventKind;

use super::super::handle_chunk_event;
use super::{diagnostics, process_event, state_with_process, state_with_vt_process};

#[test]
fn chunk_event_marks_output_and_sets_vt_chunk_seen_for_vt_process() {
    let mut state = state_with_vt_process("api");
    let mut diagnostics = diagnostics();
    let event = process_event(
        "api",
        ProcessEventKind::StdoutChunk,
        "",
        Some(b"hello\r\n".to_vec()),
    );

    handle_chunk_event(&event, &mut state, &mut diagnostics, true);

    assert!(state.output_seen_for("api"));
    assert!(state.vt_saw_chunk_for("api"));
}

#[test]
fn chunk_event_for_plain_process_only_marks_output_seen() {
    let mut state = state_with_process("api");
    let mut diagnostics = diagnostics();
    let event = process_event(
        "api",
        ProcessEventKind::StderrChunk,
        "",
        Some(b"err\r\n".to_vec()),
    );

    handle_chunk_event(&event, &mut state, &mut diagnostics, true);

    assert!(state.output_seen_for("api"));
    assert!(!state.vt_saw_chunk_for("api"));
}
