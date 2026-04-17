//! Runtime ports the built-in command layer uses to reach back into the
//! runner.
//!
//! Card 251 inverts direct `super::super::super::{locking,cache,...}`
//! reach-ins behind this trait so that the built-in layer depends on an
//! abstract contract instead of sibling runner modules. Card 250 then moves
//! `src/runner/builtin/` into its own crate and the only remaining runner
//! dependency is the port trait + a handful of shared types.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{DoctorArgs, TaskInvocation, TasksArgs};
use effigy_manifest::LoadedCatalog;

use crate::runner::cache::model::TaskCacheEntry;
use crate::runner::cache::ops;
use crate::runner::command_context;
use crate::runner::doctor;
use crate::runner::error::RunnerError;
use crate::runner::execute;
use crate::runner::locking::io as locking_io;
use crate::runner::locking::io::{LockGuard, UnlockResult};
use crate::runner::locking::model::LockScope;
use crate::runner::tasks_command;

/// Runtime services the built-in command layer depends on. Every reach-back
/// from the built-in layer into the rest of the runner should go through this
/// trait so the built-in layer can be extracted into its own crate without
/// pulling sibling runner modules along.
pub(in crate::runner) trait BuiltinRuntimePorts {
    // Locking.
    fn acquire_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[LockScope],
    ) -> Result<Vec<LockGuard>, RunnerError>;

    fn unlock_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[LockScope],
    ) -> Result<UnlockResult, RunnerError>;

    fn unlock_all(&self, workspace_root: &Path) -> Result<UnlockResult, RunnerError>;

    // Command context.
    fn current_working_dir(&self) -> Result<PathBuf, RunnerError>;

    // Execute / nested command entry points.
    fn run_manifest_task_with_cwd(
        &self,
        task: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, RunnerError>;

    fn run_doctor(&self, args: DoctorArgs) -> Result<String, RunnerError>;

    fn run_tasks(&self, args: TasksArgs) -> Result<String, RunnerError>;

    // Cache inspection / invalidation.
    fn cache_entries(&self, workspace_root: &Path) -> Result<Vec<TaskCacheEntry>, RunnerError>;

    fn cache_entry(
        &self,
        workspace_root: &Path,
        manifest_path: &Path,
        task_name: &str,
    ) -> Result<Option<TaskCacheEntry>, RunnerError>;

    fn cache_entry_key(&self, manifest_path: &Path, task_name: &str) -> String;

    fn invalidate_cache_keys(
        &self,
        workspace_root: &Path,
        keys: &[String],
    ) -> Result<Vec<String>, RunnerError>;

    fn invalidate_all_cache_entries(&self, workspace_root: &Path) -> Result<usize, RunnerError>;

    // Deferred builtin introspection.
    fn deferred_builtins_from_catalogs(
        &self,
        catalogs: &[LoadedCatalog],
        resolved_root: &Path,
    ) -> BTreeSet<String>;
}

/// Concrete `BuiltinRuntimePorts` implementation that delegates to the runner
/// modules directly.
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
        scopes: &[LockScope],
    ) -> Result<Vec<LockGuard>, RunnerError> {
        locking_io::acquire_scopes(workspace_root, scopes)
    }

    fn unlock_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[LockScope],
    ) -> Result<UnlockResult, RunnerError> {
        locking_io::unlock_scopes(workspace_root, scopes)
    }

    fn unlock_all(&self, workspace_root: &Path) -> Result<UnlockResult, RunnerError> {
        locking_io::unlock_all(workspace_root)
    }

    fn current_working_dir(&self) -> Result<PathBuf, RunnerError> {
        command_context::current_working_dir()
    }

    fn run_manifest_task_with_cwd(
        &self,
        task: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, RunnerError> {
        execute::run_manifest_task_with_cwd(task, cwd)
    }

    fn run_doctor(&self, args: DoctorArgs) -> Result<String, RunnerError> {
        doctor::run_doctor(args)
    }

    fn run_tasks(&self, args: TasksArgs) -> Result<String, RunnerError> {
        tasks_command::run_tasks(args)
    }

    fn cache_entries(&self, workspace_root: &Path) -> Result<Vec<TaskCacheEntry>, RunnerError> {
        ops::cache_entries(workspace_root)
    }

    fn cache_entry(
        &self,
        workspace_root: &Path,
        manifest_path: &Path,
        task_name: &str,
    ) -> Result<Option<TaskCacheEntry>, RunnerError> {
        ops::cache_entry(workspace_root, manifest_path, task_name)
    }

    fn cache_entry_key(&self, manifest_path: &Path, task_name: &str) -> String {
        ops::cache_entry_key(manifest_path, task_name)
    }

    fn invalidate_cache_keys(
        &self,
        workspace_root: &Path,
        keys: &[String],
    ) -> Result<Vec<String>, RunnerError> {
        ops::invalidate_cache_keys(workspace_root, keys)
    }

    fn invalidate_all_cache_entries(&self, workspace_root: &Path) -> Result<usize, RunnerError> {
        ops::invalidate_all_cache_entries(workspace_root)
    }

    fn deferred_builtins_from_catalogs(
        &self,
        catalogs: &[LoadedCatalog],
        resolved_root: &Path,
    ) -> BTreeSet<String> {
        super::deferred_builtins_from_catalogs(catalogs, resolved_root)
    }
}
