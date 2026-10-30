use super::SessionState;

#[test]
fn new_initializes_process_scoped_maps_and_defaults() {
    let state = SessionState::new(
        ".".into(),
        vec!["api".to_owned(), "shell".to_owned()],
        2000,
        240,
        8000,
    );

    assert_eq!(state.active_process(), "api");
    assert_eq!(state.logs.len(), 2);
    assert_eq!(state.scroll_offset_for("api"), 0);
    assert!(state.follow_for("api"));
    assert!(!state.output_seen_for("api"));
    assert!(!state.restart_pending_for("api"));
    assert_eq!(state.restart_count_for("api"), 0);
    assert!(!state.vt_saw_chunk_for("api"));
    assert!(state.process_started_at_for("api").is_some());
    assert!(state.vt_parser_for("api").is_some());
}

#[test]
fn accessor_fallbacks_remain_safe_for_unknown_processes() {
    let mut state = SessionState::new(".".into(), vec!["api".to_owned()], 2000, 240, 8000);

    assert_eq!(state.scroll_offset_for("missing"), 0);
    assert!(state.follow_for("missing"));
    assert!(!state.output_seen_for("missing"));
    assert!(!state.restart_pending_for("missing"));
    assert_eq!(state.restart_count_for("missing"), 0);
    assert!(!state.vt_saw_chunk_for("missing"));
    assert!(state.logs_for("missing").is_none());
    assert!(state.vt_parser_for("missing").is_none());
    assert!(state.vt_parser_mut_for("missing").is_none());

    assert!(!state.mark_process_received_output("missing"));
    assert!(!state.restart_pending_for("missing"));
}

/// The session's entry waits are a snapshot, never a growing map: a wait that
/// settled (a listener became ready, the run's own probe answered) leaves its
/// tab, and a new wait replaces the one before it (effigy#458).
#[test]
fn entry_waits_are_replaced_by_each_snapshot() {
    use crate::waits::{EntryPhase, EntryState, WaitOn};

    let mut state = SessionState::new(
        ".".into(),
        vec!["api".to_owned(), "front".to_owned()],
        2000,
        240,
        8000,
    );
    let probe = EntryState {
        entry: "api".to_owned(),
        phase: EntryPhase::Starting,
        waiting_on: Some(WaitOn::Probe {
            probe: "GET / -> 200".to_owned(),
        }),
        not_started_because: None,
        detail: None,
        elapsed_ms: 4_000,
    };
    let waiting = EntryState {
        entry: "front".to_owned(),
        phase: EntryPhase::Waiting,
        waiting_on: Some(WaitOn::Entry {
            name: "api".to_owned(),
            phase: EntryPhase::Starting,
        }),
        not_started_because: None,
        detail: None,
        elapsed_ms: 0,
    };
    state.apply_entry_waits(vec![probe.clone(), waiting.clone()]);
    assert_eq!(state.entry_waits.len(), 2);
    assert_eq!(state.entry_waits["api"].elapsed_ms, 4_000);

    // api became ready: its probe is absent from the latest snapshot.
    state.apply_entry_waits(vec![waiting.clone()]);
    assert!(
        !state.entry_waits.contains_key("api"),
        "a settled wait leaves its tab"
    );
    assert!(state.entry_waits.contains_key("front"));

    // front started too: nothing waits any more.
    state.apply_entry_waits(Vec::new());
    assert!(state.entry_waits.is_empty());
}
