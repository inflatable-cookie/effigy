use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(in crate::runner) struct ManagedProcessSpec {
    pub(in crate::runner) name: String,
    pub(in crate::runner) run: String,
    pub(in crate::runner) cwd: PathBuf,
    pub(in crate::runner) start_after_ms: u64,
    pub(in crate::runner) shutdown_on_exit: bool,
}

#[derive(Debug)]
pub(in crate::runner) struct ManagedTaskPlan {
    pub(in crate::runner) mode: String,
    pub(in crate::runner) profile: String,
    pub(in crate::runner) processes: Vec<ManagedProcessSpec>,
    pub(in crate::runner) tab_order: Vec<String>,
    pub(in crate::runner) fail_on_non_zero: bool,
    pub(in crate::runner) passthrough: Vec<String>,
}
