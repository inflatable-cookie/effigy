use super::super::super::model::catalog::LoadedCatalog;
use crate::runner::error::RunnerError;

pub(super) fn format_catalog(catalog: &LoadedCatalog) -> String {
    format!("{} ({})", catalog.alias, catalog.manifest_path.display())
}

pub(super) fn task_catalog_prefix_not_found(
    prefix_value: &str,
    catalogs: &[LoadedCatalog],
) -> RunnerError {
    RunnerError::TaskCatalogPrefixNotFound {
        prefix: prefix_value.to_owned(),
        available: super::candidates::sorted_catalog_aliases(catalogs),
    }
}

pub(super) fn task_not_found_any(task_name: &str, catalogs: &[&LoadedCatalog]) -> RunnerError {
    RunnerError::TaskNotFoundAny {
        name: task_name.to_owned(),
        catalogs: catalogs.iter().copied().map(format_catalog).collect(),
    }
}

pub(super) fn task_ambiguous(task_name: &str, candidates: &[&LoadedCatalog]) -> RunnerError {
    RunnerError::TaskAmbiguous {
        name: task_name.to_owned(),
        candidates: candidates.iter().copied().map(format_catalog).collect(),
    }
}

pub(super) fn task_not_found_in_catalog(task_name: &str, catalog: &LoadedCatalog) -> RunnerError {
    RunnerError::TaskNotFound {
        name: task_name.to_owned(),
        path: catalog.manifest_path.clone(),
    }
}
