use effigy_process::{ProcessEventKind, ProcessSupervisor};
use std::time::{Duration, Instant};

use super::support::{process_spec, process_spec_with_delay, temp_workspace};

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
        "expected sleeper to exit during graceful shutdown"
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
    assert!(
        booted_count >= 2,
        "expected service to emit startup output after restart"
    );
}
