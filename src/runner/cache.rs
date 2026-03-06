mod fingerprint;
#[path = "cache/model.rs"]
pub(in crate::runner) mod model;
#[path = "cache/ops.rs"]
pub(in crate::runner) mod ops;
mod store;

const CACHE_DIR: &str = ".effigy/cache";
const CACHE_STORE_FILE: &str = "task-cache-v1.json";
