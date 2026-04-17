#[path = "prelude/case_tables/mod.rs"]
mod case_tables;
#[path = "prelude/error_assertions.rs"]
mod error_assertions;
#[path = "prelude/fixtures.rs"]
mod fixtures;
#[path = "prelude/json_assertions.rs"]
mod json_assertions;
#[path = "prelude/output_assertions.rs"]
mod output_assertions;

pub(super) mod runtime {
    pub(in crate::runner::tests) use super::super::super::error::RunnerError;
    pub(in crate::runner::tests) use effigy_cli::{DoctorArgs, TaskInvocation, TasksArgs};
    pub(in crate::runner::tests) use effigy_tasks::TaskRuntimeArgs;
    pub(in crate::runner::tests) use std::fs;
    #[cfg(unix)]
    pub(in crate::runner::tests) use std::os::unix::fs::symlink;
    pub(in crate::runner::tests) use std::path::{Path, PathBuf};
    pub(in crate::runner::tests) use std::thread;
    pub(in crate::runner::tests) use std::time::{Duration, Instant};

    pub(in crate::runner::tests) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
        let ports = super::super::super::doctor_ports::RunnerDoctorPorts::new();
        effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
    }

    pub(in crate::runner::tests) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
        super::super::super::tasks_command::run_tasks(args)
    }
}

pub(super) mod catalog {
    pub(in crate::runner::tests) use super::super::super::test_support::catalog::{
        builtin_test_max_parallel, discover_catalogs,
    };
}

pub(super) mod parsing {
    pub(in crate::runner::tests) use super::super::super::test_support::parsing::{
        parse_task_runtime_args, parse_task_selector,
    };
}

pub(super) mod builtin_contracts {
    pub(in crate::runner::tests) use super::super::super::test_support::builtin_contracts::{
        parse_completion_contract_request, parse_config_contract_request,
        parse_unlock_contract_request, parse_watch_contract_request, CompletionParseContract,
        ConfigParseContract,
    };
}

pub(super) mod execution {
    pub(in crate::runner::tests) use super::super::super::test_support::execution::run_manifest_task_with_cwd;
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

pub(in crate::runner::tests) use cases::*;
pub(in crate::runner::tests) use errors::*;
pub(in crate::runner::tests) use fixture_support::*;
pub(in crate::runner::tests) use harness::*;
pub(in crate::runner::tests) use output::*;
pub(in crate::runner::tests) use runtime::*;
