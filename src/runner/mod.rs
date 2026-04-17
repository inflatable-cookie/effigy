use std::collections::BTreeSet;
use std::path::Path;

mod bootstrap_command;
mod builtin_ports;
mod cache;
mod changelog_command;
mod command_context;
mod container_command;
mod contracts_command;
mod deferral;
mod demo_command;
mod distribution_command;
mod docs_command;
mod doctor;
mod entrypoints;
mod env_schema_support;
mod error;
mod execute;
mod locking;
mod manifest;
mod model;
mod release_command;
mod render;
mod script_command;
mod tasks_command;
mod tasks_listing;
mod tasks_probe;
mod tasks_view;
#[cfg(test)]
mod test_support;
mod tooling;
mod util;

use effigy_manifest::LoadedCatalog;
use manifest::TaskManifest;
use model::constants::IMPLICITLY_DEFERRED_COMMAND_BUILTINS;

pub use entrypoints::{resolve_command_root, run_command};
pub use error::RunnerError;

fn implicit_deferred_builtins() -> BTreeSet<String> {
    IMPLICITLY_DEFERRED_COMMAND_BUILTINS
        .iter()
        .map(|name| (*name).to_owned())
        .collect()
}

fn has_explicit_deferral_catalog(catalogs: &[LoadedCatalog]) -> bool {
    catalogs.iter().any(|catalog| catalog.defer_run.is_some())
}

fn implicit_deferred_builtins_for_root_with_catalogs(
    root: &Path,
    catalogs: &[LoadedCatalog],
) -> BTreeSet<String> {
    if deferral::implicit_root_deferral_is_enabled(root) && !has_explicit_deferral_catalog(catalogs)
    {
        return implicit_deferred_builtins();
    }
    BTreeSet::new()
}

fn implicit_deferred_builtins_for_root(root: &Path) -> BTreeSet<String> {
    let catalogs = effigy_routing::discover_catalogs_allow_missing(root).unwrap_or_default();
    implicit_deferred_builtins_for_root_with_catalogs(root, &catalogs)
}

pub(crate) fn deferred_builtins_for_root(root: &Path) -> BTreeSet<String> {
    let manifest_path = root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let explicit = manifest::load_task_manifest(&manifest_path)
        .ok()
        .and_then(|manifest| {
            manifest
                .defer
                .as_ref()
                .map(|defer| defer.explicitly_deferred_builtins())
        })
        .unwrap_or_default();
    if !explicit.is_empty() {
        return explicit;
    }
    implicit_deferred_builtins_for_root(root)
}

pub(in crate::runner) fn deferred_builtins_from_catalogs(
    catalogs: &[LoadedCatalog],
    resolved_root: &Path,
) -> BTreeSet<String> {
    let explicit = catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .map(|catalog| catalog.deferred_builtins.clone())
        .unwrap_or_default();
    if !explicit.is_empty() {
        return explicit;
    }
    implicit_deferred_builtins_for_root_with_catalogs(resolved_root, catalogs)
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
