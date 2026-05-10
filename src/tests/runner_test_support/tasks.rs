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
            status_selector: None,
            output_json,
            pretty_json: true,
        })
    })
    .expect("run tasks")
}

pub(crate) fn run_task_status_from_repo(root: &Path, selector: &str, output_json: bool) -> String {
    crate::contract_test_support::with_cwd(root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: Some(selector.to_owned()),
            output_json,
            pretty_json: true,
        })
    })
    .expect("run task status")
}

pub(crate) fn run_tasks_with_repo(root: PathBuf) -> Result<String, RunnerError> {
    run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
        resolve_selector: None,
        status_selector: None,
        output_json: false,
        pretty_json: true,
    })
}
