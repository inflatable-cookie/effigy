use std::path::Path;

use crate::runner::model::catalog::{LoadedCatalog, TaskSelector};
use crate::runner::RunnerError;

#[path = "planning/config.rs"]
mod config;
#[path = "planning/flags.rs"]
mod flags;
#[path = "planning/model.rs"]
mod model;
#[path = "planning/resolve/mod.rs"]
mod resolve;
#[path = "planning/runnable.rs"]
mod runnable;

pub(super) use model::{
    BuiltinResolvedPlan, BuiltinTestCliFlags, BuiltinTestRunnable, BuiltinTestTarget,
};

pub(super) fn extract_builtin_test_flags(
    raw_args: &[String],
) -> (BuiltinTestCliFlags, Vec<String>) {
    flags::extract_builtin_test_flags(raw_args)
}

pub(super) fn collect_builtin_test_runnable_targets(
    targets: &[BuiltinTestTarget],
) -> Vec<BuiltinTestRunnable> {
    runnable::collect_builtin_test_runnable_targets(targets)
}

pub(super) fn apply_passthrough_to_runnable(
    runnable: Vec<BuiltinTestRunnable>,
    passthrough: &[String],
) -> Vec<BuiltinTestRunnable> {
    runnable::apply_passthrough_to_runnable(runnable, passthrough)
}

pub(super) fn resolve_builtin_test_targets(
    selector: &TaskSelector,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<BuiltinTestTarget>, RunnerError> {
    resolve::resolve_builtin_test_targets(selector.prefix.as_deref(), resolved_root, catalogs)
}

pub(super) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    config::builtin_test_max_parallel(catalogs, resolved_root)
}
