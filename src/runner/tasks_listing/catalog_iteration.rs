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
