#[path = "prepared_task_rows/catalog_projection.rs"]
mod catalog_projection;
#[path = "prepared_task_rows/json_projection.rs"]
mod json_projection;
#[path = "prepared_task_rows/text_projection.rs"]
mod text_projection;

pub(super) use catalog_projection::{CatalogAliasProjection, CatalogTaskProjection};
pub(super) use json_projection::{
    prepare_all_catalog_rows_json, prepare_filtered_rows_json, CatalogTaskJsonRow,
    CatalogTaskJsonRows, ManagedProfileJsonRow,
};
pub(super) use text_projection::{prepare_catalog_match_task_rows, prepare_default_text_rows};
