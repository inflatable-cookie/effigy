use std::collections::BTreeMap;
use std::path::Path;

use effigy_manifest::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRunStep, ManifestRunStepEnv,
};

use effigy_manifest::TaskResolverFn;

pub use effigy_core::host_alias::HostAlias;

use crate::ManagedError;
use effigy_manifest::LoadedCatalog;
use effigy_manifest::ManifestManagedRun;
#[path = "run_spec/command.rs"]
mod command;
#[path = "run_spec/run_step.rs"]
mod run_step;
#[path = "run_spec/sequence/mod.rs"]
mod sequence;

pub use sequence::StepEnvAccumulator;

use command::{
    render_builtin_task_reference_invocation, render_command_template, render_task_command,
    wrap_command_with_cwd, wrap_command_with_task_env,
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
    pub invocation_cwd: &'a Path,
    pub runtime_env_schema_override: Option<&'a Path>,
    /// Declared backing-service aliases this host run resolves to its own
    /// checkout's effective aliases (effigy#431, contract 005). Empty for a
    /// checkout that keeps its declared names and for every render that is not
    /// a host execution environment.
    pub host_env_aliases: &'a [HostAlias],
    pub depth: usize,
    pub resolver: TaskResolverFn<'a>,
}

impl RunSpecContext<'_> {
    fn with_depth(self, depth: usize) -> Self {
        Self { depth, ..self }
    }
}

/// Every env key a rendered command delivers itself.
///
/// Effigy renders a delivered environment as `env 'KEY=value' … sh -c '…'`, and
/// a command body may assign a key of its own; either way the key is delivered
/// by the command, not inherited, so a caller that resolves inherited values
/// must not resolve or prove this one. This reads the same rendering
/// [`render_task_run_spec`] writes.
pub fn declared_env_keys(command: &str) -> std::collections::BTreeSet<String> {
    let mut keys = std::collections::BTreeSet::new();
    for token in command
        .split(|character: char| character.is_whitespace() || character == '\'' || character == '"')
    {
        let Some((name, _)) = token.split_once('=') else {
            continue;
        };
        let mut characters = name.chars();
        let Some(first) = characters.next() else {
            continue;
        };
        if !(first.is_ascii_alphabetic() || first == '_')
            || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            continue;
        }
        keys.insert(name.to_owned());
    }
    keys
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
    repo_root: &Path,
) -> Result<String, ManagedError> {
    render_builtin_task_reference_invocation(task_ref, args_rendered, repo_root)
}

pub fn render_step_command_template(
    command: &str,
    repo_root: &Path,
    bundle_root: Option<&Path>,
    args_rendered: &str,
) -> String {
    render_command_template(command, repo_root, bundle_root, args_rendered)
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

// One resolved environment from one declaration: the shape predates the scoped
// alias input, and splitting it would move every caller's arguments around.
#[allow(clippy::too_many_arguments)]
pub fn resolve_run_step_env(
    owner_label: &str,
    env: Option<&ManifestRunStepEnv>,
    env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    runtime_env_schema_override: Option<&Path>,
    host_env_aliases: &[HostAlias],
) -> Result<BTreeMap<String, String>, ManagedError> {
    let mut resolved = sequence::resolve_standalone_env(
        owner_label,
        env,
        env_file,
        env_profiles,
        repo_root,
        catalogs,
        runtime_env_schema_override,
    )?;
    effigy_core::host_alias::resolve_env(&mut resolved, host_env_aliases);
    Ok(resolved)
}

pub struct RunStepSequenceContext<'a> {
    pub owner_label: &'a str,
    pub task_env: &'a BTreeMap<String, String>,
    pub task_env_file: Option<&'a ManifestEnvFileDirective>,
    pub env_profiles: &'a BTreeMap<String, ManifestEnvEntry>,
    pub repo_root: &'a Path,
    pub bundle_root: Option<&'a Path>,
    pub catalogs: &'a [LoadedCatalog],
    pub task_scope_cwd: &'a Path,
    pub runtime_env_schema_override: Option<&'a Path>,
    /// Declared backing-service aliases this host run resolves to its own
    /// checkout's effective aliases (effigy#431, contract 005).
    pub host_env_aliases: &'a [HostAlias],
    pub resolver: TaskResolverFn<'a>,
}

pub fn render_run_step_sequence<'a>(
    steps: &[ManifestManagedRunStep],
    context: RunStepSequenceContext<'a>,
) -> Result<String, ManagedError> {
    let RunStepSequenceContext {
        owner_label,
        task_env,
        task_env_file,
        env_profiles,
        repo_root,
        bundle_root,
        catalogs,
        task_scope_cwd,
        runtime_env_schema_override,
        host_env_aliases,
        resolver,
    } = context;
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
            invocation_cwd: task_scope_cwd,
            runtime_env_schema_override,
            host_env_aliases,
            depth: 1,
            resolver,
        },
    )
}
