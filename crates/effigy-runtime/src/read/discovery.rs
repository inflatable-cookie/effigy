use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use effigy_containers::{
    exec::{
        infer_host_working_dir_for_container, list_running_compose_containers_profiled,
        RunningComposeContainer,
    },
    load_all_container_policies, EffectiveContainerPolicy,
};

use crate::EffigyRuntimeError;

#[derive(Debug)]
pub(crate) struct DiscoveredRunningEnvironment {
    pub(crate) repo_root: String,
    pub(crate) runtime_profile: String,
    pub(crate) policy: EffectiveContainerPolicy,
    pub(crate) services: Vec<RunningComposeContainer>,
}

/// Maximum directory levels to walk up from a container's
/// `com.docker.compose.project.working_dir` label looking for an
/// `effigy.toml` marker.
///
/// Generated compose stacks live at `<repo>/.effigy/runtime/compose/`,
/// which Docker labels as the project working_dir (three levels deep).
/// `MAX_REPO_ROOT_WALKUP` allows for that plus a small grace margin so
/// future relocations of the compose payload still resolve.
pub const MAX_REPO_ROOT_WALKUP: usize = 6;

pub(crate) fn discover_running_environments(
) -> Result<Vec<DiscoveredRunningEnvironment>, EffigyRuntimeError> {
    let rows = list_running_compose_containers_profiled()
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let mut grouped: BTreeMap<(String, String), Vec<_>> = BTreeMap::new();
    for profiled in rows {
        let row = profiled.row;
        let Some(project_name) = row.project_name.clone() else {
            continue;
        };
        grouped
            .entry((profiled.profile, project_name))
            .or_default()
            .push(row);
    }

    let mut environments = Vec::new();
    for ((profile, project_name), mut services) in grouped {
        let Some(repo_root) = resolve_repo_root_for_project_rows(&profile, &services) else {
            continue;
        };
        let repo_path = Path::new(&repo_root);
        let Ok(policies) = load_all_container_policies(repo_path) else {
            continue;
        };
        let Some(policy) = policies
            .into_iter()
            .find(|policy| policy.project_name == project_name)
        else {
            continue;
        };
        services.sort_by(|left, right| {
            left.service
                .as_deref()
                .unwrap_or(left.container_name.as_str())
                .cmp(
                    right
                        .service
                        .as_deref()
                        .unwrap_or(right.container_name.as_str()),
                )
        });

        environments.push(DiscoveredRunningEnvironment {
            repo_root,
            runtime_profile: profile,
            policy,
            services,
        });
    }
    environments.sort_by(|left, right| {
        left.repo_root
            .cmp(&right.repo_root)
            .then(left.policy.name.cmp(&right.policy.name))
    });
    Ok(environments)
}

pub(crate) fn filter_running_environments_for_scope(
    environments: Vec<DiscoveredRunningEnvironment>,
    scope_root: &Path,
    name: Option<&str>,
) -> Vec<DiscoveredRunningEnvironment> {
    let canonical_scope = canonicalize_or_original(scope_root);
    environments
        .into_iter()
        .filter(|environment| {
            let repo_root = canonicalize_or_original(Path::new(&environment.repo_root));
            repo_root.starts_with(&canonical_scope)
                && name.is_none_or(|requested| environment.policy.name == requested)
        })
        .collect()
}

pub(crate) fn discover_effigy_repos_under(scope_root: &Path) -> Vec<PathBuf> {
    let mut discovered = BTreeSet::new();
    let mut stack = vec![canonicalize_or_original(scope_root)];

    while let Some(dir) = stack.pop() {
        if dir.join("effigy.toml").is_file() {
            discovered.insert(dir.clone());
        }

        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            if skip_effigy_repo_discovery_dir(&entry.file_name()) {
                continue;
            }
            stack.push(entry.path());
        }
    }

    discovered.into_iter().collect()
}

/// Walk up from `start` looking for an `effigy.toml` marker.
///
/// Returns the first ancestor directory (inclusive of `start`) that
/// contains an `effigy.toml`, or `None` if no marker is found within
/// `max_depth` ancestors.
pub fn resolve_effigy_repo_root(start: &Path, max_depth: usize) -> Option<PathBuf> {
    let mut current = start;
    for _ in 0..=max_depth {
        if current.join("effigy.toml").is_file() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
    None
}

/// Whether a Docker compose `working_dir` label points into `repo_root`.
///
/// Generated compose stacks emit a label like
/// `<repo>/.effigy/runtime/compose/`, which is not equal to the repo root
/// itself. Walk up from the labelled directory looking for `effigy.toml`
/// and compare the resolved owning repo against `repo_root`. Falls back to
/// an exact match when no marker is found within the walk-up budget.
pub fn working_dir_belongs_to_repo(working_dir: &str, repo_root: &Path) -> bool {
    let working_dir_path = Path::new(working_dir);
    match resolve_effigy_repo_root(working_dir_path, MAX_REPO_ROOT_WALKUP) {
        Some(resolved) => resolved == repo_root,
        None => working_dir_path == repo_root,
    }
}

fn resolve_repo_root_for_project_rows(
    profile: &str,
    rows: &[RunningComposeContainer],
) -> Option<String> {
    rows.iter().find_map(|row| {
        let working_dir = row.working_dir.clone().or_else(|| {
            infer_host_working_dir_for_container(profile, &row.container_name)
                .ok()
                .flatten()
        })?;
        let repo_path = resolve_effigy_repo_root(Path::new(&working_dir), MAX_REPO_ROOT_WALKUP)?;
        Some(repo_path.display().to_string())
    })
}

fn skip_effigy_repo_discovery_dir(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(".git")
            | Some(".effigy")
            | Some("external")
            | Some("node_modules")
            | Some("target")
            | Some("vendor")
    )
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{
        discover_effigy_repos_under, filter_running_environments_for_scope,
        resolve_effigy_repo_root, resolve_repo_root_for_project_rows, DiscoveredRunningEnvironment,
        MAX_REPO_ROOT_WALKUP,
    };
    use effigy_containers::exec::RunningComposeContainer;
    use effigy_containers::EffectiveContainerPolicy;
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_effigy_repo_root_returns_repo_when_started_at_repo_root() {
        let temp = tempdir().expect("tempdir");
        let repo = temp.path();
        fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");

        let resolved = resolve_effigy_repo_root(repo, MAX_REPO_ROOT_WALKUP).expect("resolved");

        assert_eq!(resolved, repo);
    }

    #[test]
    fn resolve_effigy_repo_root_walks_up_from_generated_compose_dir() {
        let temp = tempdir().expect("tempdir");
        let repo = temp.path();
        fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");
        let compose_dir = repo.join(".effigy/runtime/compose");
        fs::create_dir_all(&compose_dir).expect("compose dir");

        let resolved =
            resolve_effigy_repo_root(&compose_dir, MAX_REPO_ROOT_WALKUP).expect("resolved");

        assert_eq!(resolved, repo);
    }

    #[test]
    fn resolve_effigy_repo_root_returns_none_when_no_marker_present() {
        let temp = tempdir().expect("tempdir");
        let stray = temp.path().join("a/b/c");
        fs::create_dir_all(&stray).expect("stray dir");

        let resolved = resolve_effigy_repo_root(&stray, 2);

        assert!(resolved.is_none(), "got: {resolved:?}");
    }

    #[test]
    fn resolve_effigy_repo_root_respects_max_depth() {
        let temp = tempdir().expect("tempdir");
        let repo = temp.path();
        fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");
        let deep = repo.join("a/b/c/d/e/f/g/h");
        fs::create_dir_all(&deep).expect("deep dir");

        // Eight levels deep, which exceeds MAX_REPO_ROOT_WALKUP.
        let resolved = resolve_effigy_repo_root(&deep, MAX_REPO_ROOT_WALKUP);
        assert!(
            resolved.is_none(),
            "expected None for excessive depth, got: {resolved:?}"
        );
    }

    #[test]
    fn filter_running_environments_for_scope_matches_descendant_repos_only() {
        let temp = tempdir().expect("tempdir");
        let scope_root = temp.path().join("test");
        let child_repo = scope_root.join("cbs");
        let other_repo = temp.path().join("other");
        fs::create_dir_all(&child_repo).expect("child repo");
        fs::create_dir_all(&other_repo).expect("other repo");

        let policy = stub_policy("web");
        let filtered = filter_running_environments_for_scope(
            vec![
                stub_environment(child_repo.display().to_string(), policy.clone()),
                stub_environment(other_repo.display().to_string(), policy),
            ],
            &scope_root,
            None,
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].repo_root, child_repo.display().to_string());
    }

    #[test]
    fn filter_running_environments_for_scope_honors_container_name() {
        let temp = tempdir().expect("tempdir");
        let scope_root = temp.path().join("test");
        let child_repo = scope_root.join("cbs");
        fs::create_dir_all(&child_repo).expect("child repo");

        let filtered = filter_running_environments_for_scope(
            vec![
                stub_environment(child_repo.display().to_string(), stub_policy("web")),
                stub_environment(child_repo.display().to_string(), stub_policy("db")),
            ],
            &scope_root,
            Some("db"),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].policy.name, "db");
    }

    #[test]
    fn discover_effigy_repos_under_finds_scope_and_descendants() {
        let temp = tempdir().expect("tempdir");
        let scope_root = temp.path().join("scope");
        let nested_repo = scope_root.join("apps/demo");
        fs::create_dir_all(&nested_repo).expect("mkdir nested repo");
        fs::write(scope_root.join("effigy.toml"), "[manifest]\n").expect("write scope manifest");
        fs::write(nested_repo.join("effigy.toml"), "[manifest]\n").expect("write nested manifest");

        let discovered = discover_effigy_repos_under(&scope_root);
        let scope_root = fs::canonicalize(&scope_root).expect("canonical scope root");
        let nested_repo = fs::canonicalize(&nested_repo).expect("canonical nested repo");

        assert_eq!(discovered.len(), 2);
        assert!(discovered.contains(&scope_root));
        assert!(discovered.contains(&nested_repo));
    }

    #[test]
    fn discover_effigy_repos_under_skips_heavy_runtime_and_external_dirs() {
        let temp = tempdir().expect("tempdir");
        let scope_root = temp.path().join("scope");
        let target_repo = scope_root.join("target/not-a-real-repo");
        let node_modules_repo = scope_root.join("node_modules/not-a-real-repo");
        let external_repo = scope_root.join("external/not-a-real-repo");
        let real_repo = scope_root.join("apps/demo");
        fs::create_dir_all(&target_repo).expect("mkdir target repo");
        fs::create_dir_all(&node_modules_repo).expect("mkdir node_modules repo");
        fs::create_dir_all(&external_repo).expect("mkdir external repo");
        fs::create_dir_all(&real_repo).expect("mkdir real repo");
        fs::write(target_repo.join("effigy.toml"), "[manifest]\n").expect("write target manifest");
        fs::write(node_modules_repo.join("effigy.toml"), "[manifest]\n")
            .expect("write node_modules manifest");
        fs::write(external_repo.join("effigy.toml"), "[manifest]\n")
            .expect("write external manifest");
        fs::write(real_repo.join("effigy.toml"), "[manifest]\n").expect("write real manifest");

        let discovered = discover_effigy_repos_under(&scope_root);
        let real_repo = fs::canonicalize(&real_repo).expect("canonical real repo");

        assert_eq!(discovered, vec![real_repo]);
    }

    #[test]
    fn resolve_repo_root_for_project_rows_anchors_from_any_service_row() {
        let temp = tempdir().expect("tempdir");
        let repo_root = temp.path().join("demo");
        let compose_dir = repo_root.join(".effigy/runtime/compose");
        fs::create_dir_all(&compose_dir).expect("mkdir compose dir");
        fs::write(repo_root.join("effigy.toml"), "[manifest]\n").expect("write manifest");

        let rows = vec![
            RunningComposeContainer {
                container_name: "demo-db-1".to_owned(),
                status: "Up".to_owned(),
                ports: vec![],
                project_name: Some("demo".to_owned()),
                working_dir: None,
                service: Some("db".to_owned()),
            },
            RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "Up".to_owned(),
                ports: vec![],
                project_name: Some("demo".to_owned()),
                working_dir: Some(compose_dir.display().to_string()),
                service: Some("app".to_owned()),
            },
        ];

        let resolved =
            resolve_repo_root_for_project_rows("effigy", &rows).expect("resolved repo root");

        assert_eq!(resolved, repo_root.display().to_string());
    }

    fn stub_environment(
        repo_root: String,
        policy: EffectiveContainerPolicy,
    ) -> DiscoveredRunningEnvironment {
        DiscoveredRunningEnvironment {
            repo_root,
            runtime_profile: "effigy".to_owned(),
            policy,
            services: vec![RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "Up".to_owned(),
                ports: vec![],
                project_name: Some("demo".to_owned()),
                working_dir: None,
                service: Some("app".to_owned()),
            }],
        }
    }

    fn stub_policy(name: &str) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: name.to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Generated,
            compose_files: vec![],
            compose_file_display: String::new(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: format!("{name}-project"),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: vec![],
        }
    }
}
