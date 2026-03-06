use std::path::PathBuf;

use crate::TasksArgs;

use super::super::catalog::discover_catalogs_allow_missing;
use super::super::command_context::{
    current_working_dir, resolve_repo_root, task_selection_precedence_notes,
};
use super::super::model::catalog::LoadedCatalog;
use super::super::tasks_listing::{build_catalog_diagnostics, render_tasks_listing};
use super::super::tasks_probe::build_resolve_probe;
use crate::runner::error::RunnerError;

pub(super) struct PreparedTasksCommand {
    resolved_root: PathBuf,
    catalogs: Vec<LoadedCatalog>,
    precedence: Vec<String>,
    resolve_probe: Option<serde_json::Value>,
}

impl PreparedTasksCommand {
    pub(super) fn render(&self, args: &TasksArgs) -> Result<String, RunnerError> {
        let (ordered_catalogs, catalog_diagnostics) = build_catalog_diagnostics(&self.catalogs);
        render_tasks_listing(
            args,
            &self.catalogs,
            &ordered_catalogs,
            &catalog_diagnostics,
            &self.precedence,
            &self.resolve_probe,
            &self.resolved_root,
        )
    }
}

pub(in crate::runner) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
    prepare_tasks_command(&args)?.render(&args)
}

pub(super) fn prepare_tasks_command(args: &TasksArgs) -> Result<PreparedTasksCommand, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let catalogs = discover_catalogs_allow_missing(&resolved.resolved_root)?;
    let resolve_probe = build_resolve_probe(args.resolve_selector.clone(), &catalogs)?;

    Ok(PreparedTasksCommand {
        resolved_root: resolved.resolved_root,
        catalogs,
        precedence: task_selection_precedence_notes(),
        resolve_probe,
    })
}
