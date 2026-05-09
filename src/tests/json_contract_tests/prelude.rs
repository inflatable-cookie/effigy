// Flat prelude surface for json_contract_tests. Every helper the tests need
// is re-exported at this module's root. Facade submodules (`runtime`,
// `harness`, `execution`, `json`) are preserved for test sites that still
// glob-import `{runtime::*, harness::*, ...}`.

pub(crate) mod runtime {
    pub(crate) use crate::runner::error::RunnerError;
    pub(crate) use effigy_cli::{DoctorArgs, TaskInvocation, TasksArgs};
    pub(crate) use std::fs;
    #[cfg(unix)]
    pub(crate) use std::os::unix::fs::PermissionsExt;
    pub(crate) use std::path::PathBuf;
    pub(crate) use std::thread;
    pub(crate) use std::time::Duration;

    pub(crate) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
        let ports = crate::runner::doctor_ports::RunnerDoctorPorts::new();
        effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
    }

    pub(crate) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
        crate::runner::tasks_command::run_tasks(args)
    }
}

pub(crate) mod harness {
    pub(crate) use crate::contract_test_support::{
        lock_test, parse_json, temp_workspace, with_cwd, write_manifest, EnvGuard,
    };
}

pub(crate) mod execution {
    use super::runtime::{PathBuf, RunnerError, TaskInvocation};
    use effigy_cli::Command;

    pub(crate) fn run_manifest_task_with_cwd(
        invocation: &TaskInvocation,
        root: PathBuf,
    ) -> Result<String, RunnerError> {
        crate::runner::execute::api::run_manifest_task_with_cwd(invocation, root)
    }

    pub(crate) use super::run_invocation_json;

    pub(crate) fn run_command(command: Command) -> Result<String, RunnerError> {
        crate::runner::run_command(command)
    }
}

pub(crate) mod json {
    pub(crate) use super::assert_schema_v1;
}

use execution::run_manifest_task_with_cwd;
use harness::parse_json;
use runtime::{PathBuf, TaskInvocation};

pub(crate) fn assert_schema_v1(parsed: &serde_json::Value, schema: &str) {
    assert_eq!(parsed["schema"], schema);
    assert_eq!(parsed["schema_version"], 1);
}

pub(crate) fn run_invocation_json(root: PathBuf, name: &str, args: &[&str]) -> serde_json::Value {
    let invocation = match name {
        "migrate" | "unlock" | "cache" => TaskInvocation {
            name: "tasks".to_owned(),
            args: std::iter::once(name.to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        "completion" => TaskInvocation {
            name: "config".to_owned(),
            args: std::iter::once("completion".to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        _ => TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
    };
    let out = run_manifest_task_with_cwd(&invocation, root).expect("run invocation");
    parse_json(&out)
}

pub(crate) fn run_completion_candidates_json(root: PathBuf) -> serde_json::Value {
    run_invocation_json(root, "completion", &["candidates", "--json"])
}

pub(crate) fn assert_candidates_cache_policy(
    parsed: &serde_json::Value,
    hit: bool,
    state: &str,
    effective_ttl_ms: i64,
    ttl_source: &str,
) {
    assert_eq!(parsed["cache_hit"], hit);
    assert_eq!(parsed["cache_state"], state);
    assert_eq!(parsed["effective_cache_ttl_ms"], effective_ttl_ms);
    assert_eq!(parsed["cache_ttl_source"], ttl_source);
}

// Absorbed from former completion_contract_tests/prelude.rs.
pub(crate) fn with_completion_cache_default() -> harness::EnvGuard {
    harness::EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)])
}

pub(crate) fn run_completion_task(
    root: PathBuf,
    args: &[&str],
) -> Result<String, runtime::RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: std::iter::once("completion".to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        root,
    )
}

// Flat re-export surface — test sites use the absolute path
// `crate::runner::json_contract_tests::prelude::...`.
pub(crate) use harness::*;
pub(crate) use runtime::*;
