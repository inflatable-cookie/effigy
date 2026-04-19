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
    if !has_root_manifest(root) || !implicit_root_deferral_is_enabled(root) {
        return BTreeSet::new();
    }
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
    if !has_root_manifest(root) {
        return BTreeSet::new();
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

fn has_root_manifest(root: &Path) -> bool {
    root.join(effigy_manifest::TASK_MANIFEST_FILE).is_file()
}

#[cfg(test)]
mod tests {
    use super::deferred_builtins_for_root;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace(name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("effigy-deferral-builtins-{name}-{ts}"));
        fs::create_dir_all(&root).expect("mkdir workspace");
        root
    }

    #[test]
    fn deferred_builtins_ignore_unanchored_directories() {
        let root = temp_workspace("unanchored");
        fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");
        fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");

        let builtins = deferred_builtins_for_root(&root);
        assert!(builtins.is_empty(), "got: {builtins:?}");
    }

    #[test]
    fn deferred_builtins_keep_implicit_root_deferral_when_manifest_exists() {
        let root = temp_workspace("anchored-implicit");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.dev]\nrun = \"printf dev\"\n",
        )
        .expect("write manifest");
        fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");
        fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");

        let builtins = deferred_builtins_for_root(&root);
        assert!(builtins.contains("release"), "got: {builtins:?}");
    }
}
