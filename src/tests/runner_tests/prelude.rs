pub(super) use super::super::{
    builtin_test_max_parallel, discover_catalogs, parse_task_runtime_args, parse_task_selector,
    run_doctor, run_manifest_task_with_cwd, run_tasks, RunnerError, TaskRuntimeArgs,
};
pub(super) use super::runner_test_support::*;
pub(super) use crate::{DoctorArgs, TaskInvocation, TasksArgs};
pub(super) use std::fs;
#[cfg(unix)]
pub(super) use std::os::unix::fs::symlink;
pub(super) use std::path::PathBuf;
pub(super) use std::thread;
pub(super) use std::time::{Duration, Instant};

pub(super) fn write_root_dev_task_manifest(root: &PathBuf) {
    write_root_manifest(root, "[tasks.dev]\nrun = \"printf root\"\n");
}

pub(super) fn setup_root_with_catalog_tasks(
    name: &str,
    catalogs: &[(&str, &[(&str, &str)])],
    include_root_dev_task: bool,
) -> PathBuf {
    let root = temp_workspace(name);
    if include_root_dev_task {
        write_root_dev_task_manifest(&root);
    }
    for (dir_name, tasks) in catalogs {
        let dir = create_workspace_dir(&root, dir_name);
        write_catalog_tasks(&dir, Some(dir_name), tasks);
    }
    root
}

pub(super) fn create_workspace_path(root: &PathBuf, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(&path).expect("mkdir workspace path");
    path
}

pub(super) fn write_root_ping_task(root: &PathBuf) {
    write_manifest(&root.join("effigy.toml"), "[tasks.ping]\nrun = \"printf root\"\n");
}

pub(super) fn setup_root_and_farmyard_ping(workspace: &str) -> (PathBuf, PathBuf) {
    let root = temp_workspace(workspace);
    let farmyard = create_workspace_dir(&root, "farmyard");
    write_root_ping_task(&root);
    write_catalog_tasks(&farmyard, Some("farmyard"), &[("ping", "printf farmyard")]);
    (root, farmyard)
}

pub(super) fn write_managed_dev_profile_manifest(root: &PathBuf, profile: &str) {
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
concurrent = [{{ run = "printf api" }}]

[tasks.dev.profiles.{}]
concurrent = [{{ run = "printf api" }}]
"#,
            profile
        ),
    );
}

pub(super) fn run_task_in_workspace(
    root: &PathBuf,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.clone(),
    )
}

pub(super) fn write_defer_manifest(root: &PathBuf, defer_run: &str) {
    write_manifest(
        &root.join("effigy.toml"),
        &format!("[defer]\nrun = \"{defer_run}\"\n"),
    );
}

pub(super) fn doctor_nonzero_rendered(err: RunnerError) -> String {
    match err {
        RunnerError::DoctorNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn run_doctor_err_from_cwd(root: &PathBuf, fix: bool) -> RunnerError {
    with_cwd(root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix,
            verbose: false,
            explain: None,
        })
    })
    .expect_err("doctor should fail")
}

pub(super) fn setup_doctor_explain_catalog_workspace(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    let farmyard = create_workspace_dir(&root, "farmyard");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf farmyard\"\n",
    );
    root
}
