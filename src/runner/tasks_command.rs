use crate::resolver::resolve_target_root;
use crate::TasksArgs;

use super::catalog::discover_catalogs_allow_missing;
use super::tasks_diagnostics::build_catalog_diagnostics;
use super::tasks_listing::render_tasks_listing;
use super::tasks_probe::build_resolve_probe;
use super::RunnerError;

pub(super) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd, args.repo_override.clone())?;
    let catalogs = discover_catalogs_allow_missing(&resolved.resolved_root)?;
    let precedence = super::command_context::task_selection_precedence_notes();

    let resolve_probe = build_resolve_probe(args.resolve_selector.clone(), &catalogs)?;

    let (ordered_catalogs, catalog_diagnostics) = build_catalog_diagnostics(&catalogs);

    render_tasks_listing(
        &args,
        &catalogs,
        &ordered_catalogs,
        &catalog_diagnostics,
        &precedence,
        &resolve_probe,
        &resolved.resolved_root,
    )
}
