use crate::runner::error::RunnerError;
use crate::runner::execute::preflight::ExecutionPreflight;
use crate::runner::managed::run_spec::{render_task_run_spec, RunSpecContext};
use crate::runner::model::catalog::TaskSelection;
use crate::runner::util::render_passthrough_args;

pub(super) fn build_task_command(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let args_rendered = render_passthrough_args(&preflight.runtime_args_exec.passthrough);
    let run_spec =
        selection
            .task
            .run
            .as_ref()
            .ok_or_else(|| RunnerError::TaskMissingRunCommand {
                task: preflight.selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            })?;
    render_task_run_spec(
        run_spec,
        RunSpecContext {
            task_name: &preflight.selector.task_name,
            task_env: &selection.task.env,
            task_env_file: selection.task.env_file.as_ref(),
            env_profiles: &selection.catalog.manifest.env,
            args_rendered: &args_rendered,
            repo_root: &selection.catalog.catalog_root,
            catalogs: &preflight.catalogs,
            task_scope_cwd: &selection.catalog.catalog_root,
            depth: 0,
        },
    )
}
