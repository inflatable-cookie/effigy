use effigy_process::{ProcessEventKind, ProcessSupervisor};
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
