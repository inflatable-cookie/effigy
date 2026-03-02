use super::{
    builtin_test_max_parallel, discover_catalogs, parse_task_runtime_args, parse_task_selector,
    run_doctor, run_manifest_task_with_cwd, run_tasks, RunnerError, TaskRuntimeArgs,
};
use crate::{DoctorArgs, TaskInvocation, TasksArgs};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

fn write_manifest(path: &PathBuf, body: &str) {
    fs::write(path, body).expect("write manifest");
}

fn run_builtin_ok(root: PathBuf, name: &str, args: &[&str]) -> String {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
    .expect("built-in invocation should succeed")
}

fn run_builtin_err(root: PathBuf, name: &str, args: &[&str]) -> RunnerError {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
    .expect_err("built-in invocation should fail")
}

fn assert_contains_all(rendered: &str, expected: &[&str]) {
    for snippet in expected {
        assert!(
            rendered.contains(snippet),
            "expected output to contain {:?}\noutput:\n{}",
            snippet,
            rendered
        );
    }
}

fn assert_task_invocation_error_contains(err: RunnerError, expected: &[&str]) {
    match err {
        RunnerError::TaskInvocation(message) => assert_contains_all(&message, expected),
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_manifest_parse_error_contains_any(error: &toml::de::Error, expected: &[&str]) {
    let rendered = error.to_string();
    assert!(
        expected.iter().any(|pattern| rendered.contains(pattern)),
        "expected parse error to contain one of {:?}, got: {}",
        expected,
        rendered
    );
}

fn parse_json_output(rendered: &str) -> serde_json::Value {
    serde_json::from_str(rendered).expect("parse json")
}

fn run_tasks_from_repo(
    root: &PathBuf,
    task_name: Option<&str>,
    resolve_selector: Option<&str>,
    output_json: bool,
) -> String {
    with_cwd(root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: task_name.map(|value| value.to_owned()),
            resolve_selector: resolve_selector.map(|value| value.to_owned()),
            output_json,
            pretty_json: true,
        })
    })
    .expect("run tasks")
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-runner-{name}-{ts}"))
}

fn temp_workspace(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

fn with_cwd<F, T>(cwd: &PathBuf, f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = lock_test();
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(cwd).expect("set cwd");
    let out = f();
    std::env::set_current_dir(original).expect("restore cwd");
    out
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_test() -> MutexGuard<'static, ()> {
    match test_lock().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

struct EnvGuard {
    original: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn set_many(entries: &[(&str, Option<String>)]) -> Self {
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push(((*key).to_owned(), std::env::var(key).ok()));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.original {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
