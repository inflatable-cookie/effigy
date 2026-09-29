use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::test::planning::{BuiltinResolvedPlan, BuiltinTargetRuntime, BuiltinTestTarget, BuiltinTestTargetSet};
use crate::BuiltinError;
use effigy_manifest::{LoadedCatalog, ManifestTask};

use super::plan_resolution::resolve_target_test_plans;
use effigy_tasks::testing::vitest_transitive_bin_skip_reason;

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
            fallback_chain: render_fallback_chain(&plans, &suite_source, &catalog.catalog_root),
            plans,
            suite_source,
            cargo_env,
            cargo_env_match,
            runtime: resolve_target_runtime(&catalog.manifest, &catalog.catalog_root),
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
            fallback_chain: render_fallback_chain(&plans, &suite_source, &root),
            root: root.clone(),
            plans,
            suite_source,
            cargo_env,
            cargo_env_match,
            runtime: catalogs
                .iter()
                .find(|catalog| catalog.catalog_root == root)
                .map(|catalog| resolve_target_runtime(&catalog.manifest, &root))
                .unwrap_or(BuiltinTargetRuntime::Host),
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

/// Resolve the owning catalog's declared runtime target for its test suites.
///
/// This is the same execution-binding grammar the task runner resolves
/// (`[systems]` default → workspace → named container), so builtin test
/// targets and named tasks share one authoritative resolved selection path.
/// A catalog without a declared runtime target keeps its suites on the host;
/// a declared but unusable target is surfaced instead of silently choosing a
/// container by location or falling back to the host.
pub(super) fn resolve_target_runtime(
    manifest: &effigy_manifest::TaskManifest,
    target_root: &Path,
) -> BuiltinTargetRuntime {
    match effigy_manifest::resolve_task_execution_binding(
        manifest,
        "builtin test target",
        &ManifestTask::default(),
    ) {
        Ok(Some(effigy_manifest::ResolvedTaskExecutionBinding::Workspace(binding))) => {
            match binding.container {
                Some(effigy_manifest::ResolvedWorkspaceContainer::Named(container)) => {
                    BuiltinTargetRuntime::Container { container }
                }
                Some(effigy_manifest::ResolvedWorkspaceContainer::Inline(_)) => {
                    BuiltinTargetRuntime::Unusable {
                        reason: format!(
                            "test target {} declares an inline workspace container; builtin test suites require a named container target",
                            target_root.display()
                        ),
                    }
                }
                None => BuiltinTargetRuntime::Unusable {
                    reason: format!(
                        "test target {} declares a workspace runtime without a backing container",
                        target_root.display()
                    ),
                },
            }
        }
        Ok(_) => BuiltinTargetRuntime::Host,
        Err(error) => BuiltinTargetRuntime::Unusable {
            reason: format!(
                "test target {} declares an unusable runtime target: {error}",
                target_root.display()
            ),
        },
    }
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

fn render_fallback_chain(
    plans: &[BuiltinResolvedPlan],
    suite_source: &str,
    target_root: &Path,
) -> Vec<String> {
    let mut chain = plans
        .iter()
        .map(|plan| {
            format!(
                "{} -> {} (selected): {}",
                plan.suite,
                plan.command,
                plan.evidence.join("; ")
            )
        })
        .collect::<Vec<String>>();
    if suite_source == "auto-detected" && !plans.iter().any(|plan| plan.suite == "vitest") {
        if let Some(reason) = vitest_transitive_bin_skip_reason(target_root) {
            chain.push(format!("vitest skipped: {reason}"));
        }
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::resolve_target_runtime;
    use crate::test::planning::BuiltinTargetRuntime;
    use effigy_manifest::{load_task_manifest, ManifestTask};
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn manifest_from_toml(body: &str) -> effigy_manifest::TaskManifest {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-builtin-target-runtime-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&root).expect("mkdir temp manifest root");
        let path = root.join("effigy.toml");
        fs::write(&path, body).expect("write manifest");
        load_task_manifest(&path).expect("load manifest")
    }

    #[test]
    fn target_without_declared_runtime_stays_host() {
        let manifest = manifest_from_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[test.suites.unit]
run = "cargo test"
"#,
        );

        assert_eq!(
            resolve_target_runtime(&manifest, Path::new("/tmp/target")),
            BuiltinTargetRuntime::Host
        );
    }

    #[test]
    fn target_with_named_container_runtime_resolves_container() {
        let manifest = manifest_from_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );

        assert_eq!(
            resolve_target_runtime(&manifest, Path::new("/tmp/target")),
            BuiltinTargetRuntime::Container {
                container: "web".to_owned()
            }
        );
    }

    #[test]
    fn inline_workspace_runtime_is_unusable_for_suites() {
        let manifest = manifest_from_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
"#,
        );

        match resolve_target_runtime(&manifest, Path::new("/tmp/target")) {
            BuiltinTargetRuntime::Unusable { reason } => {
                assert!(reason.contains("inline workspace container"), "{reason}");
            }
            other => panic!("expected unusable runtime, got {other:?}"),
        }
    }

    #[test]
    fn malformed_runtime_target_is_unusable() {
        let manifest = manifest_from_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "missing"

[systems.dev.workspaces.app]
container = "web"
"#,
        );

        match resolve_target_runtime(&manifest, Path::new("/tmp/target")) {
            BuiltinTargetRuntime::Unusable { .. } => {}
            other => panic!("expected unusable runtime, got {other:?}"),
        }
    }

    #[test]
    fn default_task_resolution_never_inherits_task_level_run_in() {
        // A task with run_in = host on the same manifest would resolve Host;
        // the catalog-level target resolution uses a default task so only the
        // declared systems binding decides the suite runtime.
        let manifest = manifest_from_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
primary_service = "app"

[tasks.some-task]
run_in = "host"
run = "printf host"
"#,
        );

        assert_eq!(
            resolve_target_runtime(&manifest, Path::new("/tmp/target")),
            BuiltinTargetRuntime::Container {
                container: "web".to_owned()
            }
        );
        let _ = ManifestTask::default();
    }
}
