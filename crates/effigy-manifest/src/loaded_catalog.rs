//! Loaded-catalog shapes used by task-runtime consumers.
//!
//! `LoadedCatalog` bundles a parsed `TaskManifest` with the metadata a
//! runtime needs to locate and interpret it: alias, catalog root, manifest
//! path, defer spec, and composition depth. `TaskSelection` names a
//! chosen task inside a loaded catalog; `DeferredCommand` carries the
//! shell fallback template when a selector doesn't resolve locally.
//!
//! These types previously lived inside the runner binary
//! (`src/runner/model/catalog.rs`). They moved here so the
//! `effigy-managed` extraction can depend on them without the runner
//! having to expose internal module paths.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_core::task_selection::{CatalogSelectionMode, TaskSelector};

use crate::task_runtime::{ManifestTask, ManifestTaskRunIn};
use crate::TaskManifest;

/// Callback signature for resolving a `TaskSelector` against a slice
/// of `LoadedCatalog`. The runner owns the routing implementation
/// (currently `src/runner/catalog::select_catalog_and_task`); managed
/// task orchestration takes this as a parameter rather than importing
/// the routing core directly so it stays extract-ready.
///
/// The error channel is plain `String` — both runners and managed
/// orchestration have their own error enums with `task_invocation`
/// constructors that accept strings, so callers wrap at the boundary.
pub type TaskResolverFn<'r> = &'r dyn for<'c> Fn(
    &TaskSelector,
    &'c [LoadedCatalog],
    &Path,
) -> Result<TaskSelection<'c>, String>;

#[derive(Debug)]
pub struct LoadedCatalog {
    pub alias: String,
    pub catalog_root: PathBuf,
    pub manifest_path: PathBuf,
    pub bundle_root: Option<PathBuf>,
    pub manifest: TaskManifest,
    /// Digest of the effective composed value used to parse this catalog.
    /// Synthetic catalogs and readers without composition evidence use None.
    pub effective_digest: Option<effigy_kernel::id::Digest>,
    pub defer_run: Option<String>,
    pub deferred_builtins: BTreeSet<String>,
    pub depth: usize,
}

impl LoadedCatalog {
    /// Build a catalog fixture for tests.
    ///
    /// Everything a test does not vary is derived here, so a new
    /// `LoadedCatalog` field never again edits every fixture literal across
    /// the workspace: the manifest path is the catalog root's own manifest,
    /// and bundle, deferral, and deferred-builtin state stay empty.
    pub fn for_test(
        alias: impl Into<String>,
        catalog_root: impl Into<PathBuf>,
        manifest: TaskManifest,
        depth: usize,
    ) -> Self {
        let catalog_root = catalog_root.into();
        Self {
            alias: alias.into(),
            manifest_path: catalog_root.join(crate::TASK_MANIFEST_FILE),
            catalog_root,
            bundle_root: None,
            manifest,
            effective_digest: None,
            defer_run: None,
            deferred_builtins: BTreeSet::new(),
            depth,
        }
    }
}

#[derive(Debug)]
pub struct TaskSelection<'a> {
    pub catalog: &'a LoadedCatalog,
    pub task: &'a ManifestTask,
    pub mode: CatalogSelectionMode,
    pub evidence: Vec<String>,
    /// Published or draft surface the selection resolved in.
    pub surface: effigy_core::task_selection::TaskSurface,
}

/// Returns the deepest catalog whose root contains `catalog_root` and whose
/// manifest declares an `[env_schema]` section, including the catalog at
/// `catalog_root` itself. Child catalogs without their own `env_schema`
/// inherit the nearest ancestor's; the schema path still resolves against
/// the catalog that declared it.
pub fn env_schema_declaring_catalog<'a>(
    catalogs: &'a [LoadedCatalog],
    catalog_root: &Path,
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .filter(|catalog| {
            catalog_root.starts_with(&catalog.catalog_root) && catalog.manifest.env_schema.is_some()
        })
        .max_by_key(|catalog| catalog.depth)
}

#[derive(Debug, Clone)]
pub struct DeferredCommand {
    pub template: String,
    pub working_dir: PathBuf,
    pub source: String,
    pub run_in: ManifestTaskRunIn,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn loaded_catalog(alias: &str, root: &str, manifest_body: &str, depth: usize) -> LoadedCatalog {
        LoadedCatalog::for_test(
            alias,
            PathBuf::from(root),
            toml::from_str(manifest_body).expect("parse manifest"),
            depth,
        )
    }

    #[test]
    fn a_retired_catalog_graph_table_is_ignored_with_its_replacement() {
        for fields in [
            "",
            "segmented = true",
            "segmented = true\nindependent = true",
            "independent = true",
            "store = 'custom.db'",
        ] {
            let body = format!(
                "[catalog]\nalias = 'bovine'\n[catalog.graph]\n{fields}\n\n[tasks.build]\nrun = 'true'\n"
            );
            let manifest = toml::from_str::<TaskManifest>(&body).expect("retired table loads");
            assert_eq!(
                manifest
                    .catalog
                    .as_ref()
                    .and_then(|catalog| catalog.alias.clone()),
                Some("bovine".to_owned())
            );
            let notice = manifest
                .retired_config
                .notice(&PathBuf::from("effigy.toml"))
                .expect("one notice");
            assert!(notice.contains("[catalog.graph]"), "{notice}");
            assert!(notice.contains("no replacement"), "{notice}");
        }
    }
    #[test]
    fn env_schema_declaring_catalog_falls_back_to_nearest_ancestor() {
        let catalogs = vec![
            loaded_catalog(
                "root",
                "/workspace-root/acme",
                r#"
[env_schema]
schema = "env/dev.env.schema"
"#,
                0,
            ),
            loaded_catalog(
                "cp-api",
                "/workspace-root/acme/cp-api",
                r#"
[tasks.build]
run = "cargo test"
"#,
                1,
            ),
        ];

        let declaring =
            env_schema_declaring_catalog(&catalogs, Path::new("/workspace-root/acme/cp-api"))
                .expect("ancestor env schema should fill the child scope");

        assert_eq!(declaring.alias, "root");
        assert_eq!(
            declaring.catalog_root,
            PathBuf::from("/workspace-root/acme")
        );
    }

    #[test]
    fn env_schema_declaring_catalog_prefers_nearest_ancestor() {
        let catalogs = vec![
            loaded_catalog(
                "root",
                "/workspace-root/acme",
                r#"
[env_schema]
schema = "env/root.env.schema"
"#,
                0,
            ),
            loaded_catalog(
                "services",
                "/workspace-root/acme/services",
                r#"
[env_schema]
schema = "env/services.env.schema"
"#,
                1,
            ),
            loaded_catalog(
                "cp-api",
                "/workspace-root/acme/services/cp-api",
                r#"
[tasks.build]
run = "cargo test"
"#,
                2,
            ),
        ];

        let declaring = env_schema_declaring_catalog(
            &catalogs,
            Path::new("/workspace-root/acme/services/cp-api"),
        )
        .expect("nearest ancestor env schema should win");

        assert_eq!(declaring.alias, "services");
    }

    #[test]
    fn env_schema_declaring_catalog_prefers_own_schema_over_ancestor() {
        let catalogs = vec![
            loaded_catalog(
                "root",
                "/workspace-root/acme",
                r#"
[env_schema]
schema = "env/root.env.schema"
"#,
                0,
            ),
            loaded_catalog(
                "cp-api",
                "/workspace-root/acme/cp-api",
                r#"
[env_schema]
schema = "env/cp-api.env.schema"

[tasks.build]
run = "cargo test"
"#,
                1,
            ),
        ];

        let declaring =
            env_schema_declaring_catalog(&catalogs, Path::new("/workspace-root/acme/cp-api"))
                .expect("own env schema should win");

        assert_eq!(declaring.alias, "cp-api");
    }

    #[test]
    fn env_schema_declaring_catalog_returns_none_without_any_schema() {
        let catalogs = vec![
            loaded_catalog(
                "root",
                "/workspace-root/acme",
                r#"
[tasks.build]
run = "cargo test"
"#,
                0,
            ),
            loaded_catalog(
                "cp-api",
                "/workspace-root/acme/cp-api",
                r#"
[tasks.build]
run = "cargo test"
"#,
                1,
            ),
        ];

        assert!(
            env_schema_declaring_catalog(&catalogs, Path::new("/workspace-root/acme/cp-api"))
                .is_none()
        );
    }
}
