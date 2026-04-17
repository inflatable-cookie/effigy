use super::super::manifest::task_runtime::ManifestTask;
use effigy_manifest::LoadedCatalog;

pub(super) fn catalog_tasks<'a>(
    catalog: &'a LoadedCatalog,
) -> impl Iterator<Item = (&'a str, &'a ManifestTask)> + 'a {
    catalog
        .manifest
        .tasks
        .iter()
        .map(|(task_name, task)| (task_name.as_str(), task))
}
