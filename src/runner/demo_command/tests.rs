use super::{
    append_demo_terminal_input, append_demo_terminal_resize, browser_terminal_size_override,
    load_active_attempt, render_demo_execute_text, terminated_demo_attempt, wrap_pty_shell_command,
    write_active_attempt_record, DemoActiveAttempt, DemoEntrypoint, DemoLogPaths, DemoRecord,
    DemoRuntimeBackend, PersistedDemoActiveAttempt, PersistedDemoActivePhase,
    DEMO_BROWSER_TERMINAL_COLS_ENV, DEMO_BROWSER_TERMINAL_ROWS_ENV,
};
use crate::runner::manifest::{ManifestDemoMode, ManifestManagedRun};
use effigy_demo::read_recent_output_lines;
use effigy_demo::runtime::{DemoActiveTerminalSession, DemoTerminalTransport};
use effigy_demo::PersistedDemoTerminalTransport;
use effigy_demo::{DemoAttemptHistory, DemoLatestAttempt};
use effigy_manifest::ManifestDemoStatus;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn load_active_attempt_preserves_stop_requested_record_until_owner_clears_it() {
    let repo_root = std::env::temp_dir().join(format!(
        "effigy-demo-active-stop-requested-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough for test ids")
            .as_nanos()
    ));
    fs::create_dir_all(&repo_root).expect("create temp repo root");
    let demo_id = "demo";
    write_active_attempt_record(
        &repo_root,
        demo_id,
        &PersistedDemoActiveAttempt {
            schema: "effigy.demo.active.v1".to_owned(),
            schema_version: 1,
            attempt_id: "attempt-1".to_owned(),
            demo_id: demo_id.to_owned(),
            phase: PersistedDemoActivePhase::StopRequested,
            started_at_epoch_ms: 1,
            owner_pid: u32::MAX,
            target_pid: Some(u32::MAX),
            stoppable: true,
            entrypoint_kind: "run".to_owned(),
            entrypoint_value: "sleep 1".to_owned(),
            command: "sleep 1".to_owned(),
            runtime_backend_kind: Some("run".to_owned()),
            flattened_runtime_projection: false,
            browser_live_attach_supported: true,
            projection_shape_kind: Some("single-terminal".to_owned()),
            managed_process_count: None,
            managed_process_names: Vec::new(),
            projected_output_provenance_kind: Some("none".to_owned()),
            terminal_transport: PersistedDemoTerminalTransport::Stream,
            supports_input_forwarding: false,
            supports_resize: false,
            nested_tui: false,
            terminal_cols: None,
            terminal_rows: None,
            resize_handoff_path: None,
            stdin_input_path: None,
            stdout_log_path: None,
            stderr_log_path: None,
        },
    )
    .expect("write active attempt");

    let active = load_active_attempt(&repo_root, demo_id).expect("load active attempt");
    assert!(active.active);
    assert_eq!(active.state_label(), "stop-requested");
    assert!(matches!(
        active,
        DemoActiveAttempt {
            stoppable: true,
            ..
        }
    ));
    assert!(
        repo_root.join(".effigy/demo/active/demo.json").exists(),
        "stop-requested active record should survive until the owner process clears it"
    );
    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn load_active_attempt_defaults_terminal_fields_for_legacy_records() {
    let repo_root = std::env::temp_dir().join(format!(
        "effigy-demo-active-legacy-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough for test ids")
            .as_nanos()
    ));
    fs::create_dir_all(repo_root.join(".effigy/demo/active")).expect("create active dir");
    let path = repo_root.join(".effigy/demo/active/demo.json");
    fs::write(
        &path,
        format!(
            r#"{{
  "schema": "effigy.demo.active.v1",
  "schema_version": 1,
  "attempt_id": "attempt-legacy",
  "demo_id": "demo",
  "phase": "running",
  "started_at_epoch_ms": 1,
  "owner_pid": {},
  "target_pid": null,
  "stoppable": true,
  "entrypoint_kind": "run",
  "entrypoint_value": "sleep 1",
  "command": "sleep 1",
  "stdout_log_path": ".effigy/demo/logs/demo.stdout.log",
  "stderr_log_path": ".effigy/demo/logs/demo.stderr.log"
}}"#,
            std::process::id()
        ),
    )
    .expect("write legacy active attempt");

    let active = load_active_attempt(&repo_root, "demo").expect("load active attempt");
    assert!(active.active);
    assert_eq!(active.terminal_transport, DemoTerminalTransport::Stream);
    assert_eq!(active.runtime_backend().kind, "run");
    assert_eq!(
        active.runtime_backend().projection_shape.rendered_label(),
        "single-terminal"
    );
    assert!(
        active
            .runtime_backend()
            .projection_shape
            .live_terminal_eligible
    );
    assert!(!active.supports_input_forwarding);
    assert!(!active.supports_resize);
    assert!(!active.nested_tui);
    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn read_recent_output_lines_keeps_last_non_empty_lines() {
    let repo_root = std::env::temp_dir().join(format!(
        "effigy-demo-terminal-tail-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough for test ids")
            .as_nanos()
    ));
    fs::create_dir_all(repo_root.join(".effigy/demo/logs")).expect("create logs dir");
    let path = repo_root.join(".effigy/demo/logs/demo.stdout.log");
    fs::write(&path, "one\n\ntwo\nthree\nfour\n").expect("write log");

    let lines = read_recent_output_lines(&repo_root, ".effigy/demo/logs/demo.stdout.log", 2);
    assert_eq!(lines, vec!["three".to_owned(), "four".to_owned()]);

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn append_demo_terminal_input_appends_text_to_repo_relative_handoff_file() {
    let repo_root = std::env::temp_dir().join(format!(
        "effigy-demo-input-handoff-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough for test ids")
            .as_nanos()
    ));
    fs::create_dir_all(repo_root.join(".effigy/demo/active")).expect("create input dir");
    let rendered_path = ".effigy/demo/active/demo.stdin.log";

    append_demo_terminal_input(&repo_root, rendered_path, "status").expect("append first payload");
    append_demo_terminal_input(&repo_root, rendered_path, "\n").expect("append second payload");

    let written = fs::read_to_string(repo_root.join(rendered_path)).expect("read input file");
    assert_eq!(written, "status\n");

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn append_demo_terminal_resize_appends_jsonl_events() {
    let repo_root = std::env::temp_dir().join(format!(
        "effigy-demo-resize-handoff-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough for test ids")
            .as_nanos()
    ));
    fs::create_dir_all(repo_root.join(".effigy/demo/active")).expect("create resize dir");
    let rendered_path = ".effigy/demo/active/demo.resize.jsonl";

    append_demo_terminal_resize(&repo_root, rendered_path, 120, 32)
        .expect("append first resize payload");

    let written = fs::read_to_string(repo_root.join(rendered_path)).expect("read resize file");
    assert!(written.contains("\"cols\":120"));
    assert!(written.contains("\"rows\":32"));

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn browser_terminal_size_override_reads_valid_env_values() {
    unsafe {
        std::env::set_var(DEMO_BROWSER_TERMINAL_COLS_ENV, "96");
        std::env::set_var(DEMO_BROWSER_TERMINAL_ROWS_ENV, "28");
    }

    let size = browser_terminal_size_override();

    unsafe {
        std::env::remove_var(DEMO_BROWSER_TERMINAL_COLS_ENV);
        std::env::remove_var(DEMO_BROWSER_TERMINAL_ROWS_ENV);
    }

    assert_eq!(size, Some((96, 28)));
}

#[test]
fn wrap_pty_shell_command_prefixes_stty_with_terminal_size() {
    let wrapped = wrap_pty_shell_command("printf demo", Some((96, 28)));

    #[cfg(target_os = "macos")]
    assert_eq!(wrapped, "stty cols 96 rows 28 >/dev/null 2>&1; printf demo");

    #[cfg(not(target_os = "macos"))]
    assert_eq!(wrapped, "printf demo");
}

#[test]
fn render_demo_execute_treats_terminated_attempt_as_non_error_text_result() {
    let record = DemoRecord {
        id: "demo".to_owned(),
        title: "Demo".to_owned(),
        summary: "summary".to_owned(),
        proof: "proof".to_owned(),
        owner: "owner".to_owned(),
        mode: ManifestDemoMode::Interactive,
        status: ManifestDemoStatus::Ready,
        covers: Vec::new(),
        tags: Vec::new(),
        prerequisites: Vec::new(),
        dependencies: Vec::new(),
        entrypoint: DemoEntrypoint::Run(ManifestManagedRun::Command("printf demo".to_owned())),
        sources: vec!["effigy.toml".to_owned()],
        primary_source: "effigy.toml".to_owned(),
        gap_class: "existing",
        runtime_backend: DemoRuntimeBackend::run(),
        active_attempt: DemoActiveAttempt::inactive(None),
        active_terminal_session: DemoActiveTerminalSession::inactive(),
        latest_attempt: DemoLatestAttempt {
            recorded: true,
            receipt_path: Some(".effigy/demo/receipts/demo.json".to_owned()),
            outcome: Some("terminated".to_owned()),
            summary: Some("terminated".to_owned()),
            stale: false,
            artifacts: Vec::new(),
            stdout_log_path: None,
            stderr_log_path: None,
            parse_error: None,
        },
        attempt_history: DemoAttemptHistory {
            path: None,
            attempts: Vec::new(),
            parse_error: None,
        },
    };
    let attempt = terminated_demo_attempt(
        "run",
        "printf demo",
        "printf demo",
        None,
        "Demo `demo` was terminated after stop was requested.".to_owned(),
        String::new(),
        String::new(),
        DemoLogPaths::none(),
    );

    let rendered =
        render_demo_execute_text(&record, &attempt, "Demo Run").expect("render terminated");

    assert!(rendered.contains("outcome: terminated"));
    assert!(!rendered.contains("[error] Task failed"));
}
