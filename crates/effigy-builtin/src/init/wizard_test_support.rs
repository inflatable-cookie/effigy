#![cfg(test)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
use effigy_manifest::LoadedCatalog;

use crate::{BuiltinError, BuiltinLockGuards, BuiltinRuntimePorts, LockScope};

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

    fn deferred_builtins_from_catalogs(
        &self,
        _catalogs: &[LoadedCatalog],
        _resolved_root: &Path,
    ) -> BTreeSet<String> {
        BTreeSet::new()
    }
}
