pub(super) use super::super::prelude::*;

pub(super) fn run_task(root: &Path, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.to_path_buf(),
    )
}

pub(super) fn write_empty_manifest(root: &Path) {
    write_root_manifest(root, "");
}

pub(super) fn assert_run_task_ok_empty(root: &Path, name: &str, args: &[&str]) {
    let out = run_task(root, name, args).expect("task should succeed");
    assert_eq!(out, "");
}
