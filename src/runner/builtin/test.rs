use crate::TaskInvocation;
use std::path::Path;

use super::super::{LoadedCatalog, RunnerError, TaskRuntimeArgs, TaskSelector};

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
    let targets = planning::resolve_builtin_test_targets(selector, resolved_root, catalogs);
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
    let mut failures = results
        .iter()
        .filter_map(|result| {
            if result.success {
                None
            } else {
                Some((result.name.clone(), result.code))
            }
        })
        .collect::<Vec<(String, Option<i32>)>>();
    failures.sort_by(|a, b| a.0.cmp(&b.0));
    let rendered = render::render_builtin_test_results(&results, flags.verbose_results)?;
    let rendered_json = if flags.output_json {
        Some(render::render_builtin_test_results_json(
            &results,
            &targets,
            suite_selection.requested_suite.as_deref(),
            &suite_selection.passthrough,
        )?)
    } else {
        None
    };
    if failures.is_empty() {
        if let Some(json) = rendered_json {
            Ok(Some(json))
        } else {
            Ok(Some(rendered))
        }
    } else if let Some(json) = rendered_json {
        Err(RunnerError::BuiltinTestNonZero {
            failures,
            rendered: json,
        })
    } else {
        let rendered = render::append_builtin_test_filter_hint(
            rendered,
            &results,
            suite_selection.requested_suite.as_deref(),
            &suite_selection.passthrough,
        );
        Err(RunnerError::BuiltinTestNonZero { failures, rendered })
    }
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
