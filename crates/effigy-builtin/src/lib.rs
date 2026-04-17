//! Built-in command layer for Effigy.
//!
//! Extracted from `src/runner/builtin/**` under card 250. Owns the
//! registry, dispatcher, every built-in arm (doctor, tasks, test,
//! scan, config, watch, unlock, cache, completion, init, migrate,
//! help, …) plus the test-support fixtures.
//!
//! The narrow [`BuiltinError`] boundary lifts to `RunnerError` at the
//! runner's edge via `impl From<BuiltinError> for RunnerError` in
//! `src/runner/error.rs`. Runtime reach-backs go through the
//! [`BuiltinRuntimePorts`] trait, implemented by `RunnerBuiltinPorts`
//! in the runner tree.

use std::path::{Path, PathBuf};

use effigy_cli::TaskInvocation;
use effigy_manifest::LoadedCatalog;
use effigy_routing::resolve_catalog_by_prefix;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

mod arg_parser;
mod cache;
mod command_spec;
mod completion;
mod config;
pub mod constants;
mod doc_render;
mod doctor;
mod error;
mod help;
mod help_text;
mod init;
mod migrate;
pub mod ports;
mod registry;
mod response;
mod scan;
mod support;
mod tasks;
mod test;
pub mod test_support;
mod text_doc;
mod unlock;
mod watch;

pub use constants::{BUILTIN_TASKS, DEFAULT_BUILTIN_TEST_MAX_PARALLEL};
pub use error::BuiltinError;
pub use ports::{BuiltinLockGuards, BuiltinRuntimePorts, LockScope, TaskCacheEntry, UnlockResult};

pub(crate) use support::{
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

pub fn try_run_builtin_task(
    ports: &dyn BuiltinRuntimePorts,
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<Option<String>, BuiltinError> {
    let Some(entry) = registry::builtin_registry_entry(&selector.task_name) else {
        return Ok(None);
    };

    let Some(target_root) =
        resolve_builtin_task_target_root(selector, resolved_root, catalogs, invocation_cwd)
    else {
        return Ok(None);
    };

    entry.run(
        ports,
        selector,
        task,
        runtime_args,
        &target_root,
        catalogs,
        invocation_cwd,
    )
}
