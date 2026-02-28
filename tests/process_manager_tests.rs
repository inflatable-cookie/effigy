use effigy::process_manager::{ProcessEventKind, ProcessSpec, ProcessSupervisor};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn supervisor_captures_output_and_exit_events() {
    let root = temp_workspace("supervisor-output");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![
            ProcessSpec {
                name: "alpha".to_owned(),
                run: "printf alpha-out".to_owned(),
                cwd: root.clone(),
                start_after_ms: 0,
                pty: false,
            },
            ProcessSpec {
                name: "beta".to_owned(),
                run: "printf beta-out 1>&2".to_owned(),
                cwd: root.clone(),
                start_after_ms: 0,
                pty: false,
            },
        ],
    )
    .expect("spawn");

    let mut saw_alpha_out = false;
    let mut saw_beta_err = false;
    let mut exits = 0usize;

    for _ in 0..20 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(200)) {
            match event.kind {
                ProcessEventKind::Stdout => {
                    if event.process == "alpha" && event.payload.contains("alpha-out") {
                        saw_alpha_out = true;
                    }
                }
                ProcessEventKind::Stderr => {
                    if event.process == "beta" && event.payload.contains("beta-out") {
                        saw_beta_err = true;
                    }
                }
                ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
                ProcessEventKind::Exit => exits += 1,
            }
            if saw_alpha_out && saw_beta_err && exits >= 2 {
                break;
            }
        }
    }

    assert!(saw_alpha_out);
    assert!(saw_beta_err);
    assert!(exits >= 2);
}

#[test]
fn supervisor_forwards_input_to_target_process() {
    let root = temp_workspace("supervisor-input");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![ProcessSpec {
            name: "reader".to_owned(),
            run: "IFS= read -r line; printf \"seen:%s\\n\" \"$line\"".to_owned(),
            cwd: root.clone(),
            start_after_ms: 0,
            pty: false,
        }],
    )
    .expect("spawn");

    supervisor.send_input("reader", "r\n").expect("send input");

    let mut saw = false;
    for _ in 0..15 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(200)) {
            if event.kind == ProcessEventKind::Stdout && event.payload.contains("seen:r") {
                saw = true;
                break;
            }
        }
    }

    supervisor.terminate_all();
    assert!(saw, "expected forwarded stdin output");
}

#[test]
fn supervisor_forwards_input_without_wait_lock_contention() {
    let root = temp_workspace("supervisor-input-streaming");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![ProcessSpec {
            name: "reader".to_owned(),
            run: "while IFS= read -r line; do printf \"seen:%s\\n\" \"$line\"; done".to_owned(),
            cwd: root.clone(),
            start_after_ms: 0,
            pty: false,
        }],
    )
    .expect("spawn");

    supervisor
        .send_input("reader", "first\n")
        .expect("send first");
    supervisor
        .send_input("reader", "second\n")
        .expect("send second");

    let mut saw_first = false;
    let mut saw_second = false;
    for _ in 0..25 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.kind == ProcessEventKind::Stdout && event.payload.contains("seen:first") {
                saw_first = true;
            }
            if event.kind == ProcessEventKind::Stdout && event.payload.contains("seen:second") {
                saw_second = true;
            }
            if saw_first && saw_second {
                break;
            }
        }
    }

    supervisor.terminate_all();
    assert!(
        saw_first && saw_second,
        "expected both forwarded stdin outputs"
    );
}

#[test]
fn supervisor_graceful_shutdown_terminates_long_running_process() {
    let root = temp_workspace("supervisor-graceful-shutdown");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![ProcessSpec {
            name: "sleeper".to_owned(),
            run: "sleep 30".to_owned(),
            cwd: root.clone(),
            start_after_ms: 0,
            pty: false,
        }],
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
        vec![ProcessSpec {
            name: "delayed".to_owned(),
            run: "printf delayed-ready".to_owned(),
            cwd: root.clone(),
            start_after_ms: 150,
            pty: false,
        }],
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
        vec![ProcessSpec {
            name: "sleeper".to_owned(),
            run: "sleep 30".to_owned(),
            cwd: root.clone(),
            start_after_ms: 0,
            pty: false,
        }],
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
        vec![ProcessSpec {
            name: "service".to_owned(),
            run: "echo booted; sleep 30".to_owned(),
            cwd: root.clone(),
            start_after_ms: 0,
            pty: false,
        }],
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

fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-process-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}
