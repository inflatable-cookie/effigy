use super::{
    builtin_test_max_parallel,
    contract_test_support::{
        lock_test, temp_workspace, with_cwd, write_manifest as write_manifest_shared, EnvGuard,
    },
    discover_catalogs, parse_task_runtime_args, parse_task_selector, run_doctor,
    run_manifest_task_with_cwd, run_tasks, RunnerError, TaskRuntimeArgs,
};
use crate::{DoctorArgs, TaskInvocation, TasksArgs};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
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

fn write_manifest(path: &PathBuf, body: &str) {
    write_manifest_shared(path, body);
}

fn write_root_manifest(root: &PathBuf, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

fn create_workspace_dir(root: &PathBuf, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).expect("mkdir workspace dir");
    dir
}

fn write_catalog_tasks(dir: &PathBuf, alias: Option<&str>, tasks: &[(&str, &str)]) {
    let mut manifest = String::new();
    if let Some(alias) = alias {
        manifest.push_str(&format!("[catalog]\nalias = \"{alias}\"\n"));
    }
    for (task, run) in tasks {
        manifest.push_str(&format!("[tasks.{task}]\nrun = \"{run}\"\n"));
    }
    write_manifest(&dir.join("effigy.toml"), &manifest);
}

fn write_executable(path: &PathBuf, script: &str) {
    fs::write(path, script).expect("write executable");
    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod");
}

fn write_package_json_with_test_script(root: &PathBuf) {
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
}

fn write_package_json_with_vitest_dev_dependency(root: &PathBuf) {
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");
}

fn write_multi_suite_cargo_manifest(root: &PathBuf) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
}

fn setup_multi_suite_repo(root: &PathBuf) {
    write_package_json_with_test_script(root);
    write_multi_suite_cargo_manifest(root);
}

fn write_test_suites_manifest(root: &PathBuf, suites: &[(&str, &str)]) {
    let mut manifest = "[test.suites]\n".to_owned();
    for (suite, cmd) in suites {
        manifest.push_str(&format!("{suite} = \"{cmd}\"\n"));
    }
    write_root_manifest(root, &manifest);
}

fn install_local_vitest(root: &PathBuf, script: &str) {
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    write_executable(&local_bin.join("vitest"), script);
}

fn install_local_vitest_marker(root: &PathBuf, marker: &PathBuf) {
    install_local_vitest(
        root,
        &format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            marker.display()
        ),
    );
}

fn write_js_package_manager_manifest(root: &PathBuf, package_manager: &str) {
    write_root_manifest(
        root,
        &format!(
            r#"[package_manager]
js = "{package_manager}"
"#
        ),
    );
}

fn run_builtin_ok(root: PathBuf, name: &str, args: &[&str]) -> String {
    run_builtin(root, name, args).expect("built-in invocation should succeed")
}

fn run_builtin_err(root: PathBuf, name: &str, args: &[&str]) -> RunnerError {
    run_builtin(root, name, args).expect_err("built-in invocation should fail")
}

fn run_builtin(root: PathBuf, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
}

fn assert_builtin_ok_empty(root: PathBuf, name: &str, args: &[&str]) {
    let out = run_builtin_ok(root, name, args);
    assert_eq!(out, "");
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

fn assert_task_manifest_parse_runner_error_contains_any(err: RunnerError, expected: &[&str]) {
    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert_manifest_parse_error_contains_any(&error, expected);
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_doctor_non_zero_contains(err: RunnerError, expected: &[&str]) {
    match err {
        RunnerError::DoctorNonZero { rendered, .. } => assert_contains_all(&rendered, expected),
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_task_command_failure_code(err: RunnerError, expected_code: Option<Option<i32>>) {
    match err {
        RunnerError::TaskCommandFailure { code, .. } => {
            if let Some(expected) = expected_code {
                assert_eq!(code, expected);
            }
        }
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

fn run_tasks_with_repo(root: PathBuf) -> Result<String, RunnerError> {
    run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
}

fn run_doctor_task(root: PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    run_builtin(root, "doctor", args)
}
