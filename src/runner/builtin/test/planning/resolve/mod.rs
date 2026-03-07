use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::runner::manifest::config_sections::ManifestJsPackageManager;
use crate::runner::model::catalog::LoadedCatalog;
use crate::runner::tooling::vitest_command_for_js_package_manager;
use crate::runner::RunnerError;

mod cargo_env;
mod plan_resolution;
mod target_config;
#[path = "targets.rs"]
mod targets;

pub(super) fn resolve_builtin_test_targets(
    prefix: Option<&str>,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<BuiltinTestTarget>, RunnerError> {
    targets::resolve_builtin_test_targets(prefix, resolved_root, catalogs)
}

pub(super) fn apply_builtin_test_runner_config(
    mut plan: crate::testing::TestRunnerPlan,
    package_manager: Option<ManifestJsPackageManager>,
    runner_overrides: &BTreeMap<String, String>,
) -> crate::testing::TestRunnerPlan {
    if plan.runner == crate::testing::TestRunner::Vitest {
        if let Some(manager) = package_manager {
            let (command, manager_label) = vitest_command_for_js_package_manager(manager);
            plan.command = command.to_owned();
            plan.evidence
                .push(format!("package_manager.js={manager_label}"));
        }
    }

    if let Some(command) = runner_overrides.get(plan.runner.label()) {
        plan.command = command.clone();
        plan.evidence.push(format!(
            "test.runners.{} command override applied",
            plan.runner.label()
        ));
    }
    plan
}
