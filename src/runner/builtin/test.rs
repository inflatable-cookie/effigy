use crate::TaskInvocation;
use std::path::Path;

use super::super::model::catalog::{LoadedCatalog, TaskRuntimeArgs, TaskSelector};
use crate::runner::error::RunnerError;

mod execution;
mod planning;
mod render;
mod suite_selection;

pub(super) fn try_run_builtin_test(
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let (flags, passthrough) = planning::extract_builtin_test_flags(&runtime_args.passthrough);
    let targets = planning::resolve_builtin_test_targets(selector, resolved_root, catalogs)?;
    let runnable = planning::collect_builtin_test_runnable_targets(&targets);
    if runnable.is_empty() {
        return Ok(None);
    }
    let suite_selection = match suite_selection::select_builtin_test_suite(runnable, passthrough) {
        Ok(selection) => selection,
        Err(selection_error) => {
            return render::render_suite_selection_failure(
                task,
                resolved_root,
                flags,
                selection_error,
            );
        }
    };

    if flags.plan_mode {
        return render::render_builtin_test_plan(
            task,
            resolved_root,
            &targets,
            suite_selection.requested_suite.as_deref(),
            &suite_selection.passthrough,
            suite_selection.runnable.len(),
            flags,
        )
        .map(Some);
    }

    let runnable = planning::apply_passthrough_to_runnable(
        suite_selection.runnable,
        &suite_selection.passthrough,
    );
    let max_parallel = planning::builtin_test_max_parallel(catalogs, resolved_root);
    let should_tui = execution::should_run_builtin_test_tui(flags.tui, runnable.len());
    let results = if should_tui {
        execution::run_builtin_test_targets_tui(runnable)?
    } else {
        execution::run_builtin_test_targets_parallel(runnable, max_parallel, flags.output_json)?
    };
    render::finalize_builtin_test_outcome(
        &results,
        &targets,
        suite_selection.requested_suite.as_deref(),
        &suite_selection.passthrough,
        flags.verbose_results,
        flags.output_json,
    )
}

#[cfg(test)]
pub(super) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    planning::builtin_test_max_parallel(catalogs, resolved_root)
}

#[derive(Debug)]
struct BuiltinTestExecResult {
    name: String,
    runner: String,
    root: std::path::PathBuf,
    command: String,
    success: bool,
    code: Option<i32>,
}
