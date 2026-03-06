#[path = "tasks_probe/model.rs"]
mod model;
#[path = "tasks_probe/resolve.rs"]
mod resolve;

pub(super) use resolve::build_resolve_probe;
