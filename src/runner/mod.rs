use std::collections::BTreeSet;
use std::path::Path;

mod builtin;
mod cache;
mod catalog;
mod changelog_command;
mod command_context;
mod contracts_command;
mod deferral;
mod distribution_command;
mod docs_command;
mod doctor;
mod entrypoints;
mod env_schema_support;
mod error;
mod execute;
mod locking;
mod managed;
mod manifest;
mod model;
mod release_command;
mod render;
mod scan;
mod tasks_command;
mod tasks_listing;
mod tasks_probe;
mod tasks_view;
#[cfg(test)]
mod test_support;
mod tooling;
mod util;

use manifest::TaskManifest;
use model::{
    catalog::{LoadedCatalog, TaskSelector},
    constants::DEFAULT_MANAGED_SHELL_RUN,
    managed::ManagedProcessSpec,
};

pub use entrypoints::{resolve_command_root, run_command};
pub use error::RunnerError;

pub(crate) fn explicitly_deferred_builtins_for_root(root: &Path) -> BTreeSet<String> {
    let manifest_path = root.join(model::constants::TASK_MANIFEST_FILE);
    manifest::load_task_manifest(&manifest_path)
        .ok()
        .and_then(|manifest| {
            manifest
                .defer
                .as_ref()
                .map(|defer| defer.explicitly_deferred_builtins())
        })
        .unwrap_or_default()
}

pub(in crate::runner) fn explicitly_deferred_builtins_from_catalogs(
    catalogs: &[LoadedCatalog],
    resolved_root: &Path,
) -> BTreeSet<String> {
    catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .map(|catalog| catalog.deferred_builtins.clone())
        .unwrap_or_default()
}

pub(crate) fn builtin_can_be_explicitly_deferred(name: &str) -> bool {
    model::constants::EXPLICITLY_DEFERRABLE_COMMAND_BUILTINS.contains(&name)
}

#[cfg(test)]
#[path = "../tests/runner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/catalogs_contract_tests.rs"]
mod catalogs_contract_tests;

#[cfg(test)]
#[path = "../tests/json_contract_tests/mod.rs"]
mod json_contract_tests;

#[cfg(test)]
#[path = "../tests/task_ref_parser_tests.rs"]
mod task_ref_parser_tests;

#[cfg(test)]
#[path = "../tests/cache_tests/mod.rs"]
mod cache_tests;
