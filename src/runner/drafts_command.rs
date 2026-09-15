use effigy_cli::DraftsArgs;
use effigy_manifest::ManifestDraftDate;
use effigy_routing::load_effective_catalogs_allow_missing;
use effigy_tasks::{
    list_drafts, render_draft_listing_json, render_draft_listing_text, ListDraftsRequest,
};

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::error::RunnerError;

/// Render `effigy drafts`: the lifecycle-labelled draft inventory.
///
/// The evaluation date is taken once here and passed into the pure listing so
/// fixtures can inject a controlled date.
pub(in crate::runner) fn run_drafts(args: DraftsArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let catalogs = load_effective_catalogs_allow_missing(&context.resolved.resolved_root)?;
    let listing = list_drafts(
        ListDraftsRequest {
            filter: args.filter.as_deref(),
            pretty_json: args.pretty_json,
            resolved_root: &context.resolved.resolved_root,
            today: ManifestDraftDate::today_local(),
        },
        &catalogs,
    )
    .map_err(map_effigy_tasks_error)?;

    if args.output_json {
        render_draft_listing_json(&listing, args.pretty_json).map_err(map_effigy_tasks_error)
    } else {
        render_draft_listing_text(&listing).map_err(map_effigy_tasks_error)
    }
}

fn map_effigy_tasks_error(error: effigy_tasks::EffigyTasksError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}
