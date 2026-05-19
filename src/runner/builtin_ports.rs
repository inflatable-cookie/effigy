//! Concrete [`BuiltinRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-builtin`. This module threads the
//! built-in layer's reach-backs into the runner's `locking`, `cache`,
//! `doctor`, `execute`, `command_context`, and `tasks_command`
//! modules, converting `RunnerError` to `BuiltinError` at the port
//! boundary so the built-in layer only speaks its own error type.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_builtin::{
    BuiltinError, BuiltinLockGuards, BuiltinRuntimePorts, LockScope as BuiltinLockScope,
    TaskCacheEntry, UnlockResult,
};
use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
use effigy_execution::ExecutionSurface;
use effigy_manifest::LoadedCatalog;

use crate::runner::cache::ops;
use crate::runner::command_context;
use crate::runner::doctor_ports::RunnerDoctorPorts;
use crate::runner::error::RunnerError;
use crate::runner::execute::api;
use crate::runner::locking::io as locking_io;
use crate::runner::locking::io::LockGuard;
use crate::runner::tasks_command;

/// Concrete `BuiltinRuntimePorts` implementation that delegates to the
/// runner modules directly.
#[derive(Debug, Default)]
pub(in crate::runner) struct RunnerBuiltinPorts;

impl RunnerBuiltinPorts {
    pub(in crate::runner) fn new() -> Self {
        Self
    }
}

impl BuiltinRuntimePorts for RunnerBuiltinPorts {
    fn acquire_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[BuiltinLockScope],
    ) -> Result<BuiltinLockGuards, BuiltinError> {
        let guards: Vec<LockGuard> =
            locking_io::acquire_scopes(workspace_root, scopes).map_err(runner_to_builtin)?;
        Ok(BuiltinLockGuards::new(guards))
    }

    fn unlock_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[BuiltinLockScope],
    ) -> Result<UnlockResult, BuiltinError> {
        locking_io::unlock_scopes(workspace_root, scopes).map_err(runner_to_builtin)
    }

    fn unlock_all(&self, workspace_root: &Path) -> Result<UnlockResult, BuiltinError> {
        locking_io::unlock_all(workspace_root).map_err(runner_to_builtin)
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

    fn run_command(&self, command: Command) -> Result<String, BuiltinError> {
        crate::runner::run_command(command).map_err(runner_to_builtin)
    }

    fn cache_entries(&self, workspace_root: &Path) -> Result<Vec<TaskCacheEntry>, BuiltinError> {
        ops::cache_entries(workspace_root).map_err(runner_to_builtin)
    }

    fn cache_entry(
        &self,
        workspace_root: &Path,
        manifest_path: &Path,
        task_name: &str,
    ) -> Result<Option<TaskCacheEntry>, BuiltinError> {
        ops::cache_entry(workspace_root, manifest_path, task_name).map_err(runner_to_builtin)
    }

    fn cache_entry_key(&self, manifest_path: &Path, task_name: &str) -> String {
        ops::cache_entry_key(manifest_path, task_name)
    }

    fn invalidate_cache_keys(
        &self,
        workspace_root: &Path,
        keys: &[String],
    ) -> Result<Vec<String>, BuiltinError> {
        ops::invalidate_cache_keys(workspace_root, keys).map_err(runner_to_builtin)
    }

    fn invalidate_all_cache_entries(&self, workspace_root: &Path) -> Result<usize, BuiltinError> {
        ops::invalidate_all_cache_entries(workspace_root).map_err(runner_to_builtin)
    }

    fn deferred_builtins_from_catalogs(
        &self,
        catalogs: &[LoadedCatalog],
        resolved_root: &Path,
    ) -> BTreeSet<String> {
        super::deferred_builtins_from_catalogs(catalogs, resolved_root)
    }
}

/// Translate `RunnerError` variants surfaced by the runner locking /
/// cache / execute modules into `BuiltinError`. Only the shapes that
/// can actually be produced by the port surface are mirrored; anything
/// else collapses to `BuiltinError::TaskInvocation` via `Display`.
fn runner_to_builtin(error: RunnerError) -> BuiltinError {
    match error {
        RunnerError::TaskInvocation(message) => BuiltinError::TaskInvocation(message),
        RunnerError::Ui(message) => BuiltinError::Ui(message),
        RunnerError::TaskLockIo { path, error } => BuiltinError::TaskLockIo { path, error },
        RunnerError::TaskLockConflict {
            scope,
            lock_path,
            holder_pid,
            holder_started_at_epoch_ms,
            holder_heartbeat_at_epoch_ms,
            holder_hostname,
            holder_workspace_root,
            remediation,
        } => BuiltinError::TaskLockConflict {
            scope,
            lock_path,
            holder_pid,
            holder_started_at_epoch_ms,
            holder_heartbeat_at_epoch_ms,
            holder_hostname,
            holder_workspace_root,
            remediation,
        },
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
        other => BuiltinError::TaskInvocation(other.to_string()),
    }
}
