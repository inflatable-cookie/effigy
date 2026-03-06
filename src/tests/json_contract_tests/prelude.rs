pub(super) mod runtime {
    pub(crate) use super::super::super::{run_doctor, run_tasks, RunnerError};
    pub(crate) use crate::{DoctorArgs, TaskInvocation, TasksArgs};
    pub(crate) use std::fs;
    #[cfg(unix)]
    pub(crate) use std::os::unix::fs::PermissionsExt;
    pub(crate) use std::path::PathBuf;
    pub(crate) use std::thread;
    pub(crate) use std::time::Duration;
}

pub(super) mod harness {
    pub(crate) use crate::contract_test_support::{
        parse_json, temp_workspace, test_lock, with_cwd, write_manifest, EnvGuard,
    };
}

pub(super) mod execution {
    pub(crate) use super::{run_completion_candidates_json, run_invocation_json};

    use super::runtime::{PathBuf, RunnerError, TaskInvocation};

    pub(crate) fn run_manifest_task_with_cwd(
        invocation: &TaskInvocation,
        root: PathBuf,
    ) -> Result<String, RunnerError> {
        super::super::super::test_support::execution::run_manifest_task_with_cwd(invocation, root)
    }
}

pub(super) mod json {
    pub(crate) use super::{assert_candidates_cache_policy, assert_schema_v1};
}

use execution::run_manifest_task_with_cwd;
use harness::parse_json;
use runtime::{PathBuf, TaskInvocation};

pub(crate) fn assert_schema_v1(parsed: &serde_json::Value, schema: &str) {
    assert_eq!(parsed["schema"], schema);
    assert_eq!(parsed["schema_version"], 1);
}

pub(crate) fn run_invocation_json(root: PathBuf, name: &str, args: &[&str]) -> serde_json::Value {
    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
    .expect("run invocation");
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
