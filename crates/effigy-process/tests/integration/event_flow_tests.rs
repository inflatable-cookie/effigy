use effigy_process::{ProcessEventKind, ProcessSupervisor};
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
                ProcessEventKind::Starting
                | ProcessEventKind::Started
                | ProcessEventKind::StartupFailed => {}
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
                ProcessEventKind::Styled => {
                    panic!("raw process supervisor emitted a custody style event")
                }
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

/// The pty wrapper is the `effigy open` shell-tab path: the spec's terminal
/// size override must reach the wrapped command (the `stty cols/rows` prefix)
/// and the command's output must stream back through the pty as Stdout
/// events. macOS and Linux invoke the `script` wrapper with different argv,
/// so this case exercises the platform's own production invocation.
#[test]
fn supervisor_pty_wrapper_applies_terminal_size_and_streams_output() {
    let root = temp_workspace("supervisor-pty-terminal-size");
    let mut spec = process_spec("pty-shell", "stty size", &root);
    spec.pty = true;
    spec.env
        .insert("EFFIGY_BROWSER_TERMINAL_COLS".to_owned(), "96".to_owned());
    spec.env
        .insert("EFFIGY_BROWSER_TERMINAL_ROWS".to_owned(), "28".to_owned());
    let supervisor = ProcessSupervisor::spawn(root.clone(), vec![spec]).expect("spawn pty");

    let mut size_seen = false;
    let mut exit_seen = false;
    for _ in 0..40 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if event.process == "pty-shell" {
                if event.kind == ProcessEventKind::Stdout && event.payload.contains("28 96") {
                    size_seen = true;
                }
                if event.kind == ProcessEventKind::Exit {
                    exit_seen = true;
                }
            }
            if size_seen && exit_seen {
                break;
            }
        }
    }

    supervisor.terminate_all();
    assert!(
        size_seen,
        "expected the pty wrapper to apply the terminal size override (stty size reports rows cols)"
    );
    assert!(exit_seen, "expected the pty process to report Exit");
}
