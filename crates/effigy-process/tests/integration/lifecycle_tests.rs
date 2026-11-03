use effigy_process::{ProcessEventKind, ProcessManagerError, ProcessSupervisor};
use std::time::{Duration, Instant};

use super::support::{process_exists, process_spec, process_spec_with_delay, temp_workspace};
use std::fs;

#[test]
fn supervisor_graceful_shutdown_terminates_long_running_process() {
    let root = temp_workspace("supervisor-graceful-shutdown");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![process_spec("sleeper", "sleep 30", &root)],
    )
    .expect("spawn");

    supervisor.terminate_all_graceful(Duration::from_millis(500));

    let mut saw_exit = false;
    let mut exit_payload = String::new();
    for _ in 0..20 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.kind == ProcessEventKind::Exit && event.process == "sleeper" {
                saw_exit = true;
                exit_payload = event.payload;
                break;
            }
        }
    }

    assert!(
        saw_exit,
        "expected sleeper to exit during graceful shutdown"
    );
    // The stop-record contract the tui session parser consumes
    // (`ProcessExitState::from_record`): a stop-sent signal is reported as
    // `signal=<n>` so the session records a stop, never a plain exit code.
    assert_eq!(
        exit_payload, "signal=15",
        "a graceful stop must report its terminating signal"
    );
}

#[test]
fn supervisor_respects_process_start_delay() {
    let root = temp_workspace("supervisor-start-delay");
    let start = Instant::now();
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![process_spec_with_delay(
            "delayed",
            "printf delayed-ready",
            &root,
            150,
        )],
    )
    .expect("spawn");

    let mut saw_output = false;
    for _ in 0..20 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(50)) {
            if event.kind == ProcessEventKind::Stdout && event.process == "delayed" {
                saw_output = true;
                break;
            }
        }
    }

    assert!(saw_output, "expected delayed process output");
    assert!(
        start.elapsed() >= Duration::from_millis(120),
        "expected process start delay to be applied"
    );
}

#[test]
fn supervisor_can_terminate_individual_process() {
    let root = temp_workspace("supervisor-stop-process");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![process_spec("sleeper", "sleep 30", &root)],
    )
    .expect("spawn");

    supervisor
        .terminate_process("sleeper")
        .expect("terminate process");

    // The refusal contract the tui session surface consumes: an unknown
    // process is refused, never silently ignored.
    let missing = supervisor.terminate_process("absent-process");
    assert!(
        matches!(missing, Err(ProcessManagerError::ProcessNotFound { .. })),
        "terminating an unknown process must refuse with ProcessNotFound, got {missing:?}"
    );

    let mut saw_exit = false;
    for _ in 0..20 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.kind == ProcessEventKind::Exit && event.process == "sleeper" {
                saw_exit = true;
                break;
            }
        }
    }
    assert!(
        saw_exit,
        "expected sleeper exit event after terminate_process"
    );
}

#[test]
fn supervisor_can_restart_individual_process() {
    let root = temp_workspace("supervisor-restart-process");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![process_spec("service", "echo booted; sleep 30", &root)],
    )
    .expect("spawn");

    let mut booted_count = 0usize;
    for _ in 0..20 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.kind == ProcessEventKind::Stdout
                && event.process == "service"
                && event.payload.contains("booted")
            {
                booted_count += 1;
                break;
            }
        }
    }

    supervisor
        .restart_process("service")
        .expect("restart process");

    let missing = supervisor.restart_process("absent-process");
    assert!(
        matches!(missing, Err(ProcessManagerError::ProcessNotFound { .. })),
        "restarting an unknown process must refuse with ProcessNotFound, got {missing:?}"
    );

    for _ in 0..30 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(120)) {
            if event.kind == ProcessEventKind::Stdout
                && event.process == "service"
                && event.payload.contains("booted")
            {
                booted_count += 1;
                break;
            }
        }
    }
    // The restarted replacement keeps running; stop it before asserting so
    // the fixture owns its process on every exit path.
    supervisor.terminate_all();
    assert!(
        booted_count >= 2,
        "expected service to emit startup output after restart"
    );
}

#[cfg(unix)]
#[test]
fn supervisor_graceful_shutdown_terminates_descendants_with_separate_process_groups() {
    let root = temp_workspace("supervisor-shutdown-descendant-groups");
    let pid_file = root.join("descendant.pid");
    let run = format!(
        "python3 -c 'import os, pathlib, subprocess, time; child = subprocess.Popen([\"sleep\", \"30\"], preexec_fn=os.setpgrp); pathlib.Path(r\"{}\").write_text(str(child.pid)); time.sleep(30)'",
        pid_file.display()
    );
    let supervisor =
        ProcessSupervisor::spawn(root.clone(), vec![process_spec("parent", &run, &root)])
            .expect("spawn");

    let descendant_pid = wait_for_descendant_pid(&pid_file);
    assert!(
        process_exists(descendant_pid),
        "expected descendant process to be running before shutdown"
    );

    supervisor.terminate_all_graceful(Duration::from_millis(800));

    let mut saw_parent_exit = false;
    for _ in 0..20 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.kind == ProcessEventKind::Exit && event.process == "parent" {
                saw_parent_exit = true;
                break;
            }
        }
    }
    assert!(
        saw_parent_exit,
        "expected parent exit event during shutdown"
    );

    let descendant_stopped = (0..20).any(|_| {
        let exists = process_exists(descendant_pid);
        if exists {
            std::thread::sleep(Duration::from_millis(100));
        }
        !exists
    });
    assert!(
        descendant_stopped,
        "expected graceful shutdown to terminate descendant pid {descendant_pid}"
    );
}

#[cfg(unix)]
fn wait_for_descendant_pid(pid_file: &std::path::Path) -> i32 {
    for _ in 0..20 {
        if let Ok(rendered) = fs::read_to_string(pid_file) {
            if let Ok(pid) = rendered.trim().parse::<i32>() {
                return pid;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("expected descendant pid file at {}", pid_file.display());
}

/// `spawn_progressively` is the `effigy open` shell startup path: managed_run
/// and the tui session drive its event vocabulary, its cancellation and its
/// exit diagnostics. This pins the whole sequence: lifecycle events for the
/// first spec, a pending delayed start that keeps startup unfinished, a
/// cancel that joins the worker before the pending spec can spawn, and the
/// diagnostics a never-started spec reports (`not-tracked`) against a
/// finished one (`exit=0`).
#[test]
fn supervisor_progressive_startup_reports_lifecycle_and_cancel_stops_pending_start() {
    let root = temp_workspace("supervisor-progressive-cancel");
    let supervisor = ProcessSupervisor::spawn_progressively(
        root.clone(),
        vec![
            process_spec("boots", "printf boots-ready", &root),
            process_spec_with_delay("never-starts", "sleep 30", &root, 2000),
        ],
    );

    let mut starting_seen = false;
    let mut started_seen = false;
    let mut exit_seen = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !(starting_seen && started_seen && exit_seen) {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.process == "boots" {
                match event.kind {
                    ProcessEventKind::Starting => starting_seen = true,
                    ProcessEventKind::Started => started_seen = true,
                    ProcessEventKind::Exit => exit_seen = true,
                    _ => {}
                }
            }
        }
    }
    assert!(
        starting_seen && started_seen && exit_seen,
        "expected the first spec to report Starting, Started and Exit in order"
    );

    // The worker is parked inside the delayed start, so startup is unfinished.
    assert!(
        !supervisor.startup_finished(),
        "a pending delayed start must keep startup unfinished"
    );

    supervisor.cancel_startup();
    assert!(
        supervisor.startup_finished(),
        "cancel_startup must join the worker before returning"
    );
    assert!(
        supervisor.take_startup_error().is_none(),
        "a cancelled start is not a startup error"
    );

    let diagnostics = supervisor.exit_diagnostics();
    let find = |name: &str| {
        diagnostics
            .iter()
            .find(|(process, _)| process == name)
            .map(|(_, diagnostic)| diagnostic.as_str())
    };
    assert_eq!(find("never-starts"), Some("not-tracked"));
    assert_eq!(find("boots"), Some("exit=0"));

    let mut delayed_seen = false;
    for _ in 0..10 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.process == "never-starts" {
                delayed_seen = true;
            }
        }
    }
    assert!(
        !delayed_seen,
        "a cancelled pending start must never report Starting or Started"
    );
}
