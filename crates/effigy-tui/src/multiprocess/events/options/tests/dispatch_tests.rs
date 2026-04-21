use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::core::{LogEntry, LogEntryKind};
use crate::multiprocess::events::options::handle_options_overlay_key;
use crate::multiprocess::events::LoopControl;

use super::{empty_supervisor, state_with_processes};

#[test]
fn enter_toggle_follow_keeps_overlay_open_and_updates_offset() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;
    state.options_index = 0;
    state.set_follow_for("api", false);
    state.set_scroll_offset_for("api", 1);

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        42,
    )
    .expect("toggle result");

    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(state.follow_for("api"));
    assert_eq!(state.scroll_offset_for("api"), 42);
    assert!(state.show_options);

    let _ = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        99,
    )
    .expect("toggle off result");
    assert!(!state.follow_for("api"));
    assert_eq!(state.scroll_offset_for("api"), 42);
    assert!(state.show_options);
}

#[test]
fn restart_hotkey_logs_failure_and_closes_overlay() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("restart result");

    let last = state
        .logs_for("api")
        .and_then(|entries| entries.back())
        .expect("restart log entry");
    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(matches!(last.kind, LogEntryKind::Stderr));
    assert!(last.line.contains("[effigy] restart failed:"));
    assert!(!state.show_options);
}

#[test]
fn export_hotkey_writes_clean_transcript_and_closes_overlay() {
    let supervisor = empty_supervisor();
    let repo_root = export_test_root("options_export_hotkey_writes_clean_transcript");
    let mut state = state_with_processes(&["api"]);
    state.repo_root = repo_root.clone();
    state.show_options = true;
    state
        .logs
        .get_mut("api")
        .expect("api logs")
        .extend([
            LogEntry {
                kind: LogEntryKind::Stderr,
                line: "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s"
                    .to_owned(),
            },
            LogEntry {
                kind: LogEntryKind::Stdout,
                line: "ready".to_owned(),
            },
        ]);

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("export result");

    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(!state.show_options);
    let export_path = repo_root.join(".effigy/exports/managed-tui/api.log");
    let exported = fs::read_to_string(&export_path).expect("read export");
    assert_eq!(
        exported,
        "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s\nready"
    );
    assert_eq!(
        state.footer_message.as_deref(),
        Some("exported clean transcript to .effigy/exports/managed-tui/api.log")
    );
}

#[test]
fn stop_hotkey_logs_failure_and_closes_overlay() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("stop result");

    let last = state
        .logs_for("api")
        .and_then(|entries| entries.back())
        .expect("stop log entry");
    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(matches!(last.kind, LogEntryKind::Stderr));
    assert!(last.line.contains("[effigy] stop failed:"));
    assert!(!state.show_options);
}

#[test]
fn enter_cancel_closes_overlay() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;
    state.options_index = 4;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("cancel result");

    assert!(matches!(control, Some(LoopControl::Continue)));
    assert!(!state.show_options);
}

#[test]
fn quit_hotkey_requests_quit() {
    let supervisor = empty_supervisor();
    let mut state = state_with_processes(&["api"]);
    state.show_options = true;

    let control = handle_options_overlay_key(
        &KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        &supervisor,
        &mut state,
        0,
    )
    .expect("quit result");

    assert!(matches!(control, Some(LoopControl::Quit)));
}

fn export_test_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-tui-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp root");
    root
}
