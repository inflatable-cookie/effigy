//! The one selector resolver (contract 052 I-ID1, D1-10). Execution, the
//! `--plan` fallbacks and `effigy tasks --resolve` all call
//! [`resolve_route`]; nothing else decides where a selector goes. It runs
//! nothing: each route names what execution would do, and the caller does it.

use effigy_cli::TaskInvocation;
use effigy_execution::ExecutionSelectionPlan;
use effigy_manifest::{DeferredCommand, TaskSelection};
use effigy_routing::select_catalog_and_task;
use effigy_tasks::TaskSelector;

use super::planning::ExecutionPreflight;
use super::selection::build_execution_selection_plan;
use crate::runner::command_context::active_runtime_context;
use crate::runner::deferral::{
    deferred_builtins_from_catalogs, select_deferral, should_attempt_deferral,
};
use crate::runner::error::RunnerError;

pub(in crate::runner) enum Route<'a> {
    /// A declared task in a member catalog.
    Task {
        selection: TaskSelection<'a>,
        plan: Box<ExecutionSelectionPlan>,
    },
    /// A built-in command routed by name (`test`, `tasks`, `watch`, ...).
    Builtin,
    /// A container exec alias of the default exec surface.
    ExecAlias,
    /// A catalog's `[defer]` command receives the request.
    Defer {
        deferral: DeferredCommand,
        cause: RunnerError,
    },
}

impl Route<'_> {
    pub(in crate::runner) fn kind(&self) -> &'static str {
        match self {
            Route::Task { .. } => "task",
            Route::Builtin => "builtin",
            Route::ExecAlias => "exec-alias",
            Route::Defer { .. } => "defer",
        }
    }
}

/// Where `task` goes, in the one precedence order: a declared task; then,
/// only when no task matched, a removed name's error, a built-in, an exec
/// alias and finally deferral (deferral before exec aliases inside a
/// container handoff). `--plan` only falls back to `test` and to a remapped
/// built-in's deferral. An error is the selection error itself.
pub(in crate::runner) fn resolve_route<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<Route<'a>, RunnerError> {
    resolve_route_from(task, preflight, true)
}

/// The route after a built-in that declined the request at run time (for
/// example `test` with no detected suite): the same order, without built-ins.
pub(in crate::runner) fn resolve_route_past_builtin<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<Route<'a>, RunnerError> {
    resolve_route_from(task, preflight, false)
}

fn resolve_route_from<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    builtins: bool,
) -> Result<Route<'a>, RunnerError> {
    let error: RunnerError = match select_catalog_and_task(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    ) {
        Ok(selection) => {
            let plan = build_execution_selection_plan(preflight, &selection);
            return Ok(Route::Task {
                selection,
                plan: Box::new(plan),
            });
        }
        Err(error) => error.into(),
    };
    if let Some(removed) = removed_builtin_invocation_error(&preflight.selector) {
        return Err(removed);
    }
    if preflight.plan {
        // `effigy test --plan` is the test built-in's own plan.
        if builtins && preflight.selector.task_name == "test" && builtin_handles(preflight) {
            return Ok(Route::Builtin);
        }
        // A remapped built-in (`[defer].builtins = ["release"]`) keeps
        // `--plan` as its own flag; nothing else falls back under `--plan`.
        if deferred_builtins_from_catalogs(&preflight.catalogs, &preflight.resolved.resolved_root)
            .contains(&preflight.selector.task_name)
        {
            return deferral_route(preflight, error);
        }
        return Err(error);
    }
    if builtins && builtin_handles(preflight) {
        return Ok(Route::Builtin);
    }
    let prefer_deferral = active_runtime_context()
        .is_some_and(|context| context.container().inside_container_handoff);
    if prefer_deferral && should_attempt_deferral(&error) {
        if let Some(deferral) = select_deferral(
            &preflight.selector,
            &preflight.catalogs,
            &preflight.invocation_cwd,
            &preflight.resolved.resolved_root,
        ) {
            return Ok(Route::Defer {
                deferral,
                cause: error,
            });
        }
    }
    if exec_alias_handles(task, preflight, &error)? {
        return Ok(Route::ExecAlias);
    }
    deferral_route(preflight, error)
}

/// Deferral when it applies, otherwise the selection error itself.
fn deferral_route<'a>(
    preflight: &ExecutionPreflight,
    error: RunnerError,
) -> Result<Route<'a>, RunnerError> {
    if !should_attempt_deferral(&error) {
        return Err(error);
    }
    match select_deferral(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
        &preflight.resolved.resolved_root,
    ) {
        Some(deferral) => Ok(Route::Defer {
            deferral,
            cause: error,
        }),
        None => Err(error),
    }
}

fn builtin_handles(preflight: &ExecutionPreflight) -> bool {
    effigy_builtin::builtin_handles(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    )
}

fn exec_alias_handles(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
    selection_error: &RunnerError,
) -> Result<bool, RunnerError> {
    if preflight.selector.prefix.is_some()
        || !matches!(
            selection_error,
            RunnerError::TaskNotFound { .. } | RunnerError::TaskNotFoundAny { .. }
        )
    {
        return Ok(false);
    }
    crate::runner::exec_command::exec_alias_declared(
        &preflight.resolved.resolved_root,
        &task.name,
        &preflight.runtime_args_exec.passthrough,
    )
}

fn removed_builtin_invocation_error(selector: &TaskSelector) -> Option<RunnerError> {
    if !matches!(selector.task_name.as_str(), "repo-pulse" | "health") {
        return None;
    }
    let request = selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone());
    Some(RunnerError::task_invocation(format!(
        "`{request}` is no longer a built-in command. Use `effigy doctor` for consolidated health checks, or define `tasks.health` in your manifest for project-owned checks."
    )))
}
