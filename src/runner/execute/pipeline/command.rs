use std::collections::BTreeMap;

use crate::runner::error::RunnerError;
use crate::runner::execute::planning::ExecutionPreflight;
use crate::runner::util::render_passthrough_args;
use effigy_env::resolver::ResolvedEnv;
use effigy_managed::run_spec::{render_task_run_spec, RunSpecContext};
use effigy_manifest::TaskSelection;

pub(in crate::runner) fn build_task_command(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    env_schema_resolved: &Option<ResolvedEnv>,
) -> Result<String, RunnerError> {
    let args_rendered = render_passthrough_args(&preflight.runtime_args_exec.passthrough);

    // Merge env-schema plain (non-sensitive) values into the task env.
    // Task-level `env = { KEY = "value" }` from effigy.toml takes priority.
    let merged_env = merge_env_schema_plain(&selection.task.env, env_schema_resolved.as_ref());
    let runtime_env_schema_override = preflight.runtime_args_raw.env_schema_override.as_deref();

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
            task_env: &merged_env,
            task_env_file: selection.task.env_file.as_ref(),
            env_profiles: &selection.catalog.manifest.env,
            args_rendered: &args_rendered,
            args_raw: &preflight.runtime_args_exec.passthrough,
            repo_root: &selection.catalog.catalog_root,
            bundle_root: selection.catalog.bundle_root.as_deref(),
            catalogs: &preflight.catalogs,
            task_scope_cwd: &selection.catalog.catalog_root,
            invocation_cwd: &preflight.invocation_cwd,
            runtime_env_schema_override,
            depth: 0,
            resolver: &effigy_routing::resolve_task_selection,
        },
    )
    .map_err(Into::into)
}

/// Merge env-schema plain values into the task env map.
/// Task-level declarations in effigy.toml take priority over env-schema values.
fn merge_env_schema_plain(
    task_env: &BTreeMap<String, String>,
    env_schema_resolved: Option<&ResolvedEnv>,
) -> BTreeMap<String, String> {
    let Some(resolved) = env_schema_resolved else {
        return task_env.clone();
    };

    let mut merged = resolved.plain_env();
    // Task-level env takes priority — overwrite any env-schema values
    for (key, value) in task_env {
        merged.insert(key.clone(), value.clone());
    }
    merged
}
