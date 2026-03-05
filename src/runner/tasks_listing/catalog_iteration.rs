use super::super::{LoadedCatalog, ManifestTask};

pub(super) fn catalog_tasks<'a>(
    catalog: &'a LoadedCatalog,
) -> impl Iterator<Item = (&'a str, &'a ManifestTask)> + 'a {
    catalog
        .manifest
        .tasks
        .iter()
        .map(|(task_name, task)| (task_name.as_str(), task))
}

pub(super) fn for_each_catalog_task(
    catalog: &LoadedCatalog,
    mut visit: impl FnMut(&str, &ManifestTask),
) {
    for (task_name, task) in catalog_tasks(catalog) {
        visit(task_name, task);
    }
}
