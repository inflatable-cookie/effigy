use super::{run_manifest_task_with_cwd, PathBuf, RunnerError, TaskInvocation};

pub(crate) fn run_builtin_ok(root: PathBuf, name: &str, args: &[&str]) -> String {
    run_builtin(root, name, args).expect("built-in invocation should succeed")
}

pub(crate) fn run_builtin_err(root: PathBuf, name: &str, args: &[&str]) -> RunnerError {
    run_builtin(root, name, args).expect_err("built-in invocation should fail")
}

pub(crate) fn run_builtin(root: PathBuf, name: &str, args: &[&str]) -> Result<String, RunnerError> {
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
