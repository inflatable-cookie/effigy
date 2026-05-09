use super::{run_manifest_task_with_cwd, PathBuf, RunnerError, TaskInvocation};

fn builtin_invocation(name: &str, args: &[&str]) -> TaskInvocation {
    match name {
        "migrate" | "unlock" | "cache" => TaskInvocation {
            name: "tasks".to_owned(),
            args: std::iter::once(name.to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        "completion" => TaskInvocation {
            name: "config".to_owned(),
            args: std::iter::once("completion".to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect(),
        },
        _ => TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
    }
}

pub(crate) fn run_builtin_ok(root: PathBuf, name: &str, args: &[&str]) -> String {
    run_builtin(root, name, args).expect("built-in invocation should succeed")
}

pub(crate) fn run_builtin_err(root: PathBuf, name: &str, args: &[&str]) -> RunnerError {
    run_builtin(root, name, args).expect_err("built-in invocation should fail")
}

pub(crate) fn run_builtin(root: PathBuf, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(&builtin_invocation(name, args), root)
}

pub(crate) fn assert_builtin_ok_empty(root: PathBuf, name: &str, args: &[&str]) {
    let out = run_builtin_ok(root, name, args);
    assert_eq!(out, "");
}

pub(crate) fn run_doctor_task(root: PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    run_builtin(root, "doctor", args)
}
