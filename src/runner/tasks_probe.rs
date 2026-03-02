#[path = "tasks_probe/model.rs"]
mod model;
#[path = "tasks_probe/resolve.rs"]
mod resolve;

pub(super) fn build_resolve_probe(
    raw_selector: Option<String>,
    catalogs: &[super::LoadedCatalog],
) -> Result<Option<serde_json::Value>, super::RunnerError> {
    resolve::build_resolve_probe(raw_selector, catalogs)
}
