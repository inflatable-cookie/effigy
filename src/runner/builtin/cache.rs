use std::path::Path;

use crate::TaskInvocation;

#[path = "cache/args.rs"]
mod args;
#[path = "cache/dispatch.rs"]
mod dispatch;
#[path = "cache/output.rs"]
mod output;

use super::super::{LoadedCatalog, RunnerError, TaskRuntimeArgs};
use super::reject_verbose_root_for_builtin;

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
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;

    if runtime_args
        .passthrough
        .iter()
        .any(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Some(args::render_cache_help()));
    }

    let parsed = args::parse_cache_args(&runtime_args.passthrough)?;

    match parsed.command {
        CacheCommand::Inspect => dispatch::run_inspect(
            task,
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
    }
}
