use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;
use effigy_cli::TaskInvocation;
use effigy_manifest::LoadedCatalog;
use effigy_routing::resolve_catalog_by_prefix;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

mod arg_parser;
mod cache;
mod command_spec;
mod completion;
mod config;
mod doc_render;
mod doctor;
mod help;
mod help_text;
mod init;
mod migrate;
mod registry;
mod response;
mod scan;
mod support;
mod tasks;
mod test;
#[cfg(test)]
pub(in crate::runner) mod test_support;
mod text_doc;
mod unlock;
mod watch;

pub(super) use support::{
    ensure_no_unknown_builtin_args, ensure_no_unknown_builtin_args_with_prefix,
    has_builtin_help_flag, has_builtin_json_flag, render_builtin_general_help,
    render_builtin_help_text, render_builtin_help_topic,
};

fn resolve_builtin_task_target_root(
    selector: &TaskSelector,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Option<PathBuf> {
    if let Some(prefix) = selector.prefix.as_ref() {
        return resolve_catalog_by_prefix(prefix, catalogs, invocation_cwd)
            .map(|catalog| catalog.catalog_root.clone());
    }
    Some(resolved_root.to_path_buf())
}

pub(super) fn try_run_builtin_task(
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<Option<String>, RunnerError> {
    let Some(entry) = registry::builtin_registry_entry(&selector.task_name) else {
        return Ok(None);
    };

    let Some(target_root) =
        resolve_builtin_task_target_root(selector, resolved_root, catalogs, invocation_cwd)
    else {
        return Ok(None);
    };

    entry.run(
        selector,
        task,
        runtime_args,
        &target_root,
        catalogs,
        invocation_cwd,
    )
}
