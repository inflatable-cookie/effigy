use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_manifest::{ManifestCargoEnvMatchMode, ManifestTestSuiteTeardownPolicy};

#[derive(Debug, Clone)]
pub(crate) struct BuiltinResolvedPlan {
    pub(crate) suite: String,
    pub(crate) command: String,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) suite_env: Option<String>,
    pub(crate) suite_env_files: Vec<String>,
    pub(crate) setup_command: Option<String>,
    pub(crate) setup_steps: usize,
    pub(crate) teardown_command: Option<String>,
    pub(crate) teardown_steps: usize,
    pub(crate) teardown_policy: ManifestTestSuiteTeardownPolicy,
    pub(crate) evidence: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct BuiltinTestTarget {
    pub(crate) name: String,
    pub(crate) root: PathBuf,
    pub(crate) plans: Vec<BuiltinResolvedPlan>,
    pub(crate) fallback_chain: Vec<String>,
    pub(crate) suite_source: String,
    pub(crate) cargo_env: BTreeMap<String, String>,
    pub(crate) cargo_env_match: ManifestCargoEnvMatchMode,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinTestCliFlags {
    pub(crate) plan_mode: bool,
    pub(crate) verbose_results: bool,
    pub(crate) tui: bool,
    pub(crate) output_json: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct BuiltinTestRunnable {
    pub(crate) name: String,
    pub(crate) runner: String,
    pub(crate) root: PathBuf,
    pub(crate) command: String,
    pub(crate) cargo_env: BTreeMap<String, String>,
    pub(crate) cargo_env_match: ManifestCargoEnvMatchMode,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) setup_command: Option<String>,
    pub(crate) teardown_command: Option<String>,
    pub(crate) teardown_policy: ManifestTestSuiteTeardownPolicy,
}
