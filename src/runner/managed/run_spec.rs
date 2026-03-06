use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::manifest::task_runtime::{ManifestEnvEntry, ManifestEnvFileDirective};

use super::super::manifest::task_runtime::ManifestManagedRun;
use super::super::model::catalog::LoadedCatalog;
use crate::runner::error::RunnerError;
#[path = "run_spec/command.rs"]
mod command;
#[path = "run_spec/run_step.rs"]
mod run_step;
#[path = "run_spec/sequence/mod.rs"]
mod sequence;

use command::{
    render_builtin_task_reference_invocation, render_task_command, wrap_command_with_cwd,
};

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

pub(in crate::runner) fn render_task_run_spec(
    run: &ManifestManagedRun,
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    if context.depth > 12 {
        return Err(RunnerError::task_invocation(format!(
            "task `{}` run expansion exceeded maximum nested task references (12)",
            context.task_name
        )));
    }
    match run {
        ManifestManagedRun::Command(command) => Ok(render_task_command(command, context)),
        ManifestManagedRun::Sequence(steps) => {
            sequence::render_run_sequence(steps, context.with_depth(context.depth + 1))
        }
    }
}

pub(in crate::runner) fn render_builtin_reference_invocation(
    task_ref: &str,
    args_rendered: &str,
) -> Result<String, RunnerError> {
    render_builtin_task_reference_invocation(task_ref, args_rendered)
}

pub(in crate::runner) fn wrap_reference_command_in_cwd(cwd: &Path, command: &str) -> String {
    wrap_command_with_cwd(cwd, command)
}
