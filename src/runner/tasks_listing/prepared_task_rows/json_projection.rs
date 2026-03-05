use serde::Serialize;

use super::super::super::tasks_view::ManagedProfileDisplayRow;
use super::super::super::LoadedCatalog;
use super::super::row_projection::TaskSignatureProjection;
use super::super::selection::CatalogTaskMatch;
use super::catalog_projection::{
    prepare_matched_catalog_task_rows_for_path, prepare_ordered_catalog_task_rows_for_path,
    CatalogTaskListingEntry, CatalogTaskProjection,
};

#[derive(Clone, Serialize)]
pub(in super::super) struct CatalogTaskJsonRow {
    task: Option<String>,
    run: Option<String>,
    manifest: String,
}

#[derive(Clone, Serialize)]
pub(in super::super) struct ManagedProfileJsonRow {
    task: String,
    run: String,
    manifest: String,
    profile: String,
    invocation: String,
    parent_task: String,
}

pub(in super::super) struct CatalogTaskJsonRows {
    task_rows: Vec<CatalogTaskJsonRow>,
    managed_profile_rows: Vec<ManagedProfileJsonRow>,
}

impl CatalogTaskJsonRows {
    fn with_task_row_capacity(task_row_capacity: usize) -> Self {
        Self {
            task_rows: Vec::with_capacity(task_row_capacity),
            managed_profile_rows: Vec::new(),
        }
    }

    pub(in super::super) fn into_parts(
        self,
    ) -> (Vec<CatalogTaskJsonRow>, Vec<ManagedProfileJsonRow>) {
        (self.task_rows, self.managed_profile_rows)
    }

    fn push_empty_catalog(&mut self, manifest: String) {
        self.task_rows
            .push(CatalogTaskJsonRow::empty_owned(manifest));
    }

    fn push_task_rows(&mut self, projected_rows: CatalogTaskProjection) {
        let (task_row, managed_rows) = projected_json_rows(projected_rows);
        self.task_rows.push(task_row);
        self.managed_profile_rows.extend(managed_rows);
    }
}

impl CatalogTaskJsonRow {
    fn empty_owned(manifest: String) -> Self {
        Self {
            task: None,
            run: None,
            manifest,
        }
    }

    fn from_signature(row: TaskSignatureProjection, manifest: &str) -> Self {
        Self {
            task: Some(row.task().to_owned()),
            run: Some(row.run().to_owned()),
            manifest: manifest.to_owned(),
        }
    }
}

impl ManagedProfileJsonRow {
    fn from_display(row: ManagedProfileDisplayRow, manifest: &str) -> Self {
        Self {
            task: row.task,
            run: row.run,
            manifest: manifest.to_owned(),
            profile: row.profile,
            invocation: row.invocation,
            parent_task: row.parent_task,
        }
    }
}

pub(in super::super) fn prepare_all_catalog_rows_json(
    ordered_catalogs: &[&LoadedCatalog],
) -> CatalogTaskJsonRows {
    let projected_rows = prepare_ordered_catalog_task_rows_for_path(ordered_catalogs);
    let mut rows = CatalogTaskJsonRows::with_task_row_capacity(projected_rows.len());
    for projected_row in projected_rows {
        match projected_row {
            CatalogTaskListingEntry::EmptyManifest(manifest) => rows.push_empty_catalog(manifest),
            CatalogTaskListingEntry::Task(row) => rows.push_task_rows(row),
        }
    }
    rows
}

pub(in super::super) fn prepare_filtered_rows_json(
    matches: &[CatalogTaskMatch<'_>],
    task_name: &str,
) -> CatalogTaskJsonRows {
    let projected_rows = prepare_matched_catalog_task_rows_for_path(matches, task_name);
    let mut rows = CatalogTaskJsonRows::with_task_row_capacity(projected_rows.len());
    for projected_row in projected_rows {
        rows.push_task_rows(projected_row);
    }
    rows
}

fn projected_json_rows(
    row: CatalogTaskProjection,
) -> (CatalogTaskJsonRow, Vec<ManagedProfileJsonRow>) {
    let (manifest, task_row, managed_rows) = row.into_parts();
    let managed_rows = managed_rows
        .into_iter()
        .map(|row| ManagedProfileJsonRow::from_display(row, manifest.as_str()))
        .collect::<Vec<ManagedProfileJsonRow>>();
    let task_row = CatalogTaskJsonRow::from_signature(task_row, manifest.as_str());
    (task_row, managed_rows)
}
