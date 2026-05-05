use super::*;
use effigy_execution::ExecutionSurface;

#[path = "task/runtime.rs"]
mod runtime;
#[path = "task/selection.rs"]
mod selection;

pub(in crate::runner::demo_command) use selection::{
    concurrent_runner_task_process_names, demo_task_selection, DemoTaskSelectionResolved,
};

pub(in crate::runner::demo_command) fn execute_task_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    demo_mode: ManifestDemoMode,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    if let Some(selection) = demo_task_selection(repo_root, task_name)? {
        if task_is_concurrent_runner_backed(selection.task()?) {
            return runtime::execute_concurrent_runner_backed_demo(
                repo_root,
                demo_id,
                task_name,
                demo_mode,
                selection,
                output_json,
            );
        }
    }

    let active_record =
        PersistedDemoActiveAttempt::new_task_backed(build_attempt_id(demo_id), demo_id, task_name);
    let _active_guard = register_active_attempt(repo_root, demo_id, &active_record)?;

    if output_json {
        let task = TaskInvocation {
            name: task_name.to_owned(),
            args: vec!["--json".to_owned()],
        };
        return match crate::runner::execute::api::run_manifest_task_with_surface(
            &task,
            repo_root.to_path_buf(),
            ExecutionSurface::Demo,
        ) {
            Ok(rendered) => {
                parse_task_backed_attempt_json(repo_root, demo_id, task_name, &rendered)
            }
            Err(RunnerError::CommandJsonFailure { rendered }) => {
                parse_task_backed_attempt_json(repo_root, demo_id, task_name, &rendered)
            }
            Err(error) => Ok(failed_demo_attempt(
                "task",
                task_name,
                task_name,
                None,
                format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
                String::new(),
                String::new(),
                DemoLogPaths::none(),
            )),
        };
    }

    let task = TaskInvocation {
        name: task_name.to_owned(),
        args: Vec::new(),
    };
    match crate::runner::execute::api::run_manifest_task_with_surface(
        &task,
        repo_root.to_path_buf(),
        ExecutionSurface::Demo,
    ) {
        Ok(_) => Ok(successful_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            Some(format!(
                "Demo `{demo_id}` completed via task `{task_name}`."
            )),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
        Err(RunnerError::TaskCommandFailure { code, .. }) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            code,
            format!("Demo `{demo_id}` failed via task `{task_name}`."),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
        Err(error) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
            String::new(),
            String::new(),
            DemoLogPaths::none(),
        )),
    }
}

fn parse_task_backed_attempt_json(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    rendered: &str,
) -> Result<DemoExecutionAttempt, RunnerError> {
    demo_parse_task_backed_attempt_json(repo_root, demo_id, task_name, rendered).map_err(Into::into)
}
