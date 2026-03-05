use super::super::super::{LoadedCatalog, ManifestTask};
use super::super::catalog_iteration::for_each_catalog_task;
use super::super::catalog_manifest::{
    manifest_path_context, ordered_manifest_path_contexts, CatalogManifestContext,
};
use super::payload::JsonTaskRows;
use super::rows::{catalog_and_managed_rows_json, ManagedProfileRowJson, TaskRowJson};

enum CatalogContextTaskSpec<'a> {
    AllTasks,
    SingleTask {
        task_name: &'a str,
        task: &'a ManifestTask,
    },
}

struct CatalogContextWorkItem<'a> {
    context: CatalogManifestContext<'a>,
    task_spec: CatalogContextTaskSpec<'a>,
}

struct JsonRowAccumulator {
    catalog_rows: Vec<TaskRowJson>,
    managed_profile_rows: Vec<ManagedProfileRowJson>,
}

impl JsonRowAccumulator {
    fn with_task_row_capacity(task_row_capacity: usize) -> Self {
        Self {
            catalog_rows: Vec::with_capacity(task_row_capacity),
            managed_profile_rows: Vec::new(),
        }
    }

    fn push_empty_catalog(&mut self, manifest: String) {
        self.catalog_rows.push(TaskRowJson::empty_owned(manifest));
    }

    fn push_task_rows(
        &mut self,
        manifest: &str,
        catalog: &LoadedCatalog,
        task_name: &str,
        task: &ManifestTask,
    ) {
        let (catalog_row, managed_rows) =
            catalog_and_managed_rows_json(manifest, catalog, task_name, task);
        self.catalog_rows.push(catalog_row);
        self.managed_profile_rows.extend(managed_rows);
    }

    fn push_catalog_context_item(&mut self, item: CatalogContextWorkItem<'_>) {
        match item.task_spec {
            CatalogContextTaskSpec::AllTasks => {
                if item.context.catalog().manifest.tasks.is_empty() {
                    self.push_empty_catalog(item.context.into_manifest());
                    return;
                }

                for_each_catalog_task(item.context.catalog(), |task_name, task| {
                    self.push_task_rows(
                        item.context.manifest(),
                        item.context.catalog(),
                        task_name,
                        task,
                    );
                });
            }
            CatalogContextTaskSpec::SingleTask { task_name, task } => {
                self.push_task_rows(
                    item.context.manifest(),
                    item.context.catalog(),
                    task_name,
                    task,
                );
            }
        }
    }

    fn finish(self) -> JsonTaskRows {
        JsonTaskRows::new(self.catalog_rows, self.managed_profile_rows)
    }
}

fn collect_catalog_context_rows<'a>(
    task_row_capacity: usize,
    items: impl IntoIterator<Item = CatalogContextWorkItem<'a>>,
) -> JsonTaskRows {
    let mut rows = JsonRowAccumulator::with_task_row_capacity(task_row_capacity);
    for item in items {
        rows.push_catalog_context_item(item);
    }
    rows.finish()
}

pub(super) fn collect_all_catalog_rows(ordered_catalogs: &[&LoadedCatalog]) -> JsonTaskRows {
    let task_row_capacity = ordered_catalogs
        .iter()
        .map(|catalog| {
            if catalog.manifest.tasks.is_empty() {
                1
            } else {
                catalog.manifest.tasks.len()
            }
        })
        .sum();
    collect_catalog_context_rows(
        task_row_capacity,
        ordered_manifest_path_contexts(ordered_catalogs).map(|context| CatalogContextWorkItem {
            context,
            task_spec: CatalogContextTaskSpec::AllTasks,
        }),
    )
}

pub(super) fn collect_filtered_rows(
    matches: &[(&LoadedCatalog, &ManifestTask)],
    task_name: &str,
) -> JsonTaskRows {
    collect_catalog_context_rows(
        matches.len(),
        matches
            .iter()
            .map(|(catalog, task)| CatalogContextWorkItem {
                context: manifest_path_context(catalog),
                task_spec: CatalogContextTaskSpec::SingleTask { task_name, task },
            }),
    )
}
