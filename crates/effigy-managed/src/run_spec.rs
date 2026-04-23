use std::collections::BTreeMap;
use std::path::Path;

use effigy_manifest::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRunStep, ManifestRunStepEnv,
};

use effigy_manifest::TaskResolverFn;

use crate::ManagedError;
use effigy_manifest::LoadedCatalog;
use effigy_manifest::ManifestManagedRun;
#[path = "run_spec/command.rs"]
mod command;
#[path = "run_spec/run_step.rs"]
mod run_step;
#[path = "run_spec/sequence/mod.rs"]
mod sequence;

use command::{
    render_builtin_task_reference_invocation, render_task_command, wrap_command_with_cwd,
    wrap_command_with_task_env,
};

#[derive(Clone, Copy)]
pub struct RunSpecContext<'a> {
    pub task_name: &'a str,
    pub task_env: &'a BTreeMap<String, String>,
    pub task_env_file: Option<&'a ManifestEnvFileDirective>,
    pub env_profiles: &'a BTreeMap<String, ManifestEnvEntry>,
    pub args_rendered: &'a str,
    pub args_raw: &'a [String],
    pub repo_root: &'a Path,
    pub bundle_root: Option<&'a Path>,
    pub catalogs: &'a [LoadedCatalog],
    pub task_scope_cwd: &'a Path,
    pub runtime_env_schema_override: Option<&'a Path>,
    pub depth: usize,
    pub resolver: TaskResolverFn<'a>,
}

impl RunSpecContext<'_> {
    fn with_depth(self, depth: usize) -> Self {
        Self { depth, ..self }
    }
}

pub fn render_task_run_spec(
    run: &ManifestManagedRun,
    context: RunSpecContext<'_>,
) -> Result<String, ManagedError> {
    if context.depth > 12 {
        return Err(ManagedError::task_invocation(format!(
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

pub fn render_builtin_reference_invocation(
    task_ref: &str,
    args_rendered: &str,
) -> Result<String, ManagedError> {
    render_builtin_task_reference_invocation(task_ref, args_rendered)
}

pub fn wrap_reference_command_in_cwd(cwd: &Path, command: &str) -> String {
    wrap_command_with_cwd(cwd, command)
}

pub fn wrap_command_with_env(
    command: String,
    env: &BTreeMap<String, String>,
    repo_root: &Path,
) -> String {
    wrap_command_with_task_env(command, env, repo_root)
}

pub fn resolve_run_step_env(
    owner_label: &str,
    env: Option<&ManifestRunStepEnv>,
    env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    runtime_env_schema_override: Option<&Path>,
) -> Result<BTreeMap<String, String>, ManagedError> {
    sequence::resolve_standalone_env(
        owner_label,
        env,
        env_file,
        env_profiles,
        repo_root,
        catalogs,
        runtime_env_schema_override,
    )
}

pub fn render_run_step_sequence<'a>(
    owner_label: &'a str,
    steps: &[ManifestManagedRunStep],
    task_env: &'a BTreeMap<String, String>,
    task_env_file: Option<&'a ManifestEnvFileDirective>,
    env_profiles: &'a BTreeMap<String, ManifestEnvEntry>,
    repo_root: &'a Path,
    bundle_root: Option<&'a Path>,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &'a Path,
    runtime_env_schema_override: Option<&'a Path>,
    resolver: TaskResolverFn<'a>,
) -> Result<String, ManagedError> {
    sequence::render_run_sequence(
        steps,
        RunSpecContext {
            task_name: owner_label,
            task_env,
            task_env_file,
            env_profiles,
            args_rendered: "",
            args_raw: &[],
            repo_root,
            bundle_root,
            catalogs,
            task_scope_cwd,
            runtime_env_schema_override,
            depth: 1,
            resolver,
        },
    )
}
