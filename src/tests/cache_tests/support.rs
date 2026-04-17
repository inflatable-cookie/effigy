use super::super::test_support::execution::run_manifest_task_with_cwd;
use crate::contract_test_support::{temp_workspace, write_manifest, EnvGuard};
use crate::runner::error::RunnerError;
use effigy_cli::TaskInvocation;
use std::fs;
use std::path::{Path, PathBuf};

fn task_invocation(name: &str, args: &[&str]) -> TaskInvocation {
    TaskInvocation {
        name: name.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

pub(super) fn cache_workspace(name: &str) -> PathBuf {
    temp_workspace(name)
}

pub(super) fn cache_env(token: &str) -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_CACHE_TEST_TOKEN", Some(token.to_owned()))])
}

pub(super) fn write_cache_input(root: &Path, body: &str) {
    fs::write(root.join("input.txt"), body).expect("write input");
}

pub(super) fn read_marker(marker: &Path) -> String {
    fs::read_to_string(marker).expect("read marker")
}

pub(super) fn run_task(root: &Path, name: &str) -> String {
    run_manifest_task_with_cwd(&task_invocation(name, &[]), root.to_path_buf()).expect("task run")
}

pub(super) fn run_cache_builtin(root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(&task_invocation("cache", args), root.to_path_buf())
}

pub(super) fn assert_cache_task_invocation(err: RunnerError, expected: &str) {
    match err {
        RunnerError::TaskInvocation(message) => assert_eq!(message, expected),
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn write_cached_manifest(root: &Path, marker: &Path, marker_write: &str) {
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.build]\nrun = \"sh -lc 'mkdir -p out; {marker_write} >> \\\"{}\\\"; cp input.txt out/result.txt'\"\n\n[tasks.build.cache]\nenabled = true\ninputs = [\"input.txt\"]\noutputs = [\"out/result.txt\"]\nenv = [\"EFFIGY_CACHE_TEST_TOKEN\"]\n",
            marker.display()
        ),
    );
}

pub(super) fn write_non_cached_manifest(root: &Path, marker: &Path) {
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.build]\nrun = \"sh -lc 'printf run >> \\\"{}\\\"'\"\n",
            marker.display()
        ),
    );
}
