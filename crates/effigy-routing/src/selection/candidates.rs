use effigy_core::task_selection::TaskSurface;
use effigy_manifest::LoadedCatalog;

/// Catalogs declaring `task_name` on the requested surface.
pub(super) fn catalogs_matching_surface_task<'a>(
    surface: TaskSurface,
    catalogs: &'a [LoadedCatalog],
    task_name: &str,
) -> Vec<&'a LoadedCatalog> {
    catalogs
        .iter()
        .filter(|catalog| match surface {
            TaskSurface::Published => catalog.manifest.tasks.contains_key(task_name),
            TaskSurface::Draft => catalog.manifest.drafts.contains_key(task_name),
        })
        .collect()
}

pub(super) fn sorted_catalog_aliases(catalogs: &[LoadedCatalog]) -> Vec<String> {
    let mut available = catalogs
        .iter()
        .map(|catalog| catalog.alias.clone())
        .collect::<Vec<String>>();
    available.sort();
    available
}
