use std::path::Path;

use super::super::tasks_view::relative_display_path;
use effigy_manifest::LoadedCatalog;

pub(super) fn catalog_manifest_path(catalog: &LoadedCatalog) -> String {
    catalog.manifest_path.display().to_string()
}

pub(super) fn catalog_manifest_display(catalog: &LoadedCatalog, resolved_root: &Path) -> String {
    relative_display_path(resolved_root, &catalog.manifest_path)
}
