use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use effigy_manifest::ManifestTestSuiteTeardownPolicy;

use crate::test::planning::BuiltinTestTarget;

/// The manifest facts one suite's result or plan carries.
///
/// Every rendering names the same facts (source, Cargo-env match mode, suite
/// env files, step counts, teardown policy). This type decodes them once from
/// the target set, and it is the one place that decides what a suite with no
/// surviving plan means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SuiteFacts {
    pub(super) suite_source: String,
    pub(super) cargo_env_match: String,
    pub(super) available_suites: Vec<String>,
    pub(super) suite_env: Option<String>,
    pub(super) suite_env_files: Vec<String>,
    pub(super) setup_steps: usize,
    pub(super) teardown_steps: usize,
    pub(super) teardown_policy: ManifestTestSuiteTeardownPolicy,
}

impl SuiteFacts {
    /// The facts a result reports when no plan matched it: an unknown source
    /// and match mode, no env, no lifecycle steps, and the default policy.
    /// Nothing here invents a pass or a suite source.
    fn unplanned(suite: &str) -> Self {
        Self {
            suite_source: "unknown".to_owned(),
            cargo_env_match: "unknown".to_owned(),
            available_suites: vec![suite.to_owned()],
            suite_env: None,
            suite_env_files: Vec::new(),
            setup_steps: 0,
            teardown_steps: 0,
            teardown_policy: ManifestTestSuiteTeardownPolicy::OnSuccess,
        }
    }
}

/// Every planned suite's facts, keyed by the target root and suite name a
/// result reports.
pub(super) struct SuiteFactsIndex {
    facts: BTreeMap<(String, String), SuiteFacts>,
}

impl SuiteFactsIndex {
    pub(super) fn new(targets: &[BuiltinTestTarget]) -> Self {
        let mut facts = BTreeMap::new();
        for target in targets {
            let root = target.root.display().to_string();
            let available_suites = target
                .plans
                .iter()
                .map(|plan| plan.suite.clone())
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect::<Vec<String>>();
            for plan in &target.plans {
                facts.insert(
                    (root.clone(), plan.suite.clone()),
                    SuiteFacts {
                        suite_source: target.suite_source.clone(),
                        cargo_env_match: target.cargo_env_match.as_str().to_owned(),
                        available_suites: available_suites.clone(),
                        suite_env: plan.suite_env.clone(),
                        suite_env_files: plan.suite_env_files.clone(),
                        setup_steps: plan.setup_steps,
                        teardown_steps: plan.teardown_steps,
                        teardown_policy: plan.teardown_policy,
                    },
                );
            }
        }
        Self { facts }
    }

    /// The facts for one executed suite's result.
    pub(super) fn for_result(&self, root: &Path, suite: &str) -> SuiteFacts {
        let root = root.display().to_string();
        self.facts
            .get(&(root, suite.to_owned()))
            .cloned()
            .unwrap_or_else(|| SuiteFacts::unplanned(suite))
    }
}

/// The one `<none>`/join spelling for a suite's env-file list.
///
/// Each rendering keeps its own separator (the plan text reads a list, the
/// compact result line does not); the empty-list word is shared.
pub(super) fn render_suite_env_files(files: &[String], separator: &str) -> String {
    if files.is_empty() {
        "<none>".to_owned()
    } else {
        files.join(separator)
    }
}
