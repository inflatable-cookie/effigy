use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::runner::manifest::{ManifestCargoEnvMatchMode, ManifestTestSuiteTeardownPolicy};

#[derive(Debug, Clone)]
pub(in crate::runner) struct BuiltinResolvedPlan {
    pub(in crate::runner) suite: String,
    pub(in crate::runner) command: String,
    pub(in crate::runner) env: BTreeMap<String, String>,
    pub(in crate::runner) suite_env: Option<String>,
    pub(in crate::runner) suite_env_files: Vec<String>,
    pub(in crate::runner) setup_command: Option<String>,
    pub(in crate::runner) setup_steps: usize,
    pub(in crate::runner) teardown_command: Option<String>,
    pub(in crate::runner) teardown_steps: usize,
    pub(in crate::runner) teardown_policy: ManifestTestSuiteTeardownPolicy,
    pub(in crate::runner) evidence: Vec<String>,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct BuiltinTestTarget {
    pub(in crate::runner) name: String,
    pub(in crate::runner) root: PathBuf,
    pub(in crate::runner) plans: Vec<BuiltinResolvedPlan>,
    pub(in crate::runner) fallback_chain: Vec<String>,
    pub(in crate::runner) suite_source: String,
    pub(in crate::runner) cargo_env: BTreeMap<String, String>,
    pub(in crate::runner) cargo_env_match: ManifestCargoEnvMatchMode,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::runner) struct BuiltinTestCliFlags {
    pub(in crate::runner) plan_mode: bool,
    pub(in crate::runner) verbose_results: bool,
    pub(in crate::runner) tui: bool,
    pub(in crate::runner) output_json: bool,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct BuiltinTestRunnable {
    pub(in crate::runner) name: String,
    pub(in crate::runner) runner: String,
    pub(in crate::runner) root: PathBuf,
    pub(in crate::runner) command: String,
    pub(in crate::runner) cargo_env: BTreeMap<String, String>,
    pub(in crate::runner) cargo_env_match: ManifestCargoEnvMatchMode,
    pub(in crate::runner) env: BTreeMap<String, String>,
    pub(in crate::runner) setup_command: Option<String>,
    pub(in crate::runner) teardown_command: Option<String>,
    pub(in crate::runner) teardown_policy: ManifestTestSuiteTeardownPolicy,
}
