use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::runner::builtin::test::planning::{BuiltinResolvedPlan, BuiltinTestTarget};
use crate::runner::model::catalog::LoadedCatalog;
use crate::runner::RunnerError;

use super::plan_resolution::resolve_target_test_plans;

pub(super) fn resolve_builtin_test_targets(
    prefix: Option<&str>,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<BuiltinTestTarget>, RunnerError> {
    if let Some(prefix) = prefix {
        return resolve_prefixed_target(prefix, catalogs);
    }

    collect_workspace_targets(resolved_root, catalogs)
}

fn resolve_prefixed_target(
    prefix: &str,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<BuiltinTestTarget>, RunnerError> {
    let Some(catalog) = catalogs.iter().find(|catalog| catalog.alias == prefix) else {
        return Ok(Vec::new());
    };
    let (plans, suite_source, cargo_env, cargo_env_match) =
        resolve_target_test_plans(catalogs, &catalog.catalog_root)?;
    if plans.is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![BuiltinTestTarget {
        name: catalog.alias.clone(),
        root: catalog.catalog_root.clone(),
        fallback_chain: render_fallback_chain(&plans),
        plans,
        suite_source,
        cargo_env,
        cargo_env_match,
    }])
}

fn collect_workspace_targets(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<BuiltinTestTarget>, RunnerError> {
    let mut targets = Vec::new();
    for (root, name) in collect_target_roots(resolved_root, catalogs) {
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
    Ok(targets)
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
