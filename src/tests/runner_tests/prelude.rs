#[path = "prelude/builtin_command.rs"]
mod builtin_command;
#[path = "prelude/builtin_contract.rs"]
mod builtin_contract;
#[path = "prelude/builtin_help.rs"]
mod builtin_help;
#[path = "prelude/case_tables/mod.rs"]
mod case_tables;
#[path = "prelude/catalog_discovery.rs"]
mod catalog_discovery;
#[path = "prelude/deferral.rs"]
mod deferral;
#[path = "prelude/error_assertions.rs"]
mod error_assertions;
#[path = "prelude/fixtures.rs"]
mod fixtures;
#[path = "prelude/init_migrate.rs"]
mod init_migrate;
#[path = "prelude/json_assertions.rs"]
mod json_assertions;
#[path = "prelude/managed.rs"]
mod managed;
#[path = "prelude/output_assertions.rs"]
mod output_assertions;
#[path = "prelude/parsing_resolution.rs"]
mod parsing_resolution;
#[path = "prelude/run_array.rs"]
mod run_array;
#[path = "prelude/run_array_execution.rs"]
mod run_array_execution;
#[path = "prelude/tasks_listing.rs"]
mod tasks_listing;
#[path = "prelude/watch.rs"]
mod watch;

// ---------------------------------------------------------------------------
// Internal facade modules
//
// These namespaces exist to let helper modules inside prelude/ reach a stable
// set of imports (`super::runtime::Path`, `super::harness::EnvGuard`, etc.).
// They are NOT a re-export chain across prelude files — they are local
// organisation inside a single prelude surface.
// ---------------------------------------------------------------------------

pub(super) mod runtime {
    pub(in crate::runner::tests) use crate::runner::error::RunnerError;
    pub(in crate::runner::tests) use effigy_cli::{DoctorArgs, TaskInvocation, TasksArgs};
    pub(in crate::runner::tests) use effigy_tasks::TaskRuntimeArgs;
    pub(in crate::runner::tests) use std::fs;
    #[cfg(unix)]
    pub(in crate::runner::tests) use std::os::unix::fs::symlink;
    pub(in crate::runner::tests) use std::path::{Path, PathBuf};
    pub(in crate::runner::tests) use std::thread;
    pub(in crate::runner::tests) use std::time::{Duration, Instant};

    pub(in crate::runner::tests) use crate::contract_test_support::EnvGuard;

    pub(in crate::runner::tests) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
        let ports = crate::runner::doctor_ports::RunnerDoctorPorts::new();
        effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
    }

    pub(in crate::runner::tests) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
        crate::runner::tasks_command::run_tasks(args)
    }
}

pub(super) mod catalog {
    pub(in crate::runner::tests) use effigy_builtin::test_support::builtin_test_max_parallel;
    pub(in crate::runner::tests) use effigy_routing::discover_catalogs;
}

pub(super) mod parsing {
    pub(in crate::runner::tests) use crate::runner::util::parse_task_runtime_args;
    pub(in crate::runner::tests) use effigy_tasks::parse_task_selector;
}

pub(super) mod builtin_contracts {
    pub(in crate::runner::tests) use effigy_builtin::test_support::{
        parse_completion_contract_request, parse_config_contract_request,
        parse_unlock_contract_request, parse_watch_contract_request, CompletionParseContract,
        ConfigParseContract,
    };
}

pub(super) mod execution {
    pub(in crate::runner::tests) use crate::runner::execute::run_manifest_task_with_cwd;
}

pub(super) mod harness {
    pub(in crate::runner::tests) use super::super::runner_test_support::assertions::*;
    pub(in crate::runner::tests) use super::super::runner_test_support::builtin::*;
    pub(in crate::runner::tests) use super::super::runner_test_support::env::*;
    pub(in crate::runner::tests) use super::super::runner_test_support::json::*;
    pub(in crate::runner::tests) use super::super::runner_test_support::workspace::*;
}

pub(super) mod harness_assertions {
    pub(in crate::runner::tests) use super::super::runner_test_support::assertions::*;
}

pub(super) mod harness_builtin {
    pub(in crate::runner::tests) use super::super::runner_test_support::builtin::*;
}

pub(super) mod harness_env {
    pub(in crate::runner::tests) use super::super::runner_test_support::env::*;
}

pub(super) mod harness_tasks {
    pub(in crate::runner::tests) use super::super::runner_test_support::tasks::*;
}

pub(super) mod harness_workspace {
    pub(in crate::runner::tests) use super::super::runner_test_support::workspace::*;
}

pub(super) mod cases {
    pub(in crate::runner::tests) use super::case_tables::*;
}

pub(super) mod errors {
    pub(in crate::runner::tests) use super::super::runner_test_support::assertions::assert_task_command_failure_code;
    pub(in crate::runner::tests) use super::error_assertions::*;
}

pub(super) mod fixture_support {
    pub(in crate::runner::tests) use super::fixtures::*;
}

pub(super) mod json {
    pub(in crate::runner::tests) use super::json_assertions::*;
}

pub(super) mod output {
    pub(in crate::runner::tests) use super::output_assertions::*;
}

// ---------------------------------------------------------------------------
// Flat re-export surface
//
// All names below are the single prelude that test files import via
// `use crate::runner::tests::prelude::...`. No test-side prelude chain lives
// above this file; every nested `prelude.rs` has been removed.
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) use builtin_command::*;
pub(in crate::runner::tests) use builtin_contract::*;
pub(in crate::runner::tests) use builtin_contracts::*;
pub(in crate::runner::tests) use builtin_help::*;
pub(in crate::runner::tests) use case_tables::*;
pub(in crate::runner::tests) use catalog::*;
pub(in crate::runner::tests) use catalog_discovery::*;
pub(in crate::runner::tests) use deferral::*;
pub(in crate::runner::tests) use error_assertions::*;
pub(in crate::runner::tests) use fixtures::*;
pub(in crate::runner::tests) use init_migrate::*;
pub(in crate::runner::tests) use json_assertions::*;
pub(in crate::runner::tests) use managed::*;
pub(in crate::runner::tests) use output_assertions::*;
pub(in crate::runner::tests) use parsing::*;
pub(in crate::runner::tests) use parsing_resolution::*;
pub(in crate::runner::tests) use run_array::*;
pub(in crate::runner::tests) use run_array_execution::*;
pub(in crate::runner::tests) use runtime::*;
pub(in crate::runner::tests) use tasks_listing::*;
pub(in crate::runner::tests) use watch::*;

// Harness surface — the thin runner_test_support re-exports. Exposed flat so
// tests can reach `run_builtin_ok`, `temp_workspace`, `write_manifest`, etc.
// without walking through an intermediate module.
pub(in crate::runner::tests) use super::runner_test_support::assertions::*;
pub(in crate::runner::tests) use super::runner_test_support::builtin::*;
pub(in crate::runner::tests) use super::runner_test_support::env::*;
pub(in crate::runner::tests) use super::runner_test_support::tasks::*;
pub(in crate::runner::tests) use super::runner_test_support::workspace::*;
