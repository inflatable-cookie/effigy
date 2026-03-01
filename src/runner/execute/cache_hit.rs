use std::path::Path;

use crate::resolver::ResolvedTarget;

use super::super::render::render_task_resolution_trace;
use super::super::{RunnerError, TaskSelection, TaskSelector};

pub(super) struct CacheHitContext<'a> {
    pub(super) resolved: &'a ResolvedTarget,
    pub(super) selector: &'a TaskSelector,
    pub(super) selection: &'a TaskSelection<'a>,
    pub(super) repo_for_task: &'a Path,
    pub(super) command: &'a str,
    pub(super) reason: &'a str,
    pub(super) fingerprint: &'a str,
}

pub(super) fn render_cache_hit_output(
    output_json: bool,
    verbose_root: bool,
    context: &CacheHitContext<'_>,
) -> Result<String, RunnerError> {
    if output_json {
        return super::json_payload::render_task_cache_hit_json(
            &context.selector.task_name,
            context.selector,
            context.repo_for_task,
            context.command,
            context.reason,
            context.fingerprint,
        );
    }

    if verbose_root {
        let trace = render_task_resolution_trace(
            context.resolved,
            context.selector,
            context.selection,
            context.repo_for_task,
            context.command,
        );
        return Ok(format!(
            "{trace}\ncache: hit ({})\nfingerprint: {}",
            context.reason, context.fingerprint
        ));
    }

    Ok(format!(
        "cache hit: skipped `{}` ({reason})",
        context.selector.task_name,
        reason = context.reason
    ))
}
