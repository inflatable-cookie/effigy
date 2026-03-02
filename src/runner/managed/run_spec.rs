use std::path::Path;

use super::super::{LoadedCatalog, ManifestManagedRun, RunnerError};
#[path = "run_spec/command.rs"]
mod command;
#[path = "run_spec/run_step.rs"]
mod run_step;
#[path = "run_spec/sequence.rs"]
mod sequence;

use command::render_command_template;

pub(super) fn render_task_run_spec(
    task_name: &str,
    run: &ManifestManagedRun,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    if depth > 12 {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run expansion exceeded maximum nested task references (12)"
        )));
    }
    match run {
        ManifestManagedRun::Command(command) => {
            Ok(render_command_template(command, repo_root, args_rendered))
        }
        ManifestManagedRun::Sequence(steps) => sequence::render_run_sequence(
            task_name,
            steps,
            args_rendered,
            repo_root,
            catalogs,
            task_scope_cwd,
            depth + 1,
        ),
    }
}
