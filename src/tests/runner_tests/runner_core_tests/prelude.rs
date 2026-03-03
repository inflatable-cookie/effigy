pub(super) use super::super::prelude::*;

pub(super) fn run_task(root: &PathBuf, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.clone(),
    )
}

pub(super) fn assert_catalog_prefix_not_found(
    err: RunnerError,
    expected_prefix: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskCatalogPrefixNotFound { prefix, available } => {
            assert_eq!(prefix, expected_prefix);
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

pub(super) fn write_empty_manifest(root: &PathBuf) {
    write_root_manifest(root, "");
}

pub(super) fn write_build_task_manifest(root: &PathBuf, run: &str) {
    write_root_manifest(root, &format!("[tasks.build]\nrun = \"{run}\"\n"));
}

pub(super) fn write_package_json_scripts(root: &PathBuf, scripts: &[(&str, &str)]) {
    let entries = scripts
        .iter()
        .map(|(name, command)| format!("    \"{name}\": \"{command}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let package_json = format!("{{\n  \"scripts\": {{\n{entries}\n  }}\n}}\n");
    fs::write(root.join("package.json"), package_json).expect("write package scripts");
}

pub(super) struct BuiltinErrorCase {
    pub(super) workspace: &'static str,
    pub(super) command: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) manifest: &'static str,
    pub(super) expected: &'static [&'static str],
}

pub(super) struct BuiltinHelpCase {
    pub(super) workspace: &'static str,
    pub(super) command: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) expected: &'static [&'static str],
}

pub(super) fn assert_builtin_help_case(case: &BuiltinHelpCase) {
    let root = temp_workspace(case.workspace);
    write_empty_manifest(&root);
    let out = run_builtin_ok(root, case.command, case.args);
    assert_contains_all(&out, case.expected);
}

pub(super) fn assert_builtin_ok_contains(root: PathBuf, command: &str, args: &[&str], expected: &[&str]) {
    let out = run_builtin_ok(root, command, args);
    assert_contains_all(&out, expected);
}

pub(super) fn assert_run_task_ok_empty(root: &PathBuf, name: &str, args: &[&str]) {
    let out = run_task(root, name, args).expect("task should succeed");
    assert_eq!(out, "");
}
