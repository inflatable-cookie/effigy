pub(super) use super::super::prelude::*;

pub(super) fn run_validate_ok(root: &PathBuf, args: &[&str]) -> String {
    run_builtin_ok(root.clone(), "validate", args)
}

pub(super) fn run_validate_err(root: &PathBuf, args: &[&str]) -> RunnerError {
    run_builtin_err(root.clone(), "validate", args)
}

pub(super) struct RunArrayInvocationErrorCase {
    pub(super) workspace: &'static str,
    pub(super) manifest: &'static str,
    pub(super) expected: &'static [&'static str],
}

pub(super) struct RunArrayTaskRefParseErrorCase {
    pub(super) workspace: &'static str,
    pub(super) manifest: &'static str,
    pub(super) expected_tail: &'static str,
}

pub(super) struct BuiltinTestTaskRefCase {
    pub(super) workspace: &'static str,
    pub(super) suite_name: &'static str,
    pub(super) task_ref: &'static str,
}

pub(super) fn write_validate_manifest(root: &PathBuf, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

pub(super) fn assert_validate_ok_empty(root: &PathBuf, args: &[&str]) {
    let out = run_validate_ok(root, args);
    assert_eq!(out, "");
}
