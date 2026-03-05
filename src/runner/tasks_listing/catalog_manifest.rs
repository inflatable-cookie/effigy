use std::path::Path;

use super::super::tasks_view::relative_display_path;
use super::super::LoadedCatalog;

pub(super) struct CatalogManifestContext<'a> {
    catalog: &'a LoadedCatalog,
    manifest: String,
}

impl<'a> CatalogManifestContext<'a> {
    fn new(catalog: &'a LoadedCatalog, manifest: String) -> Self {
        Self { catalog, manifest }
    }

    pub(super) fn catalog(&self) -> &'a LoadedCatalog {
        self.catalog
    }

    pub(super) fn manifest(&self) -> &str {
        self.manifest.as_str()
    }

    pub(super) fn into_manifest(self) -> String {
        self.manifest
    }
}

pub(super) fn manifest_path_context(catalog: &LoadedCatalog) -> CatalogManifestContext<'_> {
    CatalogManifestContext::new(catalog, catalog.manifest_path.display().to_string())
}

pub(super) fn manifest_display_context<'a>(
    catalog: &'a LoadedCatalog,
    resolved_root: &Path,
) -> CatalogManifestContext<'a> {
    CatalogManifestContext::new(
        catalog,
        relative_display_path(resolved_root, &catalog.manifest_path),
    )
}

pub(super) fn ordered_manifest_path_contexts<'a>(
    ordered_catalogs: &'a [&'a LoadedCatalog],
) -> impl Iterator<Item = CatalogManifestContext<'a>> + 'a {
    ordered_catalogs
        .iter()
        .map(|catalog| manifest_path_context(*catalog))
}

pub(super) fn ordered_manifest_display_contexts<'a>(
    ordered_catalogs: &'a [&'a LoadedCatalog],
    resolved_root: &'a Path,
) -> impl Iterator<Item = CatalogManifestContext<'a>> + 'a {
    ordered_catalogs
        .iter()
        .map(|catalog| manifest_display_context(*catalog, resolved_root))
}
