use std::collections::BTreeSet;
use std::path::Path;

use effigy_manifest::LoadedCatalog;

pub(crate) fn deferred_builtins_for_root(root: &Path) -> BTreeSet<String> {
    let manifest_path = root.join(effigy_manifest::TASK_MANIFEST_FILE);
    crate::runner::manifest::load_task_manifest(&manifest_path)
        .ok()
        .map(|manifest| {
            let mut deferred = manifest
                .defer
                .as_ref()
                .map(|defer| defer.explicitly_deferred_builtins())
                .unwrap_or_default();
            deferred.extend(
                manifest
                    .tasks
                    .keys()
                    .filter(|task_name| is_top_level_builtin_command(task_name))
                    .cloned(),
            );
            deferred
        })
        .unwrap_or_default()
}

pub(crate) fn deferred_builtins_from_catalogs(
    catalogs: &[LoadedCatalog],
    resolved_root: &Path,
) -> BTreeSet<String> {
    catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .map(|catalog| catalog.deferred_builtins.clone())
        .unwrap_or_default()
}

fn is_top_level_builtin_command(name: &str) -> bool {
    matches!(
        name,
        "artifact"
            | "bootstrap"
            | "bundle"
            | "changelog"
            | "container"
            | "contracts"
            | "defer"
            | "demo"
            | "deploy"
            | "distribution"
            | "docs"
            | "doctor"
            | "exec"
            | "gateway"
            | "release"
            | "service"
            | "system"
            | "tasks"
            | "workspace"
    )
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
    fn deferred_builtins_ignore_legacy_root_markers_when_manifest_exists() {
        let root = temp_workspace("anchored-implicit");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.dev]\nrun = \"printf dev\"\n",
        )
        .expect("write manifest");
        fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");
        fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");

        let builtins = deferred_builtins_for_root(&root);
        assert!(!builtins.contains("release"), "got: {builtins:?}");
    }

    #[test]
    fn deferred_builtins_include_root_task_name_collisions() {
        let root = temp_workspace("implicit-collision");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.deploy]\nrun = \"printf deploy\"\n",
        )
        .expect("write manifest");

        let builtins = deferred_builtins_for_root(&root);
        assert!(builtins.contains("deploy"), "got: {builtins:?}");
    }

    #[test]
    fn deferred_builtins_merge_explicit_and_implicit_entries() {
        let root = temp_workspace("merge-explicit-implicit");
        fs::write(
            root.join("effigy.toml"),
            "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"release\"]\n[tasks.deploy]\nrun = \"printf deploy\"\n",
        )
        .expect("write manifest");

        let builtins = deferred_builtins_for_root(&root);
        assert!(builtins.contains("release"), "got: {builtins:?}");
        assert!(builtins.contains("deploy"), "got: {builtins:?}");
    }
}
