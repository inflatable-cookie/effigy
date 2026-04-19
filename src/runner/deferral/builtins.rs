use std::collections::BTreeSet;
use std::path::Path;

use effigy_manifest::LoadedCatalog;

use super::policy::IMPLICITLY_DEFERRED_COMMAND_BUILTINS;
use super::select::implicit_root_deferral_is_enabled;

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
    if implicit_root_deferral_is_enabled(root) && !has_explicit_deferral_catalog(catalogs) {
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
    let explicit = crate::runner::manifest::load_task_manifest(&manifest_path)
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

pub(crate) fn deferred_builtins_from_catalogs(
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
