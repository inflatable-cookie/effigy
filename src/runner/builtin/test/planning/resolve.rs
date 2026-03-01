use crate::testing::{detect_test_runner_plans, TestRunner};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::runner::builtin::test::planning::{BuiltinResolvedPlan, BuiltinTestTarget};
use crate::runner::util::normalize_builtin_test_suite;
use crate::runner::{LoadedCatalog, ManifestJsPackageManager};

pub(super) fn resolve_builtin_test_targets(
    prefix: Option<&str>,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<BuiltinTestTarget> {
    if let Some(prefix) = prefix {
        if let Some(catalog) = catalogs.iter().find(|catalog| catalog.alias == prefix) {
            let (plans, suite_source) = resolve_target_test_plans(catalogs, &catalog.catalog_root);
            if plans.is_empty() {
                return Vec::new();
            }
            return vec![BuiltinTestTarget {
                name: catalog.alias.clone(),
                root: catalog.catalog_root.clone(),
                fallback_chain: render_fallback_chain(&plans),
                plans,
                suite_source,
            }];
        }
        return Vec::new();
    }

    let mut targets = Vec::<BuiltinTestTarget>::new();
    let mut roots = HashMap::<PathBuf, String>::new();
    for catalog in catalogs {
        roots
            .entry(catalog.catalog_root.clone())
            .or_insert_with(|| catalog.alias.clone());
    }
    if !roots.contains_key(resolved_root) {
        roots.insert(resolved_root.to_path_buf(), "root".to_owned());
    }
    let mut ordered = roots.into_iter().collect::<Vec<(PathBuf, String)>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (root, name) in ordered {
        let (plans, suite_source) = resolve_target_test_plans(catalogs, &root);
        if plans.is_empty() {
            continue;
        }
        targets.push(BuiltinTestTarget {
            name,
            fallback_chain: render_fallback_chain(&plans),
            root,
            plans,
            suite_source,
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

fn resolve_target_test_plans(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> (Vec<BuiltinResolvedPlan>, String) {
    let configured = builtin_test_configured_suites(catalogs, target_root);
    if !configured.is_empty() {
        return (
            configured
                .into_iter()
                .map(|(suite, command)| BuiltinResolvedPlan {
                    suite: suite.clone(),
                    command,
                    evidence: vec![format!("test.suites.{suite}")],
                })
                .collect::<Vec<BuiltinResolvedPlan>>(),
            "configured".to_owned(),
        );
    }

    let package_manager = builtin_test_package_manager(catalogs, target_root);
    let runner_overrides = builtin_test_runner_command_overrides(catalogs, target_root);
    (
        detect_test_runner_plans(target_root)
            .into_iter()
            .map(|plan| apply_builtin_test_runner_config(plan, package_manager, &runner_overrides))
            .map(|plan| BuiltinResolvedPlan {
                suite: plan.runner.label().to_owned(),
                command: plan.command,
                evidence: plan.evidence,
            })
            .collect::<Vec<BuiltinResolvedPlan>>(),
        "auto-detected".to_owned(),
    )
}

fn builtin_test_package_manager(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> Option<ManifestJsPackageManager> {
    catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == target_root)
        .find_map(|catalog| {
            catalog
                .manifest
                .package_manager
                .as_ref()
                .and_then(|pm| pm.js)
        })
}

fn builtin_test_configured_suites(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> BTreeMap<String, String> {
    catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == target_root)
        .find_map(|catalog| {
            catalog.manifest.test.as_ref().map(|test| {
                test.suites
                    .iter()
                    .filter_map(|(raw_suite, suite)| {
                        suite.run().map(|command| {
                            let key = normalize_builtin_test_suite(raw_suite)
                                .unwrap_or(raw_suite.as_str())
                                .to_owned();
                            (key, command.to_owned())
                        })
                    })
                    .collect::<BTreeMap<String, String>>()
            })
        })
        .unwrap_or_default()
}

fn builtin_test_runner_command_overrides(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> BTreeMap<String, String> {
    catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == target_root)
        .find_map(|catalog| {
            catalog.manifest.test.as_ref().map(|test| {
                test.runners
                    .iter()
                    .filter_map(|(raw_runner, override_config)| {
                        override_config.command().map(|command| {
                            let key = normalize_builtin_test_suite(raw_runner)
                                .unwrap_or(raw_runner.as_str())
                                .to_owned();
                            (key, command.to_owned())
                        })
                    })
                    .collect::<BTreeMap<String, String>>()
            })
        })
        .unwrap_or_default()
}

fn apply_builtin_test_runner_config(
    mut plan: crate::testing::TestRunnerPlan,
    package_manager: Option<ManifestJsPackageManager>,
    runner_overrides: &BTreeMap<String, String>,
) -> crate::testing::TestRunnerPlan {
    if plan.runner == TestRunner::Vitest {
        if let Some(manager) = package_manager {
            let (command, manager_label) = match manager {
                ManifestJsPackageManager::Bun => ("bun x vitest run", "bun"),
                ManifestJsPackageManager::Pnpm => ("pnpm exec vitest run", "pnpm"),
                ManifestJsPackageManager::Npm => ("npx vitest run", "npm"),
                ManifestJsPackageManager::Direct => ("vitest run", "direct"),
            };
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
