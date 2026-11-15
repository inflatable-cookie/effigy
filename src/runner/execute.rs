#[path = "execute/api.rs"]
pub(in crate::runner) mod api;
#[path = "execute/binding.rs"]
pub(in crate::runner) mod binding;
#[path = "execute/context.rs"]
mod context;
#[path = "execute/entry.rs"]
mod entry;
#[path = "execute/host_alias.rs"]
pub(in crate::runner) mod host_alias;
#[path = "execute/json_payload.rs"]
mod json_payload;
#[path = "execute/nested.rs"]
mod nested;
#[path = "execute/pipeline.rs"]
mod pipeline;
#[path = "execute/planning.rs"]
mod planning;
#[path = "execute/preflight.rs"]
pub(in crate::runner) mod preflight;
#[path = "execute/process_run.rs"]
mod process_run;
#[path = "execute/render.rs"]
mod render;
#[path = "execute/route.rs"]
pub(in crate::runner) mod route;
#[path = "execute/routing.rs"]
mod routing;
#[path = "execute/runtime_inputs.rs"]
pub(in crate::runner) mod runtime_inputs;
#[path = "execute/selection.rs"]
mod selection;
#[path = "execute/sequence_run.rs"]
mod sequence_run;
#[path = "execute/workspace_seeded.rs"]
mod workspace_seeded;

#[cfg(test)]
pub(in crate::runner) use pipeline::command::{build_task_command, TaskCommand};
#[cfg(test)]
pub(in crate::runner) use pipeline::managed::resolve_managed_session_secret_access;
pub(in crate::runner) use pipeline::managed::{
    inline_managed_policy, release_inline_managed_environment,
};
#[cfg(test)]
pub(in crate::runner) use selection::{resolve_task_selection, SelectionResolution};
pub(in crate::runner) use sequence_run::render_script_path;

/// Whether a declared task tree can use the in-memory foreground route.
/// Preparation uses the same tree resolver as execution so it only advertises
/// gates for routes that verify them before admission.
pub(in crate::runner) fn task_tree_is_foreground(
    preflight: &planning::ExecutionPreflight,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> Result<bool, crate::runner::error::RunnerError> {
    Ok(sequence_run::resolve_tree(preflight, selection)?.foreground)
}

/// Whether the selected task starts separate Effigy task invocations. Strict
/// preparation is per invocation, so callers prepare each declared selector
/// independently and each receives its own bounded ring write.
pub(in crate::runner) fn task_tree_has_task_invocations(
    preflight: &planning::ExecutionPreflight,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> Result<bool, crate::runner::error::RunnerError> {
    Ok(sequence_run::resolve_tree(preflight, selection)?.has_task_invocations())
}
