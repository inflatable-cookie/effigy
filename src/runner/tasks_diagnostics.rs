use serde_json::json;

use super::LoadedCatalog;

pub(super) fn build_catalog_diagnostics(
    catalogs: &[LoadedCatalog],
) -> (Vec<&LoadedCatalog>, Vec<serde_json::Value>) {
    let mut ordered_catalogs = catalogs.iter().collect::<Vec<&LoadedCatalog>>();
    ordered_catalogs.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    let catalog_diagnostics = ordered_catalogs
        .iter()
        .map(|catalog| {
            json!({
                "alias": catalog.alias,
                "root": catalog.catalog_root.display().to_string(),
                "depth": catalog.depth,
                "manifest": catalog.manifest_path.display().to_string(),
                "has_defer": catalog.defer_run.is_some(),
            })
        })
        .collect::<Vec<serde_json::Value>>();

    (ordered_catalogs, catalog_diagnostics)
}
