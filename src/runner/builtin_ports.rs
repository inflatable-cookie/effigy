//! Concrete [`BuiltinRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-builtin`. This module threads the
//! built-in layer's reach-backs into the runner's `task_engine`,
//! `doctor`, `execute`, `command_context`, and `tasks_command`
//! modules, converting `RunnerError` to `BuiltinError` at the port
//! boundary so the built-in layer only speaks its own error type.
mod test_session;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_builtin::{
    BuiltinContainerSuiteTarget, BuiltinError, BuiltinLockGuards, BuiltinRuntimePorts,
    BuiltinTestChildEvidence, LockScope as BuiltinLockScope,
};
use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
use effigy_execution::ExecutionSurface;
use effigy_manifest::LoadedCatalog;

use crate::runner::command_context;
use crate::runner::doctor_ports::RunnerDoctorPorts;
use crate::runner::error::RunnerError;
use crate::runner::execute::api;
use crate::runner::task_engine::TaskRun;
use crate::runner::tasks_command;

/// Concrete `BuiltinRuntimePorts` implementation that delegates to the
/// runner modules directly.
///
/// A caller that owns suite custody lends its memory or durable run here:
/// each host suite then runs as a child run of it, so the suite's process tree
/// is custody the run settles before releasing its gates and any scratch.
pub(in crate::runner) struct RunnerBuiltinPorts<'a> {
    suite_parent: Option<std::cell::RefCell<&'a mut TaskRun>>,
    next_suite: std::cell::Cell<usize>,
}

impl std::fmt::Debug for RunnerBuiltinPorts<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerBuiltinPorts")
            .field("owns_host_suites", &self.suite_parent.is_some())
            .finish()
    }
}

impl Default for RunnerBuiltinPorts<'_> {
    fn default() -> Self {
        Self {
            suite_parent: None,
            next_suite: std::cell::Cell::new(0),
        }
    }
}

impl<'a> RunnerBuiltinPorts<'a> {
    pub(in crate::runner) fn new() -> RunnerBuiltinPorts<'static> {
        RunnerBuiltinPorts::default()
    }

    /// Lends the built-in `test` route's run, so its host suites become child
    /// runs of it.
    pub(in crate::runner) fn with_suite_run(run: &'a mut TaskRun) -> RunnerBuiltinPorts<'a> {
        RunnerBuiltinPorts {
            suite_parent: Some(std::cell::RefCell::new(run)),
            next_suite: std::cell::Cell::new(0),
        }
    }
}

/// One host suite's custody: a child run of the caller's run, closed on the
/// worker that ran it.
struct SuiteCustody {
    run: Option<TaskRun>,
    label: String,
    /// The outcome the runner observed for the attempt it ran, recorded when
    /// the attempt settles.
    attempt: Option<effigy_kernel::outcome::WorkloadOutcome>,
}

impl effigy_builtin::HostSuiteCustody for SuiteCustody {
    fn run(
        &mut self,
        command: &str,
        cwd: &Path,
        env: &std::collections::BTreeMap<String, String>,
        capture: bool,
    ) -> Result<effigy_builtin::HostSuiteOutcome, BuiltinError> {
        let Some(run) = self.run.as_mut() else {
            return Err(BuiltinError::task_invocation(
                "the suite's run was already settled",
            ));
        };
        let _scope = run.enter();
        let exit = crate::runner::task_engine::run_plain(
            run,
            crate::runner::task_engine::PlainCommand {
                command,
                cwd,
                env: env
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.clone(),
                            crate::runner::task_engine::EnvOverride::Plain(value.clone()),
                        )
                    })
                    .collect(),
                capture,
                timeout_ms: None,
            },
        )
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
        let stopped = exit.stopped;
        self.attempt = Some(exit.workload);
        Ok(effigy_builtin::HostSuiteOutcome {
            code: Some(exit.code),
            stopped,
        })
    }

    fn settle(mut self: Box<Self>, outcome: &effigy_builtin::HostSuiteOutcome) {
        let Some(run) = self.run.take() else {
            return;
        };
        // The run records what the runner itself observed for the attempt:
        // a cancellation stays a cancellation, and a failed suite keeps its
        // own code.
        let workload = self.attempt.unwrap_or(match outcome.code {
            Some(0) => effigy_kernel::outcome::WorkloadOutcome::Succeeded,
            Some(code) => effigy_kernel::outcome::WorkloadOutcome::Failed { code },
            None => effigy_kernel::outcome::WorkloadOutcome::Failed { code: 1 },
        });
        for warning in run.close(workload) {
            eprintln!("warning: {}: {warning}", self.label);
        }
    }
}

impl BuiltinRuntimePorts for RunnerBuiltinPorts<'_> {
    fn host_scratch_env(&self) -> Vec<(String, std::ffi::OsString)> {
        crate::runner::scratch::temp_env()
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.into_os_string()))
            .collect()
    }

    fn custody_holds_host_suites(&self) -> bool {
        self.suite_parent.is_some()
    }

    fn host_suite_env_aliases(
        &self,
        root: &Path,
    ) -> Result<Vec<effigy_builtin::BuiltinHostAlias>, BuiltinError> {
        crate::runner::execute::host_alias::host_env_aliases(root)
            .map_err(|error| BuiltinError::task_invocation(error.to_string()))
    }

    fn resolve_host_suite_inherited_env(
        &self,
        root: &Path,
        label: &str,
        delivered: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<(String, String)>, BuiltinError> {
        crate::runner::execute::host_alias::resolve_host_env_inherited(root, label, delivered)
            .map_err(|error| BuiltinError::task_invocation(error.to_string()))
    }

    fn prove_host_suite_aliases(
        &self,
        root: &Path,
        label: &str,
        texts: &[&str],
    ) -> Result<(), BuiltinError> {
        crate::runner::execute::host_alias::prove_host_env_aliases(root, label, texts)
            .map_err(|error| BuiltinError::task_invocation(error.to_string()))
    }

    fn has_test_tui_source(&self) -> bool {
        self.suite_parent.is_some()
    }

    fn test_tui_source(
        &self,
        specs: Vec<effigy_process::ProcessSpec>,
    ) -> Result<Option<Box<dyn effigy_tui::multiprocess::SessionProcesses + '_>>, BuiltinError>
    {
        self.suite_parent
            .as_ref()
            .map(|parent| test_session::source(parent, specs))
            .transpose()
    }

    fn host_suite_custody(
        &self,
        label: &str,
    ) -> Result<Option<Box<dyn effigy_builtin::HostSuiteCustody>>, BuiltinError> {
        let Some(parent) = self.suite_parent.as_ref() else {
            return Ok(None);
        };
        let index = self.next_suite.get();
        self.next_suite.set(index + 1);
        let run = parent
            .borrow_mut()
            .step(index, label)
            .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
        Ok(Some(Box::new(SuiteCustody {
            run: Some(run),
            label: label.to_owned(),
            attempt: None,
        })))
    }

    fn builtin_test_child_evidence_enabled(&self) -> bool {
        #[cfg(test)]
        {
            crate::runner::owned_children::test_diagnostic_capture_active()
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    fn record_builtin_test_child_evidence(&self, evidence: BuiltinTestChildEvidence) {
        #[cfg(test)]
        crate::runner::owned_children::record_builtin_test_child_evidence(evidence);
        #[cfg(not(test))]
        let _ = evidence;
    }

    fn acquire_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[BuiltinLockScope],
    ) -> Result<BuiltinLockGuards, BuiltinError> {
        let mut scopes = scopes.to_vec();
        scopes.sort();
        scopes.dedup();
        let runs = scopes
            .iter()
            .map(|scope| TaskRun::exclusive(workspace_root, &scope.label(), 0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(runner_to_builtin)?;
        Ok(BuiltinLockGuards::new(runs))
    }

    fn current_working_dir(&self) -> Result<PathBuf, BuiltinError> {
        command_context::current_working_dir().map_err(runner_to_builtin)
    }

    fn run_manifest_task_with_cwd(
        &self,
        task: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, BuiltinError> {
        api::run_manifest_task_with_surface(task, cwd, ExecutionSurface::DirectCli)
            .map_err(runner_to_builtin)
    }

    fn run_doctor(&self, args: DoctorArgs) -> Result<String, BuiltinError> {
        let ports = RunnerDoctorPorts::new();
        effigy_doctor::run_doctor(args, &ports)
            .map_err(|err| runner_to_builtin(RunnerError::from(err)))
    }

    fn run_tasks(&self, args: TasksArgs) -> Result<String, BuiltinError> {
        tasks_command::run_tasks(args).map_err(runner_to_builtin)
    }

    fn prepare_container_suite_target(
        &self,
        target_root: &Path,
        container: &str,
    ) -> Result<BuiltinContainerSuiteTarget, BuiltinError> {
        crate::runner::exec_command::prepare_container_suite_target(target_root, container)
            .map_err(runner_to_builtin)?
            .ok_or_else(|| {
                BuiltinError::task_invocation(
                    "container suite target resolved during planning is unavailable at execution time",
                )
            })
    }

    fn render_container_suite_command(
        &self,
        target: &BuiltinContainerSuiteTarget,
        suite_command: &str,
    ) -> Result<String, BuiltinError> {
        crate::runner::exec_command::render_container_suite_command(target, suite_command)
            .map_err(runner_to_builtin)
    }

    fn run_command(&self, command: Command) -> Result<String, BuiltinError> {
        crate::runner::run_command(command).map_err(runner_to_builtin)
    }

    fn deferred_builtins_from_catalogs(
        &self,
        catalogs: &[LoadedCatalog],
        resolved_root: &Path,
    ) -> BTreeSet<String> {
        super::deferred_builtins_from_catalogs(catalogs, resolved_root)
    }
}

/// Translate `RunnerError` variants surfaced by the runner task engine /
/// execute modules into `BuiltinError`. Only the shapes that
/// can actually be produced by the port surface are mirrored; anything
/// else collapses to `BuiltinError::TaskInvocation` via `Display`.
pub(in crate::runner) fn runner_to_builtin(error: RunnerError) -> BuiltinError {
    match error {
        RunnerError::TaskInvocation(message) => BuiltinError::TaskInvocation(message),
        RunnerError::Ui(message) => BuiltinError::Ui(message),
        RunnerError::TaskLockConflict(details) => BuiltinError::TaskLockConflict(details),
        RunnerError::TaskCommandLaunch { command, error } => {
            BuiltinError::TaskCommandLaunch { command, error }
        }
        RunnerError::TaskManifestCompose { path, detail } => {
            BuiltinError::TaskManifestCompose { path, detail }
        }
        RunnerError::DoctorNonZero {
            error_count,
            rendered,
        } => BuiltinError::DoctorNonZero {
            error_count,
            rendered,
        },
        RunnerError::CommandJsonFailure { rendered } => {
            BuiltinError::CommandJsonFailure { rendered }
        }
        other => BuiltinError::TaskInvocation(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use effigy_builtin::BuiltinRuntimePorts;

    #[test]
    fn host_scratch_env_follows_the_runs_scratch_scope() {
        let scratch = tempfile::tempdir().expect("scratch");
        let dir = scratch.path().join("scratch-dir");
        std::fs::create_dir(&dir).expect("dir");

        assert!(
            super::RunnerBuiltinPorts::new()
                .host_scratch_env()
                .is_empty(),
            "a caller with no run injects no temp variables"
        );

        crate::runner::scratch::with_test_scope(&dir, || {
            let env = super::RunnerBuiltinPorts::new().host_scratch_env();
            let keys = env.iter().map(|(key, _)| key.as_str()).collect::<Vec<_>>();
            assert_eq!(keys, ["TMPDIR", "TMP", "TEMP"]);
            for (_, value) in env {
                assert_eq!(std::path::Path::new(&value), dir);
            }
        });
    }
}
