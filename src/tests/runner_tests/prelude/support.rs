pub(in crate::runner::tests) mod runtime {
    pub(in crate::runner::tests) use crate::contract_test_support::EnvGuard;
    pub(in crate::runner::tests) use crate::runner::error::RunnerError;
    pub(in crate::runner::tests) use effigy_cli::{DoctorArgs, TaskInvocation, TasksArgs};
    pub(in crate::runner::tests) use effigy_tasks::TaskRuntimeArgs;
    pub(in crate::runner::tests) use std::fs;
    #[cfg(unix)]
    pub(in crate::runner::tests) use std::os::unix::fs::symlink;
    pub(in crate::runner::tests) use std::path::{Path, PathBuf};
    pub(in crate::runner::tests) use std::thread;
    pub(in crate::runner::tests) use std::time::{Duration, Instant};

    pub(in crate::runner::tests) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
        let ports = crate::runner::doctor_ports::RunnerDoctorPorts::new();
        effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
    }

    pub(in crate::runner::tests) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
        crate::runner::tasks_command::run_tasks(args)
    }
}

pub(in crate::runner::tests) mod catalog {
    pub(in crate::runner::tests) use effigy_builtin::test_support::builtin_test_max_parallel;
    pub(in crate::runner::tests) use effigy_routing::discover_catalogs;
}

pub(in crate::runner::tests) mod parsing {
    pub(in crate::runner::tests) use crate::runner::util::parse_task_runtime_args;
    pub(in crate::runner::tests) use effigy_tasks::parse_task_selector;
}

pub(in crate::runner::tests) mod builtin_contracts {
    pub(in crate::runner::tests) use effigy_builtin::test_support::{
        parse_completion_contract_request, parse_config_contract_request,
        parse_unlock_contract_request, parse_watch_contract_request, CompletionParseContract,
        ConfigParseContract,
    };
}

pub(in crate::runner::tests) mod execution {
    pub(in crate::runner::tests) use crate::runner::execute::run_manifest_task_with_cwd;
}

pub(in crate::runner::tests) mod harness {
    pub(in crate::runner::tests) use super::super::super::runner_test_support::assertions::*;
    pub(in crate::runner::tests) use super::super::super::runner_test_support::builtin::*;
    pub(in crate::runner::tests) use super::super::super::runner_test_support::env::*;
    pub(in crate::runner::tests) use super::super::super::runner_test_support::json::*;
    pub(in crate::runner::tests) use super::super::super::runner_test_support::workspace::*;
}

pub(in crate::runner::tests) mod harness_assertions {
    pub(in crate::runner::tests) use super::super::super::runner_test_support::assertions::*;
}

pub(in crate::runner::tests) mod harness_builtin {
    pub(in crate::runner::tests) use super::super::super::runner_test_support::builtin::*;
}

pub(in crate::runner::tests) mod harness_env {
    pub(in crate::runner::tests) use super::super::super::runner_test_support::env::*;
}

pub(in crate::runner::tests) mod harness_tasks {
    pub(in crate::runner::tests) use super::super::super::runner_test_support::tasks::*;
}

pub(in crate::runner::tests) mod harness_workspace {
    pub(in crate::runner::tests) use super::super::super::runner_test_support::workspace::*;
}

pub(in crate::runner::tests) mod cases {
    pub(in crate::runner::tests) use super::super::case_tables::*;
}

pub(in crate::runner::tests) mod errors {
    pub(in crate::runner::tests) use super::super::super::runner_test_support::assertions::assert_task_command_failure_code;
    pub(in crate::runner::tests) use super::super::error_assertions::*;
}

pub(in crate::runner::tests) mod fixture_support {
    pub(in crate::runner::tests) use super::super::fixtures::*;
}

pub(in crate::runner::tests) mod json {
    pub(in crate::runner::tests) use super::super::json_assertions::*;
}

pub(in crate::runner::tests) mod output {
    pub(in crate::runner::tests) use super::super::output_assertions::*;
}
