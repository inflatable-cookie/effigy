use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) struct MigrateScript {
    pub(super) name: String,
    pub(super) command: String,
}

pub(super) struct MigrateRequest {
    pub(super) output_json: bool,
    pub(super) apply: bool,
    pub(super) package_path: Option<PathBuf>,
    pub(super) script_filter: BTreeSet<String>,
}

pub(super) struct MigratePlan {
    pub(super) package_path: PathBuf,
    pub(super) manifest_path: PathBuf,
    pub(super) apply: bool,
    pub(super) added: Vec<MigrateScript>,
    pub(super) conflicts: Vec<MigrateScript>,
    pub(super) written: bool,
}
