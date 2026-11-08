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
        .arg(&script)
        .arg(env!("CARGO_BIN_EXE_effigy"))
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
