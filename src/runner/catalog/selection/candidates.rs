use super::super::super::LoadedCatalog;

pub(super) fn catalogs_matching_task<'a>(
    catalogs: &'a [LoadedCatalog],
    task_name: &str,
) -> Vec<&'a LoadedCatalog> {
    catalogs
        .iter()
        .filter(|catalog| catalog.manifest.tasks.contains_key(task_name))
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
