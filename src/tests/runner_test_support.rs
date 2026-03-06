use super::super::test_support::execution::run_manifest_task_with_cwd;
use super::super::{run_tasks, RunnerError};
use crate::{TaskInvocation, TasksArgs};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::contract_test_support::write_manifest as write_manifest_shared;
pub(crate) mod env {
    pub(crate) use crate::contract_test_support::{
        lock_test, temp_workspace, wait_for_path_exists, with_cwd, EnvGuard,
    };
}

pub(crate) mod workspace {
    use super::{fs, write_manifest_shared, Path, PathBuf, PermissionsExt};

    pub(crate) fn write_manifest(path: &Path, body: &str) {
        write_manifest_shared(path, body);
    }

    pub(crate) fn write_root_manifest(root: &Path, body: &str) {
        write_manifest(&root.join("effigy.toml"), body);
    }

    pub(crate) fn create_workspace_dir(root: &Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).expect("mkdir workspace dir");
        dir
    }

    pub(crate) fn write_catalog_tasks(dir: &Path, alias: Option<&str>, tasks: &[(&str, &str)]) {
        let mut manifest = String::new();
        if let Some(alias) = alias {
            manifest.push_str(&format!("[catalog]\nalias = \"{alias}\"\n"));
        }
        for (task, run) in tasks {
            manifest.push_str(&format!("[tasks.{task}]\nrun = \"{run}\"\n"));
        }
        write_manifest(&dir.join("effigy.toml"), &manifest);
    }

    pub(crate) fn write_executable(path: &Path, script: &str) {
        fs::write(path, script).expect("write executable");
        let mut perms = fs::metadata(path).expect("stat").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }

    pub(crate) fn write_package_json_with_test_script(root: &Path) {
        fs::write(
            root.join("package.json"),
            "{ \"scripts\": { \"test\": \"vitest\" } }\n",
        )
        .expect("write package");
    }

    pub(crate) fn write_package_json_with_vitest_dev_dependency(root: &Path) {
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

    pub(crate) fn write_multi_suite_cargo_manifest(root: &Path) {
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
        )
        .expect("write cargo toml");
    }

    pub(crate) fn setup_multi_suite_repo(root: &Path) {
        write_package_json_with_test_script(root);
        write_multi_suite_cargo_manifest(root);
    }

    pub(crate) fn write_test_suites_manifest(root: &Path, suites: &[(&str, &str)]) {
        let mut manifest = "[test.suites]\n".to_owned();
        for (suite, cmd) in suites {
            manifest.push_str(&format!("{suite} = \"{cmd}\"\n"));
        }
        write_root_manifest(root, &manifest);
    }

    pub(crate) fn install_local_vitest(root: &Path, script: &str) {
        let local_bin = root.join("node_modules/.bin");
        fs::create_dir_all(&local_bin).expect("mkdir local bin");
        write_executable(&local_bin.join("vitest"), script);
    }

    pub(crate) fn install_local_vitest_marker(root: &Path, marker: &Path) {
        install_local_vitest(
            root,
            &format!(
                "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
                marker.display()
            ),
        );
    }

    pub(crate) fn write_js_package_manager_manifest(root: &Path, package_manager: &str) {
        write_root_manifest(
            root,
            &format!(
                r#"[package_manager]
js = "{package_manager}"
"#
            ),
        );
    }
}

pub(crate) mod builtin {
    use super::{run_manifest_task_with_cwd, PathBuf, RunnerError, TaskInvocation};

    pub(crate) fn run_builtin_ok(root: PathBuf, name: &str, args: &[&str]) -> String {
        run_builtin(root, name, args).expect("built-in invocation should succeed")
    }

    pub(crate) fn run_builtin_err(root: PathBuf, name: &str, args: &[&str]) -> RunnerError {
        run_builtin(root, name, args).expect_err("built-in invocation should fail")
    }

    pub(crate) fn run_builtin(
        root: PathBuf,
        name: &str,
        args: &[&str],
    ) -> Result<String, RunnerError> {
        run_manifest_task_with_cwd(
            &TaskInvocation {
                name: name.to_owned(),
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            },
            root,
        )
    }

    pub(crate) fn assert_builtin_ok_empty(root: PathBuf, name: &str, args: &[&str]) {
        let out = run_builtin_ok(root, name, args);
        assert_eq!(out, "");
    }

    pub(crate) fn run_doctor_task(root: PathBuf, args: &[&str]) -> Result<String, RunnerError> {
        run_builtin(root, "doctor", args)
    }
}

pub(crate) mod assertions {
    use super::RunnerError;

    pub(crate) fn assert_contains_all(rendered: &str, expected: &[&str]) {
        for snippet in expected {
            assert!(
                rendered.contains(snippet),
                "expected output to contain {:?}\noutput:\n{}",
                snippet,
                rendered
            );
        }
    }

    pub(crate) fn assert_task_invocation_error_contains(err: RunnerError, expected: &[&str]) {
        match err {
            RunnerError::TaskInvocation(message) => assert_contains_all(&message, expected),
            other => panic!("unexpected error: {other}"),
        }
    }

    pub(crate) fn assert_task_manifest_parse_runner_error_contains_any(
        err: RunnerError,
        expected: &[&str],
    ) {
        match err {
            RunnerError::TaskManifestParse { error, .. } => {
                assert_manifest_parse_error_contains_any(&error, expected);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    pub(crate) fn assert_doctor_non_zero_contains(err: RunnerError, expected: &[&str]) {
        match err {
            RunnerError::DoctorNonZero { rendered, .. } => assert_contains_all(&rendered, expected),
            other => panic!("unexpected error: {other}"),
        }
    }

    pub(crate) fn assert_task_command_failure_code(
        err: RunnerError,
        expected_code: Option<Option<i32>>,
    ) {
        match err {
            RunnerError::TaskCommandFailure { code, .. } => {
                if let Some(expected) = expected_code {
                    assert_eq!(code, expected);
                }
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    pub(crate) fn assert_manifest_parse_error_contains_any(
        error: &toml::de::Error,
        expected: &[&str],
    ) {
        let rendered = error.to_string();
        assert!(
            expected.iter().any(|pattern| rendered.contains(pattern)),
            "expected parse error to contain one of {:?}, got: {}",
            expected,
            rendered
        );
    }
}

pub(crate) mod json {
    pub(crate) fn parse_json_output(rendered: &str) -> serde_json::Value {
        serde_json::from_str(rendered).expect("parse json")
    }
}

pub(crate) mod tasks {
    use super::{run_tasks, Path, PathBuf, RunnerError, TasksArgs};

    pub(crate) fn run_tasks_from_repo(
        root: &Path,
        task_name: Option<&str>,
        resolve_selector: Option<&str>,
        output_json: bool,
    ) -> String {
        crate::contract_test_support::with_cwd(root, || {
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

    pub(crate) fn run_tasks_with_repo(root: PathBuf) -> Result<String, RunnerError> {
        run_tasks(TasksArgs {
            repo_override: Some(root),
            task_name: None,
            resolve_selector: None,
            output_json: false,
            pretty_json: true,
        })
    }
}
