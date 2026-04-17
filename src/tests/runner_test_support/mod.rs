use super::super::error::RunnerError;
use super::super::execute::run_manifest_task_with_cwd;
use super::super::tasks_command::run_tasks;
use effigy_cli::{TaskInvocation, TasksArgs};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::contract_test_support::write_manifest as write_manifest_shared;

pub(crate) mod assertions;
pub(crate) mod builtin;
pub(crate) mod env;
pub(crate) mod json;
pub(crate) mod tasks;
pub(crate) mod workspace;
