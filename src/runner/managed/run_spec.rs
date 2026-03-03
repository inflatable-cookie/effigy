use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::manifest::{ManifestEnvEntry, ManifestEnvFileDirective};

use super::super::{LoadedCatalog, ManifestManagedRun, RunnerError};
#[path = "run_spec/command.rs"]
mod command;
#[path = "run_spec/run_step.rs"]
mod run_step;
#[path = "run_spec/sequence.rs"]
mod sequence;

use command::{render_command_template, wrap_command_with_task_env};

pub(super) fn render_task_run_spec(
    task_name: &str,
    run: &ManifestManagedRun,
    task_env: &BTreeMap<String, String>,
    task_env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
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
    let rendered = match run {
        ManifestManagedRun::Command(command) => {
            render_command_template(command, repo_root, args_rendered)
        }
        ManifestManagedRun::Sequence(steps) => sequence::render_run_sequence(
            task_name,
            steps,
            task_env_file,
            env_profiles,
            args_rendered,
            repo_root,
            catalogs,
            task_scope_cwd,
            depth + 1,
        )?,
    };
    Ok(wrap_command_with_task_env(rendered, task_env, repo_root))
}
