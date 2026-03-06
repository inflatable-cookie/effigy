use std::path::Path;

use crate::TaskInvocation;

#[path = "cache/dispatch.rs"]
mod dispatch;
#[path = "cache/output.rs"]
mod output;
#[path = "cache/request.rs"]
mod request;
#[path = "cache/selection.rs"]
mod selection;

use super::super::model::catalog::{LoadedCatalog, TaskRuntimeArgs};
use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_text;
use crate::runner::error::RunnerError;
use request::{parse_cache_request, render_cache_help, CacheRequest};

pub(super) fn run_builtin_cache(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<Option<String>, RunnerError> {
    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| render_builtin_help_text("cache", render_cache_help(), output_json),
        parse_cache_request,
        |request: CacheRequest| match request {
            CacheRequest::Inspect(request) => {
                dispatch::run_inspect(target_root, catalogs, invocation_cwd, request)
            }
            CacheRequest::Invalidate(request) => {
                dispatch::run_invalidate(target_root, catalogs, invocation_cwd, request)
            }
        },
    )
}
