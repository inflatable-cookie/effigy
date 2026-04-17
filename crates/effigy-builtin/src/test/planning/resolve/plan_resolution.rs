use std::collections::BTreeMap;
use std::path::Path;

use crate::test::planning::BuiltinResolvedPlan;
use crate::BuiltinError;
use effigy_manifest::LoadedCatalog;
use effigy_manifest::ManifestCargoEnvMatchMode;
use effigy_tasks::testing::detect_test_runner_plans;

use super::apply_builtin_test_runner_config;
use super::target_config::{
    resolve_target_test_config, BuiltinConfiguredSuite, BuiltinTestTargetConfig,
};

pub(super) fn resolve_target_test_plans(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> Result<
    (
        Vec<BuiltinResolvedPlan>,
        String,
        BTreeMap<String, String>,
        ManifestCargoEnvMatchMode,
    ),
    BuiltinError,
> {
    let BuiltinTestTargetConfig {
        configured_suites,
        package_manager,
        runner_overrides,
        cargo_env,
        cargo_env_match,
    } = resolve_target_test_config(catalogs, target_root)?;

    if !configured_suites.is_empty() {
        return Ok((
            configured_suites
                .into_iter()
                .map(
                    |BuiltinConfiguredSuite {
                         suite,
                         command,
                         env,
                         suite_env,
                         suite_env_files,
                         setup_command,
                         setup_steps,
                         teardown_command,
                         teardown_steps,
                         teardown_policy,
                     }| BuiltinResolvedPlan {
                        suite: suite.clone(),
                        command,
                        env,
                        suite_env,
                        suite_env_files,
                        setup_command,
                        setup_steps,
                        teardown_command,
                        teardown_steps,
                        teardown_policy,
                        evidence: vec![format!("test.suites.{suite}")],
                    },
                )
                .collect::<Vec<BuiltinResolvedPlan>>(),
            "configured".to_owned(),
            cargo_env,
            cargo_env_match,
        ));
    }

    Ok((
        detect_test_runner_plans(target_root)
            .into_iter()
            .map(|plan| apply_builtin_test_runner_config(plan, package_manager, &runner_overrides))
            .map(|plan| BuiltinResolvedPlan {
                suite: plan.runner.label().to_owned(),
                command: plan.command,
                env: BTreeMap::new(),
                suite_env: None,
                suite_env_files: Vec::new(),
                setup_command: None,
                setup_steps: 0,
                teardown_command: None,
                teardown_steps: 0,
                teardown_policy: effigy_manifest::ManifestTestSuiteTeardownPolicy::OnSuccess,
                evidence: plan.evidence,
            })
            .collect::<Vec<BuiltinResolvedPlan>>(),
        "auto-detected".to_owned(),
        cargo_env,
        cargo_env_match,
    ))
}
