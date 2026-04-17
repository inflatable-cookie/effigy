use std::path::Path;

use serde_json::json;

use effigy_manifest::LoadedCatalog;

pub(in crate::runner) struct ListingCatalogSnapshot<'a> {
    catalogs: &'a [LoadedCatalog],
    ordered_catalogs: &'a [&'a LoadedCatalog],
    catalog_diagnostics: &'a [serde_json::Value],
    precedence: &'a [String],
    resolved_root: &'a Path,
}

impl<'a> ListingCatalogSnapshot<'a> {
    pub(in crate::runner) fn new(
        catalogs: &'a [LoadedCatalog],
        ordered_catalogs: &'a [&'a LoadedCatalog],
        catalog_diagnostics: &'a [serde_json::Value],
        precedence: &'a [String],
        resolved_root: &'a Path,
    ) -> Self {
        Self {
            catalogs,
            ordered_catalogs,
            catalog_diagnostics,
            precedence,
            resolved_root,
        }
    }

    pub(in crate::runner) fn catalogs(&self) -> &'a [LoadedCatalog] {
        self.catalogs
    }

    pub(in crate::runner) fn ordered_catalogs(&self) -> &'a [&'a LoadedCatalog] {
        self.ordered_catalogs
    }

    pub(in crate::runner) fn catalog_diagnostics(&self) -> &'a [serde_json::Value] {
        self.catalog_diagnostics
    }

    pub(in crate::runner) fn precedence(&self) -> &'a [String] {
        self.precedence
    }

    pub(in crate::runner) fn resolved_root(&self) -> &'a Path {
        self.resolved_root
    }
}

pub(in crate::runner) fn build_catalog_diagnostics(
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
