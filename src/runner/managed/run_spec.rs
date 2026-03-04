use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::manifest::{ManifestEnvEntry, ManifestEnvFileDirective};

use super::super::{LoadedCatalog, ManifestManagedRun, RunnerError};
#[path = "run_spec/command.rs"]
mod command;
#[path = "run_spec/run_step.rs"]
mod run_step;
#[path = "run_spec/sequence/mod.rs"]
mod sequence;

use command::{render_command_template, wrap_command_with_task_env};

#[derive(Clone, Copy)]
pub(in super::super) struct RunSpecContext<'a> {
    pub(in super::super) task_name: &'a str,
    pub(in super::super) task_env: &'a BTreeMap<String, String>,
    pub(in super::super) task_env_file: Option<&'a ManifestEnvFileDirective>,
    pub(in super::super) env_profiles: &'a BTreeMap<String, ManifestEnvEntry>,
    pub(in super::super) args_rendered: &'a str,
    pub(in super::super) repo_root: &'a Path,
    pub(in super::super) catalogs: &'a [LoadedCatalog],
    pub(in super::super) task_scope_cwd: &'a Path,
    pub(in super::super) depth: usize,
}

impl RunSpecContext<'_> {
    fn with_depth(self, depth: usize) -> Self {
        Self { depth, ..self }
    }
}

pub(super) fn render_task_run_spec(
    run: &ManifestManagedRun,
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    if context.depth > 12 {
        return Err(RunnerError::task_invocation(format!(
            "task `{}` run expansion exceeded maximum nested task references (12)",
            context.task_name
        )));
    }
    let rendered = match run {
        ManifestManagedRun::Command(command) => {
            render_command_template(command, context.repo_root, context.args_rendered)
        }
        ManifestManagedRun::Sequence(steps) => {
            sequence::render_run_sequence(steps, context.with_depth(context.depth + 1))?
        }
    };
    Ok(wrap_command_with_task_env(
        rendered,
        context.task_env,
        context.repo_root,
    ))
}
