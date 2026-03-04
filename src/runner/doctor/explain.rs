use std::path::PathBuf;

use crate::resolver::resolve_target_root;
use crate::TaskInvocation;

#[path = "explain/analysis.rs"]
mod analysis;
#[path = "explain/contracts.rs"]
mod contracts;
#[path = "explain/render.rs"]
mod render;

use super::super::catalog::{discover_catalogs, select_catalog_and_task};
use super::super::util::parse_task_selector;
use super::{CatalogSelectionMode, RunnerError};
pub(super) const DEFERRAL_NOT_CONSIDERED_REASON: &str =
    "deferral was not considered because the selection outcome does not trigger deferral";
pub(super) const DEFERRAL_SELECTED_REASON: &str =
    "deferral was selected from configured or implicit fallback routing";
pub(super) const DEFERRAL_NOT_FOUND_REASON: &str =
    "deferral was considered but no eligible fallback route was found";

pub(super) fn run_doctor_explain(
    request: TaskInvocation,
    repo_override: Option<PathBuf>,
    output_json: bool,
    fix: bool,
    verbose: bool,
) -> Result<String, RunnerError> {
    if fix {
        return Err(RunnerError::task_invocation(
            "`--fix` is not supported with explain mode (`effigy doctor <task> <args>`).",
        ));
    }

    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd.clone(), repo_override)?;
    let catalogs = discover_catalogs(&resolved.resolved_root)?;
    let selector = parse_task_selector(&request.name)?;

    let candidates = analysis::candidate_catalogs(&catalogs, &selector, &cwd);
    let selection_result = select_catalog_and_task(&selector, &catalogs, &cwd);
    let selection = analysis::compute_selection_outcome(&selection_result);
    let deferral = analysis::compute_deferral_outcome(
        &selection_result,
        &selector,
        &catalogs,
        &cwd,
        &resolved.resolved_root,
    );

    if output_json {
        return render::render_explain_json(
            &request,
            &resolved,
            &selector.task_name,
            &selection,
            &deferral,
            &candidates,
        );
    }

    render::render_explain_text(
        &request,
        &resolved,
        &selection,
        &deferral,
        &candidates,
        &catalogs,
        verbose,
    )
}
