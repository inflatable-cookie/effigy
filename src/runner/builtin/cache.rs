use std::path::Path;

use crate::TaskInvocation;

#[path = "cache/args.rs"]
mod args;
#[path = "cache/dispatch.rs"]
mod dispatch;
#[path = "cache/output.rs"]
mod output;

use super::super::{LoadedCatalog, RunnerError, TaskRuntimeArgs};
use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheCommand {
    Inspect,
    Invalidate,
}

struct CacheArgs {
    command: CacheCommand,
    output_json: bool,
    invalidate_all: bool,
    selectors: Vec<String>,
}

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
        |output_json| render_builtin_help_text("cache", args::render_cache_help(), output_json),
        args::parse_cache_args,
        |parsed: CacheArgs| match parsed.command {
            CacheCommand::Inspect => dispatch::run_inspect(
                target_root,
                catalogs,
                invocation_cwd,
                parsed.output_json,
                parsed.selectors,
            ),
            CacheCommand::Invalidate => dispatch::run_invalidate(
                target_root,
                catalogs,
                invocation_cwd,
                parsed.output_json,
                parsed.invalidate_all,
                parsed.selectors,
            ),
        },
    )
}
