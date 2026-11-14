//! Choosing the catalog that owns a path's inherited section.
//!
//! A child inherits a section (env schema, systems, containers) from the nearest
//! ancestor that declares it; the section's paths resolve against that catalog.

use std::path::Path;

use crate::LoadedCatalog;

/// The deepest catalog whose root contains `catalog_root`, including the
/// catalog at that path itself. `None` means no catalog root contains it.
pub fn nearest_catalog<'a>(
    catalogs: &'a [LoadedCatalog],
    catalog_root: &Path,
) -> Option<&'a LoadedCatalog> {
    nearest_declaring_catalog(catalogs, catalog_root, |_| Some(())).map(|(catalog, ())| catalog)
}

/// The deepest catalog that contains `catalog_root` and declares `section`,
/// with the declared value. The nearest declaring ancestor wins, and the
/// returned catalog is the one the section's paths resolve against.
pub fn nearest_declaring_catalog<'a, T>(
    catalogs: &'a [LoadedCatalog],
    catalog_root: &Path,
    section: impl Fn(&'a LoadedCatalog) -> Option<T>,
) -> Option<(&'a LoadedCatalog, T)> {
    catalogs
        .iter()
        .filter(|catalog| catalog_root.starts_with(&catalog.catalog_root))
        .filter_map(|catalog| section(catalog).map(|value| (catalog, value)))
        .max_by_key(|(catalog, _)| catalog.depth)
}

/// The deepest catalog that contains `catalog_root` and declares `[env_schema]`,
/// including the catalog at that path itself.
pub fn env_schema_declaring_catalog<'a>(
    catalogs: &'a [LoadedCatalog],
    catalog_root: &Path,
) -> Option<&'a LoadedCatalog> {
    nearest_declaring_catalog(catalogs, catalog_root, |catalog| {
        catalog.manifest.env_schema.as_ref()
    })
    .map(|(catalog, _)| catalog)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{env_schema_declaring_catalog, nearest_catalog, nearest_declaring_catalog};
    use crate::LoadedCatalog;

    fn loaded_catalog(alias: &str, root: &str, manifest_body: &str, depth: usize) -> LoadedCatalog {
        LoadedCatalog::for_test(
            alias,
            PathBuf::from(root),
            toml::from_str(manifest_body).expect("parse manifest"),
            depth,
        )
    }

    fn catalogs() -> Vec<LoadedCatalog> {
        vec![
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

[systems]
default = "dev"
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
        ]
    }

    /// The nearest catalog wins, including the catalog at the path itself.
    #[test]
    fn nearest_catalog_is_the_deepest_containing_root() {
        let catalogs = catalogs();
        let path = Path::new("/workspace-root/acme/services/cp-api");
        assert_eq!(
            nearest_catalog(&catalogs, path).map(|catalog| catalog.alias.as_str()),
            Some("cp-api")
        );
        assert_eq!(
            nearest_catalog(&catalogs, Path::new("/workspace-root/acme/services"))
                .map(|catalog| catalog.alias.as_str()),
            Some("services"),
            "the catalog at the path itself counts"
        );
        assert!(
            nearest_catalog(&catalogs, Path::new("/elsewhere")).is_none(),
            "a path outside every root has no owning catalog"
        );
    }

    /// A child without its own section inherits the nearest declaring
    /// ancestor, and the returned catalog is the declaring one.
    #[test]
    fn nearest_declaring_catalog_returns_the_declaring_ancestor_and_its_value() {
        let catalogs = catalogs();
        let (catalog, systems) = nearest_declaring_catalog(
            &catalogs,
            Path::new("/workspace-root/acme/services/cp-api"),
            |catalog| catalog.manifest.systems.as_ref(),
        )
        .expect("the services catalog declares systems");
        assert_eq!(catalog.alias, "services");
        assert_eq!(systems.default.as_deref(), Some("dev"));

        assert!(
            nearest_declaring_catalog(
                &catalogs,
                Path::new("/workspace-root/acme/services/cp-api"),
                |catalog| catalog.manifest.release.as_ref(),
            )
            .is_none(),
            "no ancestor declares the section"
        );
    }

    #[test]
    fn env_schema_declaring_catalog_falls_back_to_nearest_ancestor() {
        let catalogs = catalogs();
        let declaring = env_schema_declaring_catalog(
            &catalogs,
            Path::new("/workspace-root/acme/services/cp-api"),
        )
        .expect("ancestor env schema should fill the child scope");
        assert_eq!(declaring.alias, "services");
    }

    /// A child that declares its own `[env_schema]` keeps it: the ancestor's
    /// declaration must not win over the child's own. Both catalogs in the
    /// fixture declare one, so selecting the ancestor here is the failure this
    /// test detects.
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
                .expect("the child declares its own env schema");
        assert_eq!(
            declaring.alias, "cp-api",
            "the child's own declaration wins over its ancestor's"
        );
        assert_eq!(
            declaring
                .manifest
                .env_schema
                .as_ref()
                .and_then(|schema| schema.schema.as_deref()),
            Some("env/cp-api.env.schema"),
            "the schema path resolves against the declaring catalog"
        );

        // The ancestor is still selected for a path that only it covers.
        assert_eq!(
            env_schema_declaring_catalog(&catalogs, Path::new("/workspace-root/acme"))
                .map(|catalog| catalog.alias.as_str()),
            Some("root")
        );
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
