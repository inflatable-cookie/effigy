use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

use effigy_core::worktree_scope;
use effigy_manifest::{LoadedTaskManifest, ManifestContainerConfig, ManifestContainersConfig};

use super::model::ContainerPolicyError;

pub(crate) fn default_project_name_base(loaded: &LoadedTaskManifest, repo_root: &Path) -> String {
    loaded
        .manifest_defined_catalog_alias()
        .map(sanitize_project_name_component)
        .unwrap_or_else(|| {
            let repo = repo_root
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("repo");
            sanitize_project_name_component(repo)
        })
}

fn sanitize_project_name_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| match ch {
            ch if ch.is_ascii_alphanumeric() => ch.to_ascii_lowercase(),
            '-' | '_' => ch,
            _ => '-',
        })
        .collect::<String>()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
        .to_owned();

    if sanitized.is_empty() {
        "repo".to_owned()
    } else {
        sanitized
    }
}

pub(crate) fn resolve_project_name(
    config: &ManifestContainerConfig,
    default_project_name_base: &str,
    name: &str,
    container_count: usize,
    repo_root: &Path,
) -> Result<String, ContainerPolicyError> {
    let project_name = config
        .project_name
        .clone()
        .unwrap_or_else(|| default_project_name(default_project_name_base, name, container_count));
    let project_name = apply_bootstrap_fresh_session_suffix(repo_root, project_name);
    let project_name = scope_project_name(repo_root, project_name, config.share_runtime_identity)?;
    Ok(sanitize_project_name_component(&project_name))
}

pub(crate) fn scope_project_name(
    repo_root: &Path,
    project_name: String,
    share_runtime_identity: bool,
) -> Result<String, ContainerPolicyError> {
    if share_runtime_identity {
        Ok(project_name)
    } else if let Some(scope) = worktree_scope::load_or_create(repo_root).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "cannot establish linked-worktree runtime identity for {}: {error}",
            repo_root.display()
        ))
    })? {
        Ok(format!("{project_name}-wt-{}", &scope[..12]))
    } else {
        Ok(project_name)
    }
}

fn default_project_name(
    default_project_name_base: &str,
    name: &str,
    container_count: usize,
) -> String {
    if container_count <= 1 {
        return format!("{default_project_name_base}-dev");
    }
    format!("{default_project_name_base}-{name}-dev")
}

pub(crate) fn validate_unique_project_names(
    containers: &ManifestContainersConfig,
    default_project_name_base: &str,
    repo_root: &Path,
) -> Result<(), ContainerPolicyError> {
    if containers.environments.len() <= 1 {
        return Ok(());
    }

    let mut by_project_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, config) in &containers.environments {
        by_project_name
            .entry(resolve_project_name(
                config,
                default_project_name_base,
                name,
                containers.environments.len(),
                repo_root,
            )?)
            .or_default()
            .push(name.clone());
    }

    let duplicates = by_project_name
        .into_iter()
        .filter(|(_project_name, names)| names.len() > 1)
        .map(|(project_name, mut names)| {
            names.sort();
            format!(
                "`{project_name}` for containers {}",
                names
                    .into_iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        return Ok(());
    }

    Err(ContainerPolicyError::TaskInvocation(format!(
        "containers must resolve to unique `project_name` values when more than one container is declared; duplicate effective project names: {}",
        duplicates.join("; ")
    )))
}

fn apply_bootstrap_fresh_session_suffix(repo_root: &Path, project_name: String) -> String {
    let Some(session_id) = bootstrap_fresh_session_id(repo_root) else {
        return project_name;
    };
    format!("{project_name}-{session_id}")
}

fn bootstrap_fresh_session_id(repo_root: &Path) -> Option<String> {
    if let Some(value) = std::env::var("EFFIGY_BOOTSTRAP_FRESH_SESSION_ID")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        return Some(value);
    }

    let session_file = repo_root
        .join(".effigy")
        .join("runtime")
        .join("bootstrap-fresh-session.json");
    let source = std::fs::read_to_string(session_file).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&source).ok()?;
    if parsed.get("active").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }
    parsed
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_checkout_keeps_name_while_linked_worktrees_get_stable_distinct_names() {
        let root = tempfile::tempdir().unwrap();
        let primary = root.path().join("primary");
        std::fs::create_dir_all(primary.join(".git/worktrees")).unwrap();
        let config: ManifestContainerConfig = toml::from_str("project_name = 'app-dev'").unwrap();
        assert_eq!(
            resolve_project_name(&config, "app", "dev", 1, &primary).unwrap(),
            "app-dev"
        );
        let mut names = Vec::new();
        for name in ["one", "two"] {
            let checkout = root.path().join(name);
            let private = primary.join(".git/worktrees").join(name);
            std::fs::create_dir_all(&checkout).unwrap();
            std::fs::create_dir_all(&private).unwrap();
            std::fs::write(
                checkout.join(".git"),
                format!("gitdir: {}\n", private.display()),
            )
            .unwrap();
            std::fs::write(private.join("commondir"), "../..\n").unwrap();
            let value = resolve_project_name(&config, "app", "dev", 1, &checkout).unwrap();
            assert_eq!(
                value,
                resolve_project_name(&config, "app", "dev", 1, &checkout).unwrap()
            );
            names.push(value);
        }
        assert_ne!(names[0], names[1]);
        let shared: ManifestContainerConfig =
            toml::from_str("project_name = 'app-dev'\nshare_runtime_identity = true").unwrap();
        assert_eq!(
            resolve_project_name(&shared, "app", "dev", 1, &root.path().join("one")).unwrap(),
            "app-dev"
        );
    }
}
