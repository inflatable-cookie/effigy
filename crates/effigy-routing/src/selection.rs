mod candidates;
mod errors;
mod prefix;
mod strategy;

use std::path::Path;

use super::error::RoutingError;
use effigy_core::task_selection::{CatalogSelectionMode, TaskSelector, TaskSurface};
use effigy_manifest::{LoadedCatalog, TaskSelection};

/// Resolve one published task selector across effective catalogs.
pub fn select_catalog_and_task<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, RoutingError> {
    select_catalog_and_task_on_surface(TaskSurface::Published, selector, catalogs, cwd)
}

/// Resolve one draft selector across effective catalogs.
///
/// Reuses explicit alias, cwd-nearest, and shallowest-unambiguous routing, but
/// only over `[drafts]` definitions. Ordinary flat task invocation never falls
/// through to a draft.
pub fn select_catalog_and_draft<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, RoutingError> {
    select_catalog_and_task_on_surface(TaskSurface::Draft, selector, catalogs, cwd)
}

/// Surface-aware selector resolution shared by published tasks and drafts.
pub fn select_catalog_and_task_on_surface<'a>(
    surface: TaskSurface,
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, RoutingError> {
    if let Some(prefix_value) = &selector.prefix {
        let Some(catalog) = resolve_catalog_by_prefix(prefix_value, catalogs, cwd) else {
            return Err(errors::task_catalog_prefix_not_found(
                prefix_value,
                catalogs,
            ));
        };
        return build_task_selection(
            surface,
            selector,
            catalog,
            CatalogSelectionMode::ExplicitPrefix,
            vec![prefix::selection_evidence_for_prefix(prefix_value, catalog)],
        );
    }

    let matches =
        candidates::catalogs_matching_surface_task(surface, catalogs, &selector.task_name);
    if matches.is_empty() {
        let all_catalogs = catalogs.iter().collect::<Vec<&LoadedCatalog>>();
        return Err(errors::surface_task_not_found_any(
            surface,
            &selector.task_name,
            all_catalogs.as_slice(),
        ));
    }

    let (selected, mode, evidence) =
        strategy::select_unprefixed_catalog(cwd, &matches, &selector.task_name)?;
    build_task_selection(
        surface,
        selector,
        selected,
        mode,
        vec![match surface {
            TaskSurface::Published => evidence,
            TaskSurface::Draft => format!("draft surface: {evidence}"),
        }],
    )
}

pub fn resolve_catalog_by_prefix<'a>(
    prefix_value: &str,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Option<&'a LoadedCatalog> {
    prefix::resolve_catalog_by_prefix(prefix_value, catalogs, cwd)
}

fn build_task_selection<'a>(
    surface: TaskSurface,
    selector: &TaskSelector,
    catalog: &'a LoadedCatalog,
    mode: CatalogSelectionMode,
    evidence: Vec<String>,
) -> Result<TaskSelection<'a>, RoutingError> {
    let task = match surface {
        TaskSurface::Published => catalog.manifest.tasks.get(&selector.task_name),
        TaskSurface::Draft => catalog.draft_task(&selector.task_name),
    };
    let Some(task) = task else {
        return Err(errors::surface_task_not_found_in_catalog(
            surface,
            &selector.task_name,
            catalog,
        ));
    };
    Ok(TaskSelection {
        catalog,
        task,
        mode,
        evidence,
        surface,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use effigy_core::task_selection::{TaskSelector, TaskSurface};
    use effigy_manifest::{LoadedCatalog, TaskManifest};

    use super::{select_catalog_and_draft, select_catalog_and_task_on_surface};

    fn catalog(alias: &str, root: &str, body: &str, depth: usize) -> LoadedCatalog {
        let root = PathBuf::from(root);
        LoadedCatalog {
            alias: alias.to_owned(),
            catalog_root: root.clone(),
            manifest_path: root.join("effigy.toml"),
            bundle_root: None,
            manifest: toml::from_str::<TaskManifest>(body).expect("parse manifest"),
            defer_run: None,
            deferred_builtins: Default::default(),
            depth,
            draft_sources: BTreeMap::new(),
        }
    }

    fn selector(name: &str) -> TaskSelector {
        TaskSelector {
            prefix: None,
            task_name: name.to_owned(),
        }
    }

    const BODY: &str = r#"
[tasks.build]
run = "printf build"

[drafts.provider-smoke]
created = "2026-09-15"
purpose = "temporary"
run = "printf draft"
"#;

    #[test]
    fn published_selection_does_not_find_drafts() {
        let catalogs = vec![catalog("root", "/tmp/repo", BODY, 0)];
        let error = select_catalog_and_task_on_surface(
            TaskSurface::Published,
            &selector("provider-smoke"),
            &catalogs,
            Path::new("/tmp/repo"),
        )
        .expect_err("draft must not resolve as published");
        assert!(
            error.to_string().contains("task `provider-smoke`"),
            "{error}"
        );
    }

    #[test]
    fn draft_selection_finds_only_drafts() {
        let catalogs = vec![catalog("root", "/tmp/repo", BODY, 0)];
        let selection = select_catalog_and_draft(
            &selector("provider-smoke"),
            &catalogs,
            Path::new("/tmp/repo"),
        )
        .expect("draft resolves");
        assert_eq!(selection.surface, TaskSurface::Draft);

        let error = select_catalog_and_draft(&selector("build"), &catalogs, Path::new("/tmp/repo"))
            .expect_err("published task must not resolve as draft");
        assert!(error.to_string().contains("draft `build`"), "{error}");
    }

    #[test]
    fn explicit_prefix_resolves_within_the_draft_surface() {
        let catalogs = vec![
            catalog("root", "/tmp/repo", BODY, 0),
            catalog("api", "/tmp/repo/api", BODY, 1),
        ];
        let mut prefixed = selector("provider-smoke");
        prefixed.prefix = Some("api".to_owned());
        let selection = select_catalog_and_draft(&prefixed, &catalogs, Path::new("/tmp/repo"))
            .expect("prefixed draft resolves");
        assert_eq!(selection.catalog.alias, "api");
    }
}
