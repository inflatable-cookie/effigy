use std::path::Path;

use super::super::model::catalog::{DeferredCommand, LoadedCatalog, TaskSelector};
use super::super::model::constants::IMPLICIT_ROOT_DEFER_TEMPLATE;

pub(in crate::runner) fn select_deferral(
    selector: &TaskSelector,
    catalogs: &[LoadedCatalog],
    cwd: &Path,
    workspace_root: &Path,
) -> Option<DeferredCommand> {
    if let Some(selected) = select_explicit_catalog(selector, catalogs) {
        return Some(selected);
    }
    if let Some(selected) = select_in_scope_catalog(catalogs, cwd) {
        return Some(selected);
    }
    if let Some(selected) = select_fallback_catalog(catalogs) {
        return Some(selected);
    }
    infer_implicit_root_deferral(workspace_root)
}

fn select_explicit_catalog(
    selector: &TaskSelector,
    catalogs: &[LoadedCatalog],
) -> Option<DeferredCommand> {
    let prefix = selector.prefix.as_ref()?;
    let catalog = catalogs.iter().find(|catalog| &catalog.alias == prefix)?;
    catalog.defer_run.as_ref().map(|template| DeferredCommand {
        template: template.clone(),
        working_dir: catalog.catalog_root.clone(),
        source: catalog_source(catalog),
    })
}

fn select_in_scope_catalog(catalogs: &[LoadedCatalog], cwd: &Path) -> Option<DeferredCommand> {
    let mut in_scope = catalogs
        .iter()
        .filter(|catalog| catalog.defer_run.is_some() && cwd.starts_with(&catalog.catalog_root))
        .collect::<Vec<&LoadedCatalog>>();
    in_scope.sort_by(|a, b| {
        b.depth
            .cmp(&a.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    in_scope
        .first()
        .and_then(|catalog| defer_command_from_catalog(catalog))
}

fn select_fallback_catalog(catalogs: &[LoadedCatalog]) -> Option<DeferredCommand> {
    let mut fallback = catalogs
        .iter()
        .filter(|catalog| catalog.defer_run.is_some())
        .collect::<Vec<&LoadedCatalog>>();
    fallback.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    fallback
        .first()
        .and_then(|catalog| defer_command_from_catalog(catalog))
}

fn defer_command_from_catalog(catalog: &LoadedCatalog) -> Option<DeferredCommand> {
    catalog.defer_run.as_ref().map(|template| DeferredCommand {
        template: template.clone(),
        working_dir: catalog.catalog_root.clone(),
        source: catalog_source(catalog),
    })
}

fn catalog_source(catalog: &LoadedCatalog) -> String {
    format!(
        "catalog {} ({})",
        catalog.alias,
        catalog.manifest_path.display()
    )
}

fn infer_implicit_root_deferral(workspace_root: &Path) -> Option<DeferredCommand> {
    let has_effigy_json = workspace_root.join("effigy.json").is_file();
    let has_composer_json = workspace_root.join("composer.json").is_file();
    if has_effigy_json && has_composer_json {
        return Some(DeferredCommand {
            template: IMPLICIT_ROOT_DEFER_TEMPLATE.to_owned(),
            working_dir: workspace_root.to_path_buf(),
            source: "implicit root deferral (composer.json + effigy.json)".to_owned(),
        });
    }
    None
}
