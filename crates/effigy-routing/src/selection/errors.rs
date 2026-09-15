use super::super::error::RoutingError;
use effigy_core::task_selection::TaskSurface;
use effigy_manifest::LoadedCatalog;

pub(super) fn format_catalog(catalog: &LoadedCatalog) -> String {
    format!("{} ({})", catalog.alias, catalog.manifest_path.display())
}

pub(super) fn task_catalog_prefix_not_found(
    prefix_value: &str,
    catalogs: &[LoadedCatalog],
) -> RoutingError {
    RoutingError::TaskCatalogPrefixNotFound {
        prefix: prefix_value.to_owned(),
        available: super::candidates::sorted_catalog_aliases(catalogs),
    }
}

pub(super) fn surface_task_not_found_any(
    surface: TaskSurface,
    task_name: &str,
    catalogs: &[&LoadedCatalog],
) -> RoutingError {
    let catalogs = catalogs.iter().copied().map(format_catalog).collect();
    match surface {
        TaskSurface::Published => RoutingError::TaskNotFoundAny {
            name: task_name.to_owned(),
            catalogs,
        },
        TaskSurface::Draft => RoutingError::DraftNotFoundAny {
            name: task_name.to_owned(),
            catalogs,
        },
    }
}

pub(super) fn surface_task_ambiguous(
    surface: TaskSurface,
    task_name: &str,
    candidates: &[&LoadedCatalog],
) -> RoutingError {
    let candidates = candidates.iter().copied().map(format_catalog).collect();
    match surface {
        TaskSurface::Published => RoutingError::TaskAmbiguous {
            name: task_name.to_owned(),
            candidates,
        },
        TaskSurface::Draft => RoutingError::DraftAmbiguous {
            name: task_name.to_owned(),
            candidates,
        },
    }
}

pub(super) fn surface_task_not_found_in_catalog(
    surface: TaskSurface,
    task_name: &str,
    catalog: &LoadedCatalog,
) -> RoutingError {
    let path = catalog.manifest_path.clone();
    match surface {
        TaskSurface::Published => RoutingError::TaskNotFound {
            name: task_name.to_owned(),
            path,
        },
        TaskSurface::Draft => RoutingError::DraftNotFound {
            name: task_name.to_owned(),
            path,
        },
    }
}

pub(super) fn task_not_found_any(task_name: &str, catalogs: &[&LoadedCatalog]) -> RoutingError {
    surface_task_not_found_any(TaskSurface::Published, task_name, catalogs)
}

pub(super) fn task_ambiguous(task_name: &str, candidates: &[&LoadedCatalog]) -> RoutingError {
    surface_task_ambiguous(TaskSurface::Published, task_name, candidates)
}
