//! Exercise managed pane bytes through the real PTY and supervisor.
#[cfg(unix)]
#[test]
fn managed_pane_feedback_stays_plain() {
    use std::process::Command;
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/scripts/managed-pane-feedback.py"
        ))
        .arg(env!("CARGO_BIN_EXE_effigy"))
        .output()
        .expect("run pane fixture");
    assert!(
        output.status.success(),
        "pane fixture failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
