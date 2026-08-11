use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::test::planning::{BuiltinResolvedPlan, BuiltinTestTarget, BuiltinTestTargetSet};
use crate::BuiltinError;
use effigy_manifest::LoadedCatalog;

use super::plan_resolution::resolve_target_test_plans;

pub(super) fn resolve_builtin_test_targets(
    prefix: Option<&str>,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<BuiltinTestTargetSet, BuiltinError> {
    if let Some(prefix) = prefix {
        return resolve_prefixed_target(prefix, catalogs);
    }

    collect_workspace_targets(resolved_root, catalogs)
}

fn resolve_prefixed_target(
    prefix: &str,
    catalogs: &[LoadedCatalog],
) -> Result<BuiltinTestTargetSet, BuiltinError> {
    let Some(catalog) = catalogs.iter().find(|catalog| catalog.alias == prefix) else {
        return Ok(BuiltinTestTargetSet {
            targets: Vec::new(),
            excluded_targets: Vec::new(),
            warnings: Vec::new(),
        });
    };
    let (plans, suite_source, cargo_env, cargo_env_match) =
        resolve_target_test_plans(catalogs, &catalog.catalog_root)?;
    if plans.is_empty() {
        return Ok(BuiltinTestTargetSet {
            targets: Vec::new(),
            excluded_targets: Vec::new(),
            warnings: Vec::new(),
        });
    }

    Ok(BuiltinTestTargetSet {
        targets: vec![BuiltinTestTarget {
            name: catalog.alias.clone(),
            root: catalog.catalog_root.clone(),
            fallback_chain: render_fallback_chain(&plans),
            plans,
            suite_source,
            cargo_env,
            cargo_env_match,
        }],
        excluded_targets: Vec::new(),
        warnings: Vec::new(),
    })
}

fn collect_workspace_targets(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<BuiltinTestTargetSet, BuiltinError> {
    let excluded_targets = workspace_exclusions(resolved_root, catalogs)?;
    let mut targets = Vec::new();
    for (root, name) in collect_target_roots(resolved_root, catalogs) {
        if excluded_targets.binary_search(&name).is_ok() {
            continue;
        }
        let (plans, suite_source, cargo_env, cargo_env_match) =
            resolve_target_test_plans(catalogs, &root)?;
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
    let warnings = overlapping_cargo_target_warnings(&targets);
    Ok(BuiltinTestTargetSet {
        targets,
        excluded_targets,
        warnings,
    })
}

fn workspace_exclusions(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<String>, BuiltinError> {
    let excluded = catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == resolved_root)
        .and_then(|catalog| catalog.manifest.test.as_ref())
        .map(|test| {
            test.exclude_catalogs
                .iter()
                .cloned()
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    for alias in &excluded {
        if !catalogs.iter().any(|catalog| &catalog.alias == alias) {
            return Err(BuiltinError::task_invocation(format!(
                "test.exclude_catalogs names unknown catalog `{alias}`"
            )));
        }
    }
    Ok(excluded)
}

fn overlapping_cargo_target_warnings(targets: &[BuiltinTestTarget]) -> Vec<String> {
    let mut warnings = Vec::new();
    for (index, parent) in targets.iter().enumerate() {
        for child in targets.iter().skip(index + 1) {
            let (parent, child) = if child.root.starts_with(&parent.root) {
                (parent, child)
            } else if parent.root.starts_with(&child.root) {
                (child, parent)
            } else {
                continue;
            };
            if target_uses_cargo(parent) && target_uses_cargo(child) {
                warnings.push(format!(
                    "overlapping Cargo targets `{}` ({}) and `{}` ({}); exclude the child catalog from root fanout when the parent workspace already owns it",
                    parent.name,
                    parent.root.display(),
                    child.name,
                    child.root.display()
                ));
            }
        }
    }
    warnings
}

fn target_uses_cargo(target: &BuiltinTestTarget) -> bool {
    target
        .plans
        .iter()
        .any(|plan| plan.command.contains("cargo ") || plan.command.contains("cargo-nextest"))
}

fn collect_target_roots(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> BTreeMap<PathBuf, String> {
    let mut roots = BTreeMap::<PathBuf, String>::new();
    for catalog in catalogs {
        roots
            .entry(catalog.catalog_root.clone())
            .or_insert_with(|| catalog.alias.clone());
    }
    if !roots.contains_key(resolved_root) {
        roots.insert(resolved_root.to_path_buf(), "root".to_owned());
    }
    roots
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
        .collect()
}
