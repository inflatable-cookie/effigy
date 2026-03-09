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
    if stdout.is_empty() && stderr.is_empty() {
        return write!(f, "task command failed `{command}` (code={:?})", code);
    }
    write!(
        f,
        "task command failed `{command}` (code={:?})\nstdout:\n{}\nstderr:\n{}",
        code, stdout, stderr
    )
}

pub(super) fn write_lock_conflict(
    f: &mut std::fmt::Formatter<'_>,
    scope: &str,
    lock_path: &std::path::Path,
    holder_pid: &Option<u32>,
    holder_started_at_epoch_ms: &Option<u128>,
    holder_heartbeat_at_epoch_ms: &Option<u128>,
    holder_hostname: &Option<String>,
    holder_workspace_root: &Option<String>,
    remediation: &str,
) -> std::fmt::Result {
    write!(
        f,
        "lock conflict for `{scope}` (holder_pid={}, started_at_epoch_ms={}, heartbeat_at_epoch_ms={}, holder_hostname={}, holder_workspace_root={}, lock={}); {remediation}",
        render_optional(holder_pid),
        render_optional(holder_started_at_epoch_ms),
        render_optional(holder_heartbeat_at_epoch_ms),
        render_optional(holder_hostname),
        render_optional(holder_workspace_root),
        lock_path.display()
    )
}

pub(super) fn write_task_lock_io(
    f: &mut std::fmt::Formatter<'_>,
    path: &std::path::Path,
    error: &std::io::Error,
) -> std::fmt::Result {
    write!(f, "lock I/O failed at {}: {error}", path.display())
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

fn render_optional<T: std::fmt::Display>(value: &Option<T>) -> String {
    value
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "<unknown>".to_owned())
}
