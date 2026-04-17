use super::super::error::RoutingError;
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

pub(super) fn task_not_found_any(task_name: &str, catalogs: &[&LoadedCatalog]) -> RoutingError {
    RoutingError::TaskNotFoundAny {
        name: task_name.to_owned(),
        catalogs: catalogs.iter().copied().map(format_catalog).collect(),
    }
}

pub(super) fn task_ambiguous(task_name: &str, candidates: &[&LoadedCatalog]) -> RoutingError {
    RoutingError::TaskAmbiguous {
        name: task_name.to_owned(),
        candidates: candidates.iter().copied().map(format_catalog).collect(),
    }
}

pub(super) fn task_not_found_in_catalog(task_name: &str, catalog: &LoadedCatalog) -> RoutingError {
    RoutingError::TaskNotFound {
        name: task_name.to_owned(),
        path: catalog.manifest_path.clone(),
    }
}
