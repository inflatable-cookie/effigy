use super::super::{RunnerError, TASK_MANIFEST_FILE};

pub(super) fn fmt_runner_error(
    error: &RunnerError,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    match error {
        RunnerError::Cwd(err) => write!(f, "failed to resolve current directory: {err}"),
        RunnerError::Resolve(err) => write!(f, "{err}"),
        RunnerError::Task(err) => write!(f, "{err}"),
        RunnerError::Ui(msg) => write!(f, "ui render failed: {msg}"),
        RunnerError::TaskInvocation(msg) => write!(f, "{msg}"),
        RunnerError::TaskCatalogsMissing { root } => write!(
            f,
            "no task catalogs found under {} (expected one or more {} files)",
            root.display(),
            TASK_MANIFEST_FILE
        ),
        RunnerError::TaskCatalogReadDir { path, error } => {
            write!(f, "failed to read directory {}: {error}", path.display())
        }
        RunnerError::TaskManifestRead { path, error } => {
            write!(f, "failed to read {}: {error}", path.display())
        }
        RunnerError::TaskManifestParse { path, error } => {
            write!(f, "failed to parse {}: {error}", path.display())
        }
        RunnerError::TaskCatalogAliasConflict {
            alias,
            first_path,
            second_path,
        } => write!(
            f,
            "duplicate task catalog alias `{alias}` found in {} and {}",
            first_path.display(),
            second_path.display()
        ),
        RunnerError::TaskCatalogPrefixNotFound { prefix, available } => write!(
            f,
            "task catalog prefix `{prefix}` not found (available: {})",
            available.join(", ")
        ),
        RunnerError::TaskNotFound { name, path } => {
            write!(f, "task `{name}` is not defined in {}", path.display())
        }
        RunnerError::TaskNotFoundAny { name, catalogs } => write!(
            f,
            "task `{name}` is not defined in discovered catalogs: {}",
            catalogs.join(", ")
        ),
        RunnerError::TaskAmbiguous { name, candidates } => write!(
            f,
            "task `{name}` is ambiguous; matched multiple catalogs: {}",
            candidates.join(", ")
        ),
        RunnerError::TaskCommandLaunch { command, error } => {
            write!(f, "failed to launch task command `{command}`: {error}")
        }
        RunnerError::TaskCommandFailure {
            command,
            code,
            stdout,
            stderr,
        } => write_task_command_failure(f, command, code, stdout, stderr),
        RunnerError::TaskLockConflict {
            scope,
            lock_path,
            holder_pid,
            holder_started_at_epoch_ms,
            remediation,
        } => write_lock_conflict(
            f,
            scope,
            lock_path,
            holder_pid,
            holder_started_at_epoch_ms,
            remediation,
        ),
        RunnerError::TaskLockIo { path, error } => {
            write!(f, "lock I/O failed at {}: {error}", path.display())
        }
        RunnerError::CommandJsonFailure { .. } => {
            write!(f, "command failed (json output available)")
        }
        RunnerError::ManagedProcess(error) => write!(f, "{error}"),
        RunnerError::TaskManagedUnsupportedMode { task, mode } => write!(
            f,
            "task `{task}` declares unsupported managed mode `{mode}` (expected `tui`)"
        ),
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => write!(
            f,
            "managed task `{task}` profile `{profile}` not found (available: {})",
            available.join(", ")
        ),
        RunnerError::TaskManagedProfileEmpty { task, profile } => write!(
            f,
            "managed task `{task}` profile `{profile}` has no processes configured"
        ),
        RunnerError::TaskManagedProcessNotFound {
            task,
            profile,
            process,
        } => write!(
            f,
            "managed task `{task}` profile `{profile}` references undefined process `{process}`"
        ),
        RunnerError::TaskManagedProcessInvalidDefinition {
            task,
            process,
            detail,
        } => write!(
            f,
            "managed task `{task}` process `{process}` is invalid: {detail}"
        ),
        RunnerError::TaskManagedProfileTabOrderInvalid {
            task,
            profile,
            detail,
        } => write!(
            f,
            "managed task `{task}` profile `{profile}` tab order is invalid: {detail}"
        ),
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => write!(
            f,
            "managed task `{task}` process `{process}` task ref `{reference}` is invalid: {detail}"
        ),
        RunnerError::TaskManagedNonZeroExit {
            task,
            profile,
            processes,
        } => write_managed_non_zero_exit(f, task, profile, processes),
        RunnerError::TaskMissingRunCommand { task, path } => write!(
            f,
            "task `{task}` in {} is missing `run` command (required for non-managed tasks)",
            path.display()
        ),
        RunnerError::BuiltinTestNonZero { failures, .. } => {
            write_builtin_test_non_zero(f, failures)
        }
        RunnerError::DoctorNonZero { error_count, .. } => {
            write!(f, "doctor found {error_count} error finding(s)")
        }
        RunnerError::DeferLoopDetected { depth } => write!(
            f,
            "deferral loop detected ({} deferral hop(s)); refusing to defer again",
            depth
        ),
    }
}

fn render_optional<T: std::fmt::Display>(value: &Option<T>) -> String {
    value
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "<unknown>".to_owned())
}

fn write_task_command_failure(
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

fn write_lock_conflict(
    f: &mut std::fmt::Formatter<'_>,
    scope: &str,
    lock_path: &std::path::Path,
    holder_pid: &Option<u32>,
    holder_started_at_epoch_ms: &Option<u128>,
    remediation: &str,
) -> std::fmt::Result {
    write!(
        f,
        "lock conflict for `{scope}` (holder_pid={}, started_at_epoch_ms={}, lock={}); {remediation}",
        render_optional(holder_pid),
        render_optional(holder_started_at_epoch_ms),
        lock_path.display()
    )
}

fn write_managed_non_zero_exit(
    f: &mut std::fmt::Formatter<'_>,
    task: &str,
    profile: &str,
    processes: &[(String, String)],
) -> std::fmt::Result {
    let rendered = processes
        .iter()
        .map(|(name, diagnostic)| format!("{name} ({diagnostic})"))
        .collect::<Vec<String>>()
        .join(", ");
    write!(
        f,
        "managed task `{task}` profile `{profile}` had non-zero exits: {rendered}"
    )
}

fn write_builtin_test_non_zero(
    f: &mut std::fmt::Formatter<'_>,
    failures: &[(String, Option<i32>)],
) -> std::fmt::Result {
    let rendered = failures
        .iter()
        .map(|(target, code)| match code {
            Some(value) => format!("{target}: exit={value}"),
            None => format!("{target}: terminated"),
        })
        .collect::<Vec<String>>()
        .join(", ");
    write!(f, "one or more built-in test targets failed: {rendered}")
}
