pub(super) use super::super::prelude::*;

pub(super) fn run_dev(root: &PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.clone(),
    )
}

pub(super) fn run_dev_with_repo(root: &PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(args.iter().map(|arg| (*arg).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: full_args,
        },
        root.clone(),
    )
}

pub(super) fn run_unlock_with_repo(root: &PathBuf, scopes: &[&str]) -> Result<String, RunnerError> {
    let mut args = vec!["--repo".to_owned(), root.display().to_string()];
    args.extend(scopes.iter().map(|scope| (*scope).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "unlock".to_owned(),
            args,
        },
        root.clone(),
    )
}

pub(super) fn run_task_with_repo(
    root: &PathBuf,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(args.iter().map(|arg| (*arg).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: full_args,
        },
        root.clone(),
    )
}

pub(super) fn managed_tui_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))])
}

pub(super) fn managed_stream_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))])
}

pub(super) fn write_catalogs_with_tasks(root: &PathBuf, catalogs: &[(&str, &[(&str, &str)])]) {
    for (name, tasks) in catalogs {
        let dir = create_workspace_dir(root, name);
        write_catalog_tasks(&dir, Some(name), tasks);
    }
}

pub(super) fn write_managed_admin_profile_manifest(root: &PathBuf) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "front", run = "vite dev", start = 2, tab = 2 },
  { name = "admin", run = "vite dev --config admin", start = 3, tab = 3 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "admin", run = "vite dev --config admin", start = 2, tab = 2 }
]
"#,
    );
}

pub(super) fn write_managed_stream_builtin_test_manifest(
    root: &PathBuf,
    suite: &str,
    test_task_ref: &str,
    marker: &PathBuf,
) {
    write_root_manifest(
        root,
        &format!(
            r#"[test.suites]
{} = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "{}" }}]
"#,
            suite,
            marker.display(),
            test_task_ref
        ),
    );
}

pub(super) fn write_managed_stream_profile_manifest(root: &PathBuf) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "default-only", run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ name = "front-only", run = "printf front-ok" }]
"#,
    );
}

pub(super) fn assert_run_dev_with_repo_contains(root: &PathBuf, args: &[&str], expected: &[&str]) {
    let out = run_dev_with_repo(root, args).expect("managed plan should render");
    assert_contains_all(&out, expected);
}

pub(super) struct ManagedPlanCase {
    pub(super) workspace: &'static str,
}

pub(super) struct ManagedStreamBuiltinTestCase {
    pub(super) workspace: &'static str,
    pub(super) suite: &'static str,
    pub(super) task_ref: &'static str,
}

pub(super) struct ManagedTaskRefInvalidCase {
    pub(super) workspace: &'static str,
    pub(super) manifest: &'static str,
    pub(super) expected_reference: &'static str,
    pub(super) expected_detail: &'static str,
}

pub(super) fn assert_managed_process_invalid_definition(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_detail_substring: Option<&str>,
) {
    match err {
        RunnerError::TaskManagedProcessInvalidDefinition {
            task,
            process,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            if let Some(detail_substring) = expected_detail_substring {
                assert!(detail.contains(detail_substring));
            }
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn assert_managed_profile_not_found(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
            assert_eq!(
                available,
                expected_available
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn assert_managed_task_reference_invalid(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_reference: &str,
    expected_detail_substring: &str,
) {
    match err {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            assert_eq!(reference, expected_reference);
            assert!(detail.contains(expected_detail_substring));
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn assert_managed_non_zero_exit(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_processes: &[(&str, &str)],
) {
    match err {
        RunnerError::TaskManagedNonZeroExit {
            task,
            profile,
            processes,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
            assert_eq!(
                processes,
                expected_processes
                    .iter()
                    .map(|(process, exit)| ((*process).to_owned(), (*exit).to_owned()))
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn assert_lock_conflict(err: RunnerError, expected_scope: &str, expected_remediation: &str) {
    match err {
        RunnerError::TaskLockConflict {
            scope, remediation, ..
        } => {
            assert_eq!(scope, expected_scope);
            assert!(remediation.contains(expected_remediation));
        }
        other => panic!("unexpected error: {other}"),
    }
}
