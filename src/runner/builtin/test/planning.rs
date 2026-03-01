use std::path::{Path, PathBuf};

use super::super::super::{LoadedCatalog, TaskSelector, DEFAULT_BUILTIN_TEST_MAX_PARALLEL};

#[path = "planning/resolve.rs"]
mod resolve;
#[path = "planning/runnable.rs"]
mod runnable;

#[derive(Debug, Clone)]
pub(super) struct BuiltinResolvedPlan {
    pub(super) suite: String,
    pub(super) command: String,
    pub(super) evidence: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BuiltinTestTarget {
    pub(super) name: String,
    pub(super) root: PathBuf,
    pub(super) plans: Vec<BuiltinResolvedPlan>,
    pub(super) fallback_chain: Vec<String>,
    pub(super) suite_source: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BuiltinTestCliFlags {
    pub(super) plan_mode: bool,
    pub(super) verbose_results: bool,
    pub(super) tui: bool,
    pub(super) output_json: bool,
}

#[derive(Debug, Clone)]
pub(super) struct BuiltinTestRunnable {
    pub(super) name: String,
    pub(super) runner: String,
    pub(super) root: PathBuf,
    pub(super) command: String,
}

pub(super) fn extract_builtin_test_flags(
    raw_args: &[String],
) -> (BuiltinTestCliFlags, Vec<String>) {
    let mut flags = BuiltinTestCliFlags {
        plan_mode: false,
        verbose_results: false,
        tui: false,
        output_json: false,
    };
    let passthrough = raw_args
        .iter()
        .filter_map(|arg| {
            if arg == "--plan" {
                flags.plan_mode = true;
                None
            } else if arg == "--verbose-results" {
                flags.verbose_results = true;
                None
            } else if arg == "--tui" {
                flags.tui = true;
                None
            } else if arg == "--json" {
                flags.output_json = true;
                None
            } else {
                Some(arg.clone())
            }
        })
        .collect::<Vec<String>>();
    (flags, passthrough)
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
) -> Vec<BuiltinTestTarget> {
    resolve::resolve_builtin_test_targets(selector.prefix.as_deref(), resolved_root, catalogs)
}

pub(super) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    let configured = catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == resolved_root)
        .find_map(|catalog| {
            catalog
                .manifest
                .test
                .as_ref()
                .and_then(|test| test.max_parallel)
        })
        .filter(|value| *value > 0);

    configured.unwrap_or(DEFAULT_BUILTIN_TEST_MAX_PARALLEL)
}
