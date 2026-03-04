use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::runner::builtin::test::planning::{BuiltinResolvedPlan, BuiltinTestTarget};
use crate::runner::tooling::vitest_command_for_js_package_manager;
use crate::runner::{LoadedCatalog, ManifestJsPackageManager};

use self::plan_resolution::resolve_target_test_plans;

mod cargo_env;
mod plan_resolution;
mod target_config;

pub(super) fn resolve_builtin_test_targets(
    prefix: Option<&str>,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<BuiltinTestTarget> {
    if let Some(prefix) = prefix {
        if let Some(catalog) = catalogs.iter().find(|catalog| catalog.alias == prefix) {
            let (plans, suite_source, cargo_env, cargo_env_match) =
                resolve_target_test_plans(catalogs, &catalog.catalog_root);
            if plans.is_empty() {
                return Vec::new();
            }
            return vec![BuiltinTestTarget {
                name: catalog.alias.clone(),
                root: catalog.catalog_root.clone(),
                fallback_chain: render_fallback_chain(&plans),
                plans,
                suite_source,
                cargo_env,
                cargo_env_match,
            }];
        }
        return Vec::new();
    }

    let mut targets = Vec::<BuiltinTestTarget>::new();
    let mut roots = BTreeMap::<PathBuf, String>::new();
    for catalog in catalogs {
        roots
            .entry(catalog.catalog_root.clone())
            .or_insert_with(|| catalog.alias.clone());
    }
    if !roots.contains_key(resolved_root) {
        roots.insert(resolved_root.to_path_buf(), "root".to_owned());
    }
    for (root, name) in roots {
        let (plans, suite_source, cargo_env, cargo_env_match) =
            resolve_target_test_plans(catalogs, &root);
        if plans.is_empty() {
            continue;
        }
        targets.push(BuiltinTestTarget {
            name,
            fallback_chain: render_fallback_chain(&plans),
            root,
            plans,
            suite_source,
            cargo_env,
            cargo_env_match,
        });
    }
    targets
}

fn render_fallback_chain(plans: &[BuiltinResolvedPlan]) -> Vec<String> {
    plans
        .iter()
        .map(|plan| {
            format!(
                "{} -> {} (selected): {}",
                plan.suite,
                plan.command,
                plan.evidence.join("; ")
            )
        })
        .collect::<Vec<String>>()
}

fn apply_builtin_test_runner_config(
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
