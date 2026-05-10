use super::execution::run_manifest_task_with_cwd;
use super::harness_builtin::run_builtin_ok;
use super::harness_env::{temp_workspace, with_cwd};
use super::harness_workspace::{
    create_workspace_dir, write_catalog_tasks, write_manifest, write_root_manifest,
};
use super::runtime::{fs, run_doctor, DoctorArgs, Path, PathBuf, RunnerError, TaskInvocation};

pub(in crate::runner::tests) fn write_root_dev_task_manifest(root: &Path) {
    write_root_manifest(root, "[tasks.dev]\nrun = \"printf root\"\n");
}

pub(in crate::runner::tests) fn setup_root_with_catalog_tasks(
    name: &str,
    catalogs: &[(&str, &[(&str, &str)])],
    include_root_dev_task: bool,
) -> PathBuf {
    let root = temp_workspace(name);
    if include_root_dev_task {
        write_root_dev_task_manifest(&root);
    } else {
        write_root_manifest(&root, "");
    }
    for (dir_name, tasks) in catalogs {
        let dir = create_workspace_dir(&root, dir_name);
        write_catalog_tasks(&dir, Some(dir_name), tasks);
    }
    root
}

pub(in crate::runner::tests) fn create_workspace_path(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(&path).expect("mkdir workspace path");
    path
}

pub(in crate::runner::tests) fn write_root_ping_task(root: &Path) {
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
}

pub(in crate::runner::tests) fn setup_root_and_catalog_a_ping(
    workspace: &str,
) -> (PathBuf, PathBuf) {
    let root = temp_workspace(workspace);
    let catalog_a = create_workspace_dir(&root, "catalog_a");
    write_root_ping_task(&root);
    write_catalog_tasks(
        &catalog_a,
        Some("catalog_a"),
        &[("ping", "printf catalog_a")],
    );
    (root, catalog_a)
}

pub(in crate::runner::tests) fn write_managed_dev_profile_manifest(root: &Path, profile: &str) {
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

pub(in crate::runner::tests) fn run_task_in_workspace(
    root: &Path,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.to_path_buf(),
    )
}

pub(in crate::runner::tests) fn write_defer_manifest(
    root: &Path,
    defer_run: &str,
    run_in: Option<&str>,
) {
    let run_in = run_in
        .map(|value| format!("run_in = \"{value}\"\n"))
        .unwrap_or_default();
    write_manifest(
        &root.join("effigy.toml"),
        &format!("[defer]\nrun = \"{defer_run}\"\n{run_in}"),
    );
}

pub(in crate::runner::tests) fn doctor_nonzero_rendered(err: RunnerError) -> String {
    match err {
        RunnerError::DoctorNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn run_doctor_err_from_cwd(root: &Path, fix: bool) -> RunnerError {
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

pub(in crate::runner::tests) fn setup_doctor_explain_catalog_workspace(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    let catalog_a = create_workspace_dir(&root, "catalog_a");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf catalog_a\"\n",
    );
    root
}

pub(in crate::runner::tests) fn workspace_with_empty_manifest(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    write_root_manifest(&root, "");
    root
}

pub(in crate::runner::tests) fn run_config_ok(root: PathBuf, args: &[&str]) -> String {
    run_builtin_ok(root, "config", args)
}
