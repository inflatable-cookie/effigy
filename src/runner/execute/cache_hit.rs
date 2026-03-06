use super::context::ExecutionTaskContext;
use crate::runner::error::RunnerError;

pub(super) fn render_cache_hit_output(
    output_json: bool,
    verbose_root: bool,
    context: &ExecutionTaskContext<'_>,
    reason: &str,
    fingerprint: &str,
) -> Result<String, RunnerError> {
    if output_json {
        return super::json_payload::render_task_cache_hit_json(
            &context.selector.task_name,
            context.selector,
            context.repo_for_task(),
            context.command(),
            reason,
            fingerprint,
        );
    }

    if verbose_root {
        return Ok(cache_hit_verbose_output(context, reason, fingerprint));
    }

    Ok(cache_hit_short_output(context, reason))
}

fn cache_hit_verbose_output(
    context: &ExecutionTaskContext<'_>,
    reason: &str,
    fingerprint: &str,
) -> String {
    format!(
        "{}\ncache: hit ({})\nfingerprint: {}",
        context.render_resolution_trace(),
        reason,
        fingerprint
    )
}

fn cache_hit_short_output(context: &ExecutionTaskContext<'_>, reason: &str) -> String {
    format!(
        "cache hit: skipped `{}` ({reason})",
        context.selector.task_name,
        reason = reason
    )
}
