pub(super) use super::super::{
    contract_test_support::{
        parse_json, temp_workspace, test_lock, with_cwd, write_manifest, EnvGuard,
    },
    run_doctor, run_manifest_task_with_cwd, run_tasks, DoctorArgs, RunnerError, TasksArgs,
};
pub(super) use crate::TaskInvocation;
pub(super) use std::fs;
#[cfg(unix)]
pub(super) use std::os::unix::fs::PermissionsExt;
pub(super) use std::path::PathBuf;
pub(super) use std::thread;
pub(super) use std::time::Duration;

pub(super) fn assert_schema_v1(parsed: &serde_json::Value, schema: &str) {
    assert_eq!(parsed["schema"], schema);
    assert_eq!(parsed["schema_version"], 1);
}

pub(super) fn run_invocation_json(root: PathBuf, name: &str, args: &[&str]) -> serde_json::Value {
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

pub(super) fn run_completion_candidates_json(root: PathBuf) -> serde_json::Value {
    run_invocation_json(root, "completion", &["candidates", "--json"])
}

pub(super) fn assert_candidates_cache_policy(
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
