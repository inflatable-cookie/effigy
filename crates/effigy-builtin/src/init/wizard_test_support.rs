#![cfg(test)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
use effigy_manifest::LoadedCatalog;

use crate::{
    BuiltinError, BuiltinLockGuards, BuiltinRuntimePorts, LockScope, TaskCacheEntry, UnlockResult,
};

#[derive(Debug, Default)]
pub(crate) struct WizardTestPorts;

impl BuiltinRuntimePorts for WizardTestPorts {
    fn acquire_scopes(
        &self,
        _workspace_root: &Path,
        _scopes: &[LockScope],
    ) -> Result<BuiltinLockGuards, BuiltinError> {
        Ok(BuiltinLockGuards::new(()))
    }

    fn unlock_scopes(
        &self,
        _workspace_root: &Path,
        _scopes: &[LockScope],
    ) -> Result<UnlockResult, BuiltinError> {
        Ok(UnlockResult {
            removed: Vec::new(),
            missing: Vec::new(),
        })
    }

    fn unlock_all(&self, _workspace_root: &Path) -> Result<UnlockResult, BuiltinError> {
        Ok(UnlockResult {
            removed: Vec::new(),
            missing: Vec::new(),
        })
    }

    fn current_working_dir(&self) -> Result<PathBuf, BuiltinError> {
        std::env::current_dir()
            .map_err(|error| BuiltinError::task_invocation(format!("failed to read cwd: {error}")))
    }

    fn run_manifest_task_with_cwd(
        &self,
        task: &TaskInvocation,
        _cwd: PathBuf,
    ) -> Result<String, BuiltinError> {
        Ok(format!("ran task {}", task.name))
    }

    fn run_doctor(&self, _args: DoctorArgs) -> Result<String, BuiltinError> {
        Ok("doctor ok".to_owned())
    }

    fn run_tasks(&self, _args: TasksArgs) -> Result<String, BuiltinError> {
        Ok("tasks ok".to_owned())
    }

    fn run_command(&self, command: Command) -> Result<String, BuiltinError> {
        Ok(format!("ran command {command:?}"))
    }

    fn cache_entries(&self, _workspace_root: &Path) -> Result<Vec<TaskCacheEntry>, BuiltinError> {
        Ok(Vec::new())
    }

    fn cache_entry(
        &self,
        _workspace_root: &Path,
        _manifest_path: &Path,
        _task_name: &str,
    ) -> Result<Option<TaskCacheEntry>, BuiltinError> {
        Ok(None)
    }

    fn cache_entry_key(&self, manifest_path: &Path, task_name: &str) -> String {
        format!("{}::{task_name}", manifest_path.display())
    }

    fn invalidate_cache_keys(
        &self,
        _workspace_root: &Path,
        _keys: &[String],
    ) -> Result<Vec<String>, BuiltinError> {
        Ok(Vec::new())
    }

    fn invalidate_all_cache_entries(&self, _workspace_root: &Path) -> Result<usize, BuiltinError> {
        Ok(0)
    }

    fn deferred_builtins_from_catalogs(
        &self,
        _catalogs: &[LoadedCatalog],
        _resolved_root: &Path,
    ) -> BTreeSet<String> {
        BTreeSet::new()
    }
}
