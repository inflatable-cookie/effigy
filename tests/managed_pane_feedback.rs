//! Exercise managed pane bytes through the real PTY and supervisor.
//!
//! Both fixtures drive the real binary in a real PTY and judge its canonical
//! pane logs plus the rendered screen stream: a pane is not a terminal for the
//! process inside it, so its feedback must stay bounded plain lines.
#[cfg(unix)]
fn pane_fixture(script: &str) {
    use std::process::Command;
    let script = format!("{}/tests/scripts/{script}", env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("python3")
        .arg("-B")
        .arg(&script)
        .arg(env!("CARGO_BIN_EXE_effigy"))
        .env(
            "EFFIGY_PANE_LIVE_STEP_FIXTURE",
            std::env::current_exe().expect("pane fixture binary"),
        )
        .output()
        .expect("run pane fixture");
    assert!(
        output.status.success(),
        "{script} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn managed_pane_feedback_stays_plain() {
    pane_fixture("managed-pane-feedback.py");
}

/// effigy#729: a tab program that starts Effigy with the internal namespace
/// scrubbed (the shipped bundle's `profile_env.py`) is still inside the pane,
/// so its child draws one bounded start and finish per step, no banner and no
/// staircase of live frames - and the failure stays readable.
#[cfg(unix)]
#[test]
fn a_nested_child_through_a_scrubbing_wrapper_stays_bounded() {
    pane_fixture("managed-pane-nested.py");
}

/// The Python PTY owner releases this non-streaming step only after observing
/// five real redraw frames. Producer streaming is proved separately.
#[cfg(unix)]
#[test]
#[ignore = "PTY fixture launched by both pane proofs"]
fn non_streaming_live_step_fixture() {
    use effigy_ui::steps::{configure_from_terminal, live_step, ProgressSource};

    configure_from_terminal(false, false);
    let step = live_step(ProgressSource::waiting_for("fixture-animation"));
    let mut release = String::new();
    std::io::stdin().read_line(&mut release).unwrap();
    assert_eq!(release, "release\n");
    step.succeed("animation complete");
}
