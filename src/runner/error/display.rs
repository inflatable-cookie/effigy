use super::RunnerError;
use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, strict_manifest_parse_failed_in_path,
};
use effigy_manifest::TASK_MANIFEST_FILE;

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
    write_task_command_launch,
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
        RunnerError::ContainerWaitRefused { diagnostic, .. } => f.write_str(diagnostic),
        RunnerError::ManagedHostReadiness {
            process,
            diagnostic,
            observed_status,
        } => match observed_status {
            Some(status) => write!(
                f,
                "managed host listener `{process}` failed before readiness after answering HTTP {status} ({diagnostic})"
            ),
            None => write!(
                f,
                "managed host listener `{process}` failed before readiness ({diagnostic})"
            ),
        },
        RunnerError::ManagedListenerExposure {
            environment,
            exposed,
            failures,
        } => {
            write!(
                f,
                "environment `{environment}` settled with {} route(s) withheld and {} verified and published{}",
                failures.len(),
                exposed.len(),
                if exposed.is_empty() {
                    String::new()
                } else {
                    format!(": {}", exposed.join(", "))
                }
            )?;
            for failure in failures {
                write!(f, "\n[withheld] {}", failure.summary())?;
                if let Some(tail) = &failure.output_tail {
                    write!(f, "\n  last child output: {tail}")?;
                }
            }
            Ok(())
        }
        RunnerError::ContainerSurfaceRegistryMissing => {
            write!(f, "manifest does not define a `[containers]` registry")
        }
        RunnerError::ContainerSurfaceDefaultTargetMissing => write!(
            f,
            "`effigy exec` requires `[systems].default` to resolve to a workspace with a backing container"
        ),
        RunnerError::ContainerSurfaceNotDefined { container } => write!(
            f,
            "container `{container}` is not defined in `[containers]`"
        ),
        RunnerError::ContainerSurfaceNotRunning { container } => write!(
            f,
            "container `{container}` is not running — start it with `effigy container {container} up`"
        ),
        RunnerError::ContainerSurfacePolicy {
            phase,
            container,
            detail,
        } => write!(
            f,
            "container surface {phase} failed for `{container}`: {detail}"
        ),
        RunnerError::WorkspaceSessionCleanup {
            shell_error,
            cleanup_error,
        } => write!(
            f,
            "{shell_error}\nworkspace cleanup also failed: {cleanup_error}"
        ),
        RunnerError::GatewayRouteTable {
            phase,
            path,
            detail,
        } => write!(
            f,
            "gateway route table {phase} failed at {}: {detail}",
            path.display()
        ),
        RunnerError::GatewayRouteRegistration {
            phase,
            domain,
            detail,
        } => write!(
            f,
            "gateway route {phase} failed for `{domain}`: {detail}"
        ),
        RunnerError::GatewayRouteShape { phase, detail } => {
            write!(f, "gateway route {phase} failed: {detail}")
        }
        RunnerError::GatewayLoopback { phase, detail } => {
            write!(f, "gateway loopback {phase} failed: {detail}")
        }
        RunnerError::GatewayRuntimeTarget { phase, detail } => {
            write!(f, "gateway runtime target {phase} failed: {detail}")
        }
        RunnerError::ContainerRuntimePolicy { phase, detail } => {
            write!(f, "container runtime {phase} failed: {detail}")
        }
        RunnerError::TaskCatalogsMissing { root } => write_catalogs_missing(f, root),
        RunnerError::TaskManifestRead { path, error } => {
            write!(f, "{}", failed_to_read_path(path, error))
        }
        RunnerError::TaskManifestParse {
            path,
            error,
            stale_local_install,
        } => {
            write!(f, "{}", failed_to_parse_path(path, error))?;
            if let Some(stale) = stale_local_install.as_deref() {
                write!(
                    f,
                    "\n\nrepository-local install is behind this checkout: {} records {}, but {} is at {}\nrefresh it from the checkout root with: cargo run --bin effigy -- install:local",
                    stale.executable.display(),
                    stale.installed_identity,
                    stale.repo_root.display(),
                    stale.current_identity
                )?;
            }
            Ok(())
        }
        RunnerError::TaskManifestCompose { path, detail } => {
            write!(f, "{}", strict_manifest_parse_failed_in_path(path, detail))
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
        RunnerError::TaskNotFound { name, path } => write!(
            f,
            "task `{name}` is not defined in {}; run `effigy tasks` to see the task names this \
             checkout routes",
            path.display()
        ),
        RunnerError::TaskNotFoundAny { name, catalogs } => write!(
            f,
            "task `{name}` is not defined in effective catalogs: {}; run `effigy tasks` to see the \
             task names this checkout routes",
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
        RunnerError::TaskLockConflict(details) => write_lock_conflict(f, details),
        RunnerError::RunAdmission { message, .. } => f.write_str(message),
        RunnerError::AdmissionRefused { detail, .. }
        | RunnerError::RunControl { detail, .. } => f.write_str(detail),
        RunnerError::RunSettled { code } => write!(f, "the run settled with exit {code}"),
        RunnerError::RunKeyExists { run } => write!(f, "the `--run-id` key names run {run}"),
        RunnerError::CommandJsonFailure { .. } => {
            write!(f, "command failed (json output available)")
        }
        RunnerError::BoundedExecFailure { message, .. } => f.write_str(message),
        RunnerError::DepsOperationNonZero {
            command,
            outcome,
            error_count,
            ..
        } => write!(
            f,
            "{command} failed (outcome: {outcome}; {error_count} reported error{})",
            if *error_count == 1 { "" } else { "s" }
        ),
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
        RunnerError::BuiltinTestNonZero {
            failures,
            cancelled,
            ..
        } => write_builtin_test_non_zero(f, failures, *cancelled),
        RunnerError::BuiltinScanNonZero { finding_count, .. } => {
            write_builtin_scan_non_zero(f, *finding_count)
        }
        RunnerError::DoctorNonZero { error_count, .. } => write_doctor_non_zero(f, *error_count),
        RunnerError::DeferLoopDetected { depth } => write_defer_loop_detected(f, *depth),
        RunnerError::EnvSchema(err) => write!(f, "{err}"),
    }
}

fn write_catalogs_missing(
    f: &mut std::fmt::Formatter<'_>,
    root: &std::path::Path,
) -> std::fmt::Result {
    write!(
        f,
        "no root catalog manifest found at {}",
        root.join(TASK_MANIFEST_FILE).display()
    )
}
