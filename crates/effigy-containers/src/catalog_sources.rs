//! Catalog sources for container work (Q-005): the project catalog with
//! translated `compose_file` fragments, the pinned bundle's `catalog/`,
//! declared `[catalog.sources.<name>]`, then the compiled baseline.
//!
//! Every layer is checkout-local pinned state or embedded bytes. Building a
//! resolver reads only; source pins move through `effigy bundle sync` on the
//! host, and a guest reads the same pins through the checkout mount.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use effigy_catalog::CatalogResolver;
use effigy_manifest::TASK_MANIFEST_FILE;

use crate::ContainerPolicyError;

pub(crate) const PROJECT_LOCAL_CATALOG_DIR: &str = "infra/dev/catalog";
const RETIRED_PACK_STORE_DIR: &str = "catalog-packs";
const RETIRED_USER_CATALOG_DIR: &str = "catalog";

/// Resolver for `repo_root`'s catalog sources. `None` resolves the compiled
/// baseline only: shared services are machine-scoped and never take one
/// project's sources.
pub fn catalog_resolver(repo_root: Option<&Path>) -> Result<CatalogResolver, ContainerPolicyError> {
    warn_retired_catalog_state(crate::policy_support::effigy_home_dir().as_deref());
    let Some(repo_root) = repo_root else {
        return Ok(CatalogResolver::new(None));
    };
    let project = repo_root.join(PROJECT_LOCAL_CATALOG_DIR);
    let roots = effigy_manifest::catalog_source_roots(&repo_root.join(TASK_MANIFEST_FILE))?;
    let mut resolver = CatalogResolver::new(project.is_dir().then_some(project))
        .with_overlay(crate::policy::compose_file::overlay_for(repo_root))
        .with_bundle_catalog(roots.bundle);
    for (name, root) in roots.declared {
        resolver = resolver.with_declared_source(&name, root)?;
    }
    Ok(resolver)
}

/// Where one environment service's catalog entry resolved from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CatalogServiceSource {
    pub service: String,
    pub catalog: String,
    pub source: String,
    pub layer: &'static str,
    pub path: Option<PathBuf>,
}

/// Catalog source of every service declared by `environment`, in name order.
pub fn catalog_service_sources(
    repo_root: &Path,
    environment: &str,
) -> Result<Vec<CatalogServiceSource>, ContainerPolicyError> {
    let loaded = effigy_manifest::load_task_manifest_with_inspection(
        &repo_root.join(TASK_MANIFEST_FILE),
    )?;
    let Some(config) = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get(environment))
    else {
        return Ok(Vec::new());
    };
    let project = catalog_resolver(Some(repo_root))?;
    let machine = catalog_resolver(None)?;
    config
        .services
        .iter()
        .map(|(name, service)| {
            let resolver = if service.shared == Some(true) {
                &machine
            } else {
                &project
            };
            let fragment = resolver.resolve(&service.catalog)?;
            Ok(CatalogServiceSource {
                service: name.clone(),
                catalog: service.catalog.clone(),
                source: fragment.source.handle().to_owned(),
                layer: fragment.source.layer(),
                path: fragment.source.path().map(Path::to_path_buf),
            })
        })
        .collect()
}

/// One notice per invocation for retired user catalog state. The directories
/// are never read, changed or removed.
fn warn_retired_catalog_state(home: Option<&Path>) {
    static EMITTED: AtomicBool = AtomicBool::new(false);
    let Some(home) = home else {
        return;
    };
    let notices = retired_catalog_notices(home);
    if !notices.is_empty() && !EMITTED.swap(true, Ordering::Relaxed) {
        for notice in notices {
            eprintln!("[warn] {notice}");
        }
    }
}

fn retired_catalog_notices(home: &Path) -> Vec<String> {
    let mut notices = Vec::new();
    let store = home.join(RETIRED_PACK_STORE_DIR);
    if store.is_dir() {
        notices.push(format!(
            "installed catalog pack state `{}` is retired and ignored; services resolve from the project catalog, the bundle's catalog/, [catalog.sources] and the compiled baseline. The directory is left untouched; delete it when convenient",
            store.display()
        ));
    }
    let user_catalog = home.join(RETIRED_USER_CATALOG_DIR);
    if user_catalog.is_dir() {
        notices.push(format!(
            "user-global catalog override `{}` is retired and ignored; move custom fragments to {PROJECT_LOCAL_CATALOG_DIR}/ in each consuming repository, or publish them as a [catalog.sources] source",
            user_catalog.display()
        ));
    }
    notices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fragment(root: &Path, name: &str, description: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("service.toml"),
            format!("[service]\nname = \"{name}\"\ndescription = \"{description}\"\n"),
        )
        .unwrap();
        std::fs::write(dir.join("compose.fragment.yml"), "image: fixture\n").unwrap();
    }

    #[test]
    fn baseline_only_without_a_repository() {
        let resolver = catalog_resolver(None).unwrap();
        assert_eq!(resolver.source_handles(), vec!["project", "baseline"]);
        assert_eq!(
            resolver.resolve("redis").unwrap().source,
            effigy_catalog::FragmentSource::Baseline
        );
    }

    #[test]
    fn repository_sources_resolve_in_precedence_order() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let bundle = temp.path().join("bundle");
        let extra = temp.path().join("extra");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("bundle.toml"), "[bundle]\nname = \"fixture\"\n").unwrap();
        std::fs::write(bundle.join("effigy.toml"), "").unwrap();
        write_fragment(&bundle.join("catalog"), "redis", "bundle");
        write_fragment(&bundle.join("catalog"), "queue", "bundle");
        write_fragment(&extra, "queue", "extra");
        write_fragment(&extra, "mailer", "extra");
        write_fragment(&repo.join(PROJECT_LOCAL_CATALOG_DIR), "mailer", "project");
        std::fs::write(
            repo.join("effigy.toml"),
            format!(
                "[bundle]\nbase = {{ type = \"path\", dir = \"{}\" }}\n\n[catalog.sources.extra]\ntype = \"path\"\ndir = \"{}\"\n",
                bundle.display(),
                extra.display()
            ),
        )
        .unwrap();
        let resolver = catalog_resolver(Some(&repo)).unwrap();
        assert_eq!(
            resolver.source_handles(),
            vec!["project", "bundle", "extra", "baseline"]
        );
        for (name, source) in [
            ("mailer", "project"),
            ("queue", "bundle"),
            ("redis", "bundle"),
            ("extra/queue", "extra"),
            ("baseline/redis", "baseline"),
            ("postgres", "baseline"),
        ] {
            assert_eq!(
                resolver.resolve(name).unwrap().source.handle(),
                source,
                "{name}"
            );
        }
    }

    #[test]
    fn unprepared_declared_source_names_bundle_sync() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("effigy.toml"),
            "[catalog.sources.remote]\ntype = \"git\"\nurl = \"https://invalid.invalid/catalog\"\n",
        )
        .unwrap();
        let error = catalog_resolver(Some(temp.path()))
            .err()
            .expect("unprepared source")
            .to_string();
        assert!(error.contains("effigy bundle sync"), "{error}");
        assert!(!temp.path().join(".effigy").exists());
    }

    #[test]
    fn retired_state_is_named_and_left_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join(RETIRED_PACK_STORE_DIR).join("v1");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("state.json"), "{}").unwrap();
        std::fs::create_dir_all(temp.path().join(RETIRED_USER_CATALOG_DIR)).unwrap();
        let notices = retired_catalog_notices(temp.path());
        assert_eq!(notices.len(), 2, "{notices:?}");
        assert!(notices[0].contains("retired and ignored"), "{notices:?}");
        assert!(notices[0].contains("left untouched"), "{notices:?}");
        assert_eq!(
            std::fs::read_to_string(store.join("state.json")).unwrap(),
            "{}"
        );
        assert!(retired_catalog_notices(&temp.path().join("empty")).is_empty());
    }
}
