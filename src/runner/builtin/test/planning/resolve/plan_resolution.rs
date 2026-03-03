use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::builtin::test::planning::BuiltinResolvedPlan;
use crate::runner::manifest::ManifestCargoEnvMatchMode;
use crate::runner::LoadedCatalog;
use crate::testing::detect_test_runner_plans;

use super::target_config::{resolve_target_test_config, BuiltinTestTargetConfig};
use super::apply_builtin_test_runner_config;

pub(super) fn resolve_target_test_plans(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> (
    Vec<BuiltinResolvedPlan>,
    String,
    BTreeMap<String, String>,
    ManifestCargoEnvMatchMode,
) {
    let BuiltinTestTargetConfig {
        configured_suites,
        package_manager,
        runner_overrides,
        cargo_env,
        cargo_env_match,
    } = resolve_target_test_config(catalogs, target_root);

    if !configured_suites.is_empty() {
        return (
            configured_suites
                .into_iter()
                .map(|(suite, command)| BuiltinResolvedPlan {
                    suite: suite.clone(),
                    command,
                    evidence: vec![format!("test.suites.{suite}")],
                })
                .collect::<Vec<BuiltinResolvedPlan>>(),
            "configured".to_owned(),
            cargo_env,
            cargo_env_match,
        );
    }

    (
        detect_test_runner_plans(target_root)
            .into_iter()
            .map(|plan| {
                apply_builtin_test_runner_config(plan, package_manager, &runner_overrides)
            })
            .map(|plan| BuiltinResolvedPlan {
                suite: plan.runner.label().to_owned(),
                command: plan.command,
                evidence: plan.evidence,
            })
            .collect::<Vec<BuiltinResolvedPlan>>(),
        "auto-detected".to_owned(),
        cargo_env,
        cargo_env_match,
    )
}
