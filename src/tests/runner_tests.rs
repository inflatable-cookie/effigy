use super::{
    builtin_test_max_parallel, discover_catalogs, parse_task_runtime_args, parse_task_selector,
    run_doctor, run_manifest_task_with_cwd, run_tasks, RunnerError, TaskRuntimeArgs,
};
use crate::{DoctorArgs, TaskInvocation, TasksArgs};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

#[path = "runner_tests/catalog_discovery_tests.rs"]
mod catalog_discovery_tests;

#[path = "runner_tests/runner_core_tests.rs"]
mod runner_core_tests;

#[path = "runner_tests/run_array_tests.rs"]
mod run_array_tests;

#[path = "runner_tests/tasks_listing_tests.rs"]
mod tasks_listing_tests;

#[path = "runner_tests/builtin_command_tests.rs"]
mod builtin_command_tests;

#[path = "runner_tests/catalogs_builtin_tests.rs"]
mod catalogs_builtin_tests;

#[path = "runner_tests/tasks_and_doctor_command_tests.rs"]
mod tasks_and_doctor_command_tests;

#[path = "runner_tests/config_builtin_tests.rs"]
mod config_builtin_tests;

#[cfg(unix)]
#[path = "runner_tests/doctor_text_output_tests.rs"]
mod doctor_text_output_tests;

#[path = "runner_tests/deferral_tests.rs"]
mod deferral_tests;

#[path = "runner_tests/managed_and_locking_tests.rs"]
mod managed_and_locking_tests;

#[path = "runner_test_support.rs"]
mod runner_test_support;

use runner_test_support::*;
