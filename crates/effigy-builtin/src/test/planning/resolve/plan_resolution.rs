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

type ResolvedTargetTestPlans = (
    Vec<BuiltinResolvedPlan>,
    String,
    BTreeMap<String, String>,
    ManifestCargoEnvMatchMode,
);

pub(super) fn resolve_target_test_plans(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> Result<ResolvedTargetTestPlans, BuiltinError> {
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
                         is_default,
                         nested_invocation,
                     }| BuiltinResolvedPlan {
                        suite: suite.clone(),
                        command,
                        auto_workspace_scope: false,
                        env,
                        suite_env,
                        suite_env_files,
                        setup_command,
                        setup_steps,
                        teardown_command,
                        teardown_steps,
                        teardown_policy,
                        is_default,
                        evidence: vec![format!("test.suites.{suite}")],
                        nested_invocation,
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
            .map(|plan| {
                // Mark the workspace flag before any runner override can
                // replace the command: only detection-owned `--workspace`
                // flags yield to explicit package scope.
                let detected_workspace_scope = plan
                    .command
                    .split_whitespace()
                    .any(|token| token == "--workspace");
                let overridden = runner_overrides.contains_key(plan.runner.label());
                let plan =
                    apply_builtin_test_runner_config(plan, package_manager, &runner_overrides);
                (plan, detected_workspace_scope && !overridden)
            })
            .map(|(plan, auto_workspace_scope)| BuiltinResolvedPlan {
                suite: plan.runner.label().to_owned(),
                command: plan.command,
                auto_workspace_scope,
                env: BTreeMap::new(),
                suite_env: None,
                suite_env_files: Vec::new(),
                setup_command: None,
                setup_steps: 0,
                teardown_command: None,
                teardown_steps: 0,
                teardown_policy: effigy_manifest::ManifestTestSuiteTeardownPolicy::OnSuccess,
                is_default: true,
                evidence: plan.evidence,
                nested_invocation: false,
            })
            .collect::<Vec<BuiltinResolvedPlan>>(),
        "auto-detected".to_owned(),
        cargo_env,
        cargo_env_match,
    ))
}
