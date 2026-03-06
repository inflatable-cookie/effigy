#[path = "catalog/discovery.rs"]
mod discovery;
#[path = "catalog/selection.rs"]
mod selection;

pub(super) use discovery::{
    default_alias, discover_catalogs, discover_catalogs_allow_missing, discover_manifest_paths,
};
pub(super) use selection::{resolve_catalog_by_prefix, select_catalog_and_task};
