use effigy::process_manager::{ProcessEventKind, ProcessSupervisor};
use std::time::Duration;

use super::support::{process_spec, temp_workspace};

#[test]
fn supervisor_captures_output_and_exit_events() {
    let root = temp_workspace("supervisor-output");
    let supervisor = ProcessSupervisor::spawn(
        root.clone(),
        vec![
            process_spec("alpha", "printf alpha-out", &root),
            process_spec("beta", "printf beta-out 1>&2", &root),
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
        vec![process_spec(
            "reader",
            "IFS= read -r line; printf \"seen:%s\\n\" \"$line\"",
            &root,
        )],
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
        vec![process_spec(
            "reader",
            "while IFS= read -r line; do printf \"seen:%s\\n\" \"$line\"; done",
            &root,
        )],
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
