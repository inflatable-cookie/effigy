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

/// Every env key a rendered command delivers itself as a **spawn-level**
/// assignment.
///
/// Effigy renders a delivered environment as `env 'KEY=value' … <program>`, so
/// the assignments are the shell words between the leading `env` and the first
/// word that is not one. Only those count: a key that merely appears in the
/// command body — a log line, a pattern, a quoted literal — names nothing this
/// child's environment carries, so a caller resolving inherited values must
/// still resolve and prove that key (effigy#431 review).
pub fn declared_env_keys(command: &str) -> std::collections::BTreeSet<String> {
    let mut keys = std::collections::BTreeSet::new();
    let mut rest = command.trim_start();
    let Some(after_env) = rest.strip_prefix("env") else {
        return keys;
    };
    if !after_env.is_empty()
        && !after_env
            .chars()
            .next()
            .is_some_and(|character| character.is_whitespace())
    {
        return keys;
    }
    rest = after_env.trim_start();
    while let Some((word, remaining)) = shell_word(rest) {
        let Some((name, _)) = word.split_once('=') else {
            break;
        };
        let mut characters = name.chars();
        let Some(first) = characters.next() else {
            break;
        };
        if !(first.is_ascii_alphabetic() || first == '_')
            || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            break;
        }
        keys.insert(name.to_owned());
        rest = remaining.trim_start();
    }
    keys
}

/// One shell word and the text after it, honouring single and double quotes and
/// a backslash escape. `None` at the end of the text.
fn shell_word(text: &str) -> Option<(String, &str)> {
    let mut word = String::new();
    let mut characters = text.char_indices();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while let Some((index, character)) = characters.next() {
        if escaped {
            word.push(character);
            escaped = false;
            continue;
        }
        match (quote, character) {
            (_, '\\') if quote != Some('\'') => escaped = true,
            (Some(open), _) if character == open => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, _) if character.is_whitespace() => {
                return Some((word, &text[index + character.len_utf8()..]));
            }
            _ => word.push(character),
        }
    }
    if word.is_empty() && quote.is_none() {
        return None;
    }
    Some((word, ""))
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

#[cfg(test)]
mod tests {
    use super::declared_env_keys;

    /// Only the spawn-level `env` prefix counts: a key the command body merely
    /// mentions is not delivered, so a caller must still resolve and prove it
    /// (effigy#431 review).
    #[test]
    fn only_the_spawn_level_prefix_delivers_a_key() {
        assert_eq!(
            declared_env_keys(
                "env 'DATABASE_URL=postgres://a@b:5432/db' 'OUT=/tmp/x' sh -c 'true'"
            )
            .into_iter()
            .collect::<Vec<_>>(),
            vec!["DATABASE_URL".to_owned(), "OUT".to_owned()]
        );
        assert_eq!(
            declared_env_keys("env DATABASE_URL=simple sh -c 'true'")
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["DATABASE_URL".to_owned()],
            "a plain word is an assignment"
        );
        assert!(declared_env_keys("sh -lc 'echo DATABASE_URL=only-a-log'").is_empty());
        assert!(declared_env_keys("sh -lc 'printf %s $DATABASE_URL > out'").is_empty());
        assert!(declared_env_keys("DATABASE_URL=x sh -c 'true'").is_empty());
        assert!(declared_env_keys("environs 'DATABASE_URL=x' sh -c 'true'").is_empty());
        assert!(declared_env_keys("").is_empty());
    }
}
