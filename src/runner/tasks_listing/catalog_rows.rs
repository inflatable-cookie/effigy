use super::super::{LoadedCatalog, ManifestTask};

pub(super) enum CatalogRow<'a> {
    EmptyCatalog {
        catalog: &'a LoadedCatalog,
    },
    Task {
        catalog: &'a LoadedCatalog,
        task_name: &'a str,
        task: &'a ManifestTask,
    },
}

pub(super) struct CatalogRows<'a> {
    rows: Vec<CatalogRow<'a>>,
    has_tasks: bool,
}

impl<'a> CatalogRows<'a> {
    pub(super) fn rows(&self) -> &[CatalogRow<'a>] {
        self.rows.as_slice()
    }

    pub(super) fn has_tasks(&self) -> bool {
        self.has_tasks
    }
}

pub(super) fn assemble_catalog_rows<'a>(ordered_catalogs: &[&'a LoadedCatalog]) -> CatalogRows<'a> {
    let mut rows = Vec::<CatalogRow<'a>>::new();
    let mut has_tasks = false;
    for catalog in ordered_catalogs {
        if catalog.manifest.tasks.is_empty() {
            rows.push(CatalogRow::EmptyCatalog { catalog });
            continue;
        }
        for (task_name, task) in &catalog.manifest.tasks {
            rows.push(CatalogRow::Task {
                catalog,
                task_name,
                task,
            });
            has_tasks = true;
        }
    }
    CatalogRows { rows, has_tasks }
}
