use effigy::process_manager::{ProcessEventKind, ProcessSpec, ProcessSupervisor};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn supervisor_captures_output_and_exit_events() {
    let root = temp_workspace("supervisor-output");
    let supervisor = ProcessSupervisor::spawn(
        root,
        vec![
            ProcessSpec {
                name: "alpha".to_owned(),
                run: "printf alpha-out".to_owned(),
            },
            ProcessSpec {
                name: "beta".to_owned(),
                run: "printf beta-out 1>&2".to_owned(),
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
        root,
        vec![ProcessSpec {
            name: "reader".to_owned(),
            run: "IFS= read -r line; printf \"seen:%s\\n\" \"$line\"".to_owned(),
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

fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-process-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}
