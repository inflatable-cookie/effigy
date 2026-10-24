pub(super) fn write_task_command_launch(
    f: &mut std::fmt::Formatter<'_>,
    command: &str,
    error: &std::io::Error,
) -> std::fmt::Result {
    write!(f, "failed to launch task command `{command}`: {error}")
}

pub(super) fn write_task_command_failure(
    f: &mut std::fmt::Formatter<'_>,
    command: &str,
    code: &Option<i32>,
    stdout: &str,
    stderr: &str,
) -> std::fmt::Result {
    // The sentence names the command and its exit code, and shows the bounded
    // tail of its own output. `--json` keeps the full streams.
    f.write_str(
        &crate::runner::error::task_command_failure_human(command, *code, stdout, stderr)
            .render_text_body(),
    )
}

pub(super) fn write_lock_conflict(
    f: &mut std::fmt::Formatter<'_>,
    details: &effigy_core::task_lock::TaskLockConflict,
) -> std::fmt::Result {
    f.write_str(&details.remediation)
}

pub(super) fn write_defer_loop_detected(
    f: &mut std::fmt::Formatter<'_>,
    depth: u8,
) -> std::fmt::Result {
    write!(
        f,
        "deferral loop detected ({} deferral hop(s)); refusing to defer again",
        depth
    )
}
