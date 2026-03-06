use super::super::TASK_MANIFEST_FILE;
use super::RunnerError;
use crate::path_error_text::{failed_to_parse_path, failed_to_read_path};

#[path = "display/builtin.rs"]
mod builtin;
#[path = "display/managed.rs"]
mod managed;
#[path = "display/task_failures.rs"]
mod task_failures;

use builtin::{write_builtin_scan_non_zero, write_builtin_test_non_zero, write_doctor_non_zero};
use managed::{
    write_managed_non_zero_exit, write_task_managed_process_invalid_definition,
    write_task_managed_process_not_found, write_task_managed_profile_empty,
    write_task_managed_profile_not_found, write_task_managed_profile_tab_order_invalid,
    write_task_managed_task_reference_invalid, write_task_managed_unsupported_mode,
};
use task_failures::{
    write_defer_loop_detected, write_lock_conflict, write_task_command_failure,
    write_task_command_launch, write_task_lock_io,
};

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
        RunnerError::TaskCatalogsMissing { root } => write_catalogs_missing(f, root),
        RunnerError::TaskCatalogReadDir { path, error } => {
            write!(f, "failed to read directory {}: {error}", path.display())
        }
        RunnerError::TaskManifestRead { path, error } => {
            write!(f, "{}", failed_to_read_path(path, error))
        }
        RunnerError::TaskManifestParse { path, error } => {
            write!(f, "{}", failed_to_parse_path(path, error))
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
            write_task_command_launch(f, command, error)
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
        RunnerError::TaskLockIo { path, error } => write_task_lock_io(f, path, error),
        RunnerError::CommandJsonFailure { .. } => {
            write!(f, "command failed (json output available)")
        }
        RunnerError::ManagedProcess(error) => write!(f, "{error}"),
        RunnerError::TaskManagedUnsupportedMode { task, mode } => {
            write_task_managed_unsupported_mode(f, task, mode)
        }
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => write_task_managed_profile_not_found(f, task, profile, available),
        RunnerError::TaskManagedProfileEmpty { task, profile } => {
            write_task_managed_profile_empty(f, task, profile)
        }
        RunnerError::TaskManagedProcessNotFound {
            task,
            profile,
            process,
        } => write_task_managed_process_not_found(f, task, profile, process),
        RunnerError::TaskManagedProcessInvalidDefinition {
            task,
            process,
            detail,
        } => write_task_managed_process_invalid_definition(f, task, process, detail),
        RunnerError::TaskManagedProfileTabOrderInvalid {
            task,
            profile,
            detail,
        } => write_task_managed_profile_tab_order_invalid(f, task, profile, detail),
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => write_task_managed_task_reference_invalid(f, task, process, reference, detail),
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
        RunnerError::BuiltinScanNonZero { finding_count, .. } => {
            write_builtin_scan_non_zero(f, *finding_count)
        }
        RunnerError::DoctorNonZero { error_count, .. } => write_doctor_non_zero(f, *error_count),
        RunnerError::DeferLoopDetected { depth } => write_defer_loop_detected(f, *depth),
    }
}

fn write_catalogs_missing(
    f: &mut std::fmt::Formatter<'_>,
    root: &std::path::Path,
) -> std::fmt::Result {
    write!(
        f,
        "no task catalogs found under {} (expected one or more {} files)",
        root.display(),
        TASK_MANIFEST_FILE
    )
}
