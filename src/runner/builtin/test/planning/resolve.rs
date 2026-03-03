use crate::testing::{detect_test_runner_plans, TestRunner};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::runner::builtin::test::planning::{BuiltinResolvedPlan, BuiltinTestTarget};
use crate::runner::util::normalize_builtin_test_suite;
use crate::runner::{LoadedCatalog, ManifestJsPackageManager};

struct BuiltinTestTargetConfig {
    configured_suites: BTreeMap<String, String>,
    package_manager: Option<ManifestJsPackageManager>,
    runner_overrides: BTreeMap<String, String>,
}

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
    let config = resolve_target_test_config(catalogs, target_root);
    if !config.configured_suites.is_empty() {
        return (
            config
                .configured_suites
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

    (
        detect_test_runner_plans(target_root)
            .into_iter()
            .map(|plan| {
                apply_builtin_test_runner_config(
                    plan,
                    config.package_manager,
                    &config.runner_overrides,
                )
            })
            .map(|plan| BuiltinResolvedPlan {
                suite: plan.runner.label().to_owned(),
                command: plan.command,
                evidence: plan.evidence,
            })
            .collect::<Vec<BuiltinResolvedPlan>>(),
        "auto-detected".to_owned(),
    )
}

fn resolve_target_test_config(
    catalogs: &[LoadedCatalog],
    target_root: &Path,
) -> BuiltinTestTargetConfig {
    let catalog = catalog_for_root(catalogs, target_root);
    let configured_suites = catalog
        .and_then(|entry| entry.manifest.test.as_ref())
        .map(|test| {
            test.suites
                .iter()
                .filter_map(|(raw_suite, suite)| {
                    suite
                        .run()
                        .map(|command| (normalize_suite_key(raw_suite), command.to_owned()))
                })
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();
    let package_manager =
        catalog.and_then(|entry| entry.manifest.package_manager.as_ref().and_then(|pm| pm.js));
    let runner_overrides = catalog
        .and_then(|entry| entry.manifest.test.as_ref())
        .map(|test| {
            test.runners
                .iter()
                .filter_map(|(raw_runner, override_config)| {
                    override_config
                        .command()
                        .map(|command| (normalize_suite_key(raw_runner), command.to_owned()))
                })
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();
    BuiltinTestTargetConfig {
        configured_suites,
        package_manager,
        runner_overrides,
    }
}

fn catalog_for_root<'a>(
    catalogs: &'a [LoadedCatalog],
    target_root: &Path,
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == target_root)
}

fn normalize_suite_key(raw: &str) -> String {
    normalize_builtin_test_suite(raw).unwrap_or(raw).to_owned()
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
