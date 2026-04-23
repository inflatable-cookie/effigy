use std::path::PathBuf;

use effigy_cli::TasksArgs;
use effigy_tasks::{
    list_tasks, probe_task_resolution, render_task_listing_json, render_task_listing_text,
    ListTasksRequest, ProbeTaskResolutionRequest,
};

use super::super::command_context::{
    current_working_dir, resolve_repo_root, task_selection_precedence_notes,
};
use super::super::deferred_builtins_from_catalogs;
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
use effigy_routing::discover_catalogs_allow_missing;

pub(super) struct PreparedTasksCommand {
    resolved_root: PathBuf,
    catalogs: Vec<LoadedCatalog>,
    precedence: Vec<String>,
    resolve_probe: Option<serde_json::Value>,
}

impl PreparedTasksCommand {
    pub(super) fn render(&self, args: &TasksArgs) -> Result<String, RunnerError> {
        let deferred_builtins =
            deferred_builtins_from_catalogs(&self.catalogs, &self.resolved_root);
        let listing = list_tasks(
            ListTasksRequest {
                filter: args.task_name.as_deref(),
                pretty_json: args.pretty_json,
                resolved_root: &self.resolved_root,
                precedence: &self.precedence,
                resolve_probe: self.resolve_probe.clone(),
                deferred_builtins: &deferred_builtins,
            },
            &self.catalogs,
        )
        .map_err(map_effigy_tasks_error)?;

        if args.output_json {
            render_task_listing_json(&listing, args.pretty_json).map_err(map_effigy_tasks_error)
        } else {
            render_task_listing_text(&listing).map_err(map_effigy_tasks_error)
        }
    }
}

pub(in crate::runner) fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
    prepare_tasks_command(&args)?.render(&args)
}

pub(super) fn prepare_tasks_command(args: &TasksArgs) -> Result<PreparedTasksCommand, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd.clone(), args.repo_override.clone())?;
    let catalogs = discover_catalogs_allow_missing(&resolved.resolved_root)?;
    let deferred_builtins = deferred_builtins_from_catalogs(&catalogs, &resolved.resolved_root);
    let resolve_probe = probe_task_resolution(
        ProbeTaskResolutionRequest {
            raw_selector: args.resolve_selector.as_deref(),
            cwd: &cwd,
            deferred_builtins: &deferred_builtins,
        },
        &catalogs,
    )
    .map_err(map_effigy_tasks_error)?
    .map(|probe| probe.into_json());

    Ok(PreparedTasksCommand {
        resolved_root: resolved.resolved_root,
        catalogs,
        precedence: task_selection_precedence_notes(),
        resolve_probe,
    })
}

fn map_effigy_tasks_error(error: effigy_tasks::EffigyTasksError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}
