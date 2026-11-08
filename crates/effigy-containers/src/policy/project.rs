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
    let project_name = scope_project_name_with(
        repo_root,
        project_name,
        config.share_runtime_identity,
        config.checkout_scoped_identity,
    )?;
    Ok(sanitize_project_name_component(&project_name))
}

pub(crate) fn scope_project_name(
    repo_root: &Path,
    project_name: String,
    share_runtime_identity: bool,
) -> Result<String, ContainerPolicyError> {
    scope_project_name_with(repo_root, project_name, share_runtime_identity, false)
}

/// The Compose project identity for one environment.
///
/// A runtime-scoped checkout (linked worktree or marked ephemeral clone) keeps
/// its generation token, so a recreated scope gets a new identity. An
/// environment that declares `checkout_scoped_identity` additionally owns one
/// identity per checkout even when that checkout keeps primary-checkout
/// behaviour: the suffix is the canonical checkout path's digest, so two
/// primary checkouts on one host never share a project, while repeated builds
/// in one checkout keep the same one.
fn scope_project_name_with(
    repo_root: &Path,
    project_name: String,
    share_runtime_identity: bool,
    checkout_scoped_identity: bool,
) -> Result<String, ContainerPolicyError> {
    if share_runtime_identity {
        return Ok(project_name);
    }
    let Some((kind, scope)) = worktree_scope::load_scoped(repo_root).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "cannot establish checkout runtime scope for {}: {error}",
            repo_root.display()
        ))
    })?
    else {
        return Ok(
            match worktree_scope::scope_kind(repo_root)
                .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))?
            {
                Some(kind) => format!("{project_name}-{}-unprepared", kind.project_tag()),
                None if checkout_scoped_identity => {
                    format!("{project_name}-co-{}", checkout_identity_key(repo_root)?)
                }
                None => project_name,
            },
        );
    };
    Ok(format!(
        "{project_name}-{}-{}",
        kind.project_tag(),
        &scope[..12]
    ))
}

/// The stable identity of one owning checkout, for an environment that owns a
/// Compose project per checkout even where the checkout keeps its declared
/// project name. It is the canonical checkout path's digest, never a name.
fn checkout_identity_key(repo_root: &Path) -> Result<String, ContainerPolicyError> {
    let checkout = worktree_scope::checkout_root(repo_root)
        .canonicalize()
        .map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "cannot establish checkout identity for {}: {error}",
                repo_root.display()
            ))
        })?;
    let digest = effigy_gateway::ports::sha256_hex(checkout.as_os_str().as_encoded_bytes());
    Ok(digest[..12].to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_scoped_identity_separates_primary_checkouts_and_reuses_one_per_checkout() {
        let root = tempfile::tempdir().unwrap();
        let config: ManifestContainerConfig =
            toml::from_str("project_name = 'app-dev'\ncheckout_scoped_identity = true").unwrap();
        let mut names = Vec::new();
        for name in ["one", "two"] {
            let checkout = root.path().join(name);
            std::fs::create_dir_all(checkout.join(".git")).unwrap();
            std::fs::write(checkout.join(".git/config"), "[core]\n").unwrap();
            let value = resolve_project_name(&config, "app", "dev", 1, &checkout).unwrap();
            // One exact checkout keeps one identity across ordinary rebuilds.
            assert_eq!(
                value,
                resolve_project_name(&config, "app", "dev", 1, &checkout).unwrap()
            );
            assert!(value.starts_with("app-dev-co-"), "{value}");
            assert_eq!(value.len(), "app-dev-co-".len() + 12);
            names.push(value);
        }
        assert_ne!(names[0], names[1]);

        // The digest is of the canonical checkout path, not of the spelling
        // used to reach it.
        let aliased = root.path().join("one").join("nested");
        std::fs::create_dir_all(&aliased).unwrap();
        assert_eq!(
            resolve_project_name(&config, "app", "dev", 1, &aliased).unwrap(),
            names[0],
            "a nested catalog directory belongs to the enclosing checkout"
        );

        // A runtime-scoped checkout keeps its generation token instead.
        let marked = root.path().join("marked");
        std::fs::create_dir_all(marked.join(".git")).unwrap();
        std::fs::write(
            marked.join(".git/config"),
            "[effigy]\n\truntimeScope = ephemeral\n",
        )
        .unwrap();
        worktree_scope::prepare_checkout_token(&marked).unwrap();
        assert!(resolve_project_name(&config, "app", "dev", 1, &marked)
            .unwrap()
            .starts_with("app-dev-ec-"));

        // Declared sharing wins for an environment that asks for it; the
        // manifest refuses that contradiction before any policy is built.
        let shared: ManifestContainerConfig =
            toml::from_str("project_name = 'app-dev'\nshare_runtime_identity = true").unwrap();
        assert_eq!(
            resolve_project_name(&shared, "app", "dev", 1, &root.path().join("one")).unwrap(),
            "app-dev"
        );
    }

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
            worktree_scope::prepare_checkout_token(&checkout).unwrap();
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

    #[test]
    fn marked_ephemeral_clone_gets_stable_distinct_name_and_unmarked_clone_keeps_base() {
        let root = tempfile::tempdir().unwrap();
        let config: ManifestContainerConfig = toml::from_str("project_name = 'app-dev'").unwrap();

        // Unmarked full clone: primary-checkout behavior, no suffix.
        let plain = root.path().join("plain");
        std::fs::create_dir_all(plain.join(".git")).unwrap();
        std::fs::write(plain.join(".git/config"), "[core]\n").unwrap();
        assert_eq!(
            resolve_project_name(&config, "app", "dev", 1, &plain).unwrap(),
            "app-dev"
        );

        // Marked clone: stable `-ec-` suffixed name, distinct per token.
        let marked = root.path().join("marked");
        std::fs::create_dir_all(marked.join(".git")).unwrap();
        std::fs::write(
            marked.join(".git/config"),
            "[effigy]\n\truntimeScope = ephemeral\n",
        )
        .unwrap();
        worktree_scope::prepare_checkout_token(&marked).unwrap();
        let first = resolve_project_name(&config, "app", "dev", 1, &marked).unwrap();
        assert_eq!(
            first,
            resolve_project_name(&config, "app", "dev", 1, &marked).unwrap()
        );
        assert!(first.starts_with("app-dev-ec-"));
        assert_ne!(first, "app-dev");

        // Recreating the clone at the same path yields a new generation.
        std::fs::remove_dir_all(marked.join(".git")).unwrap();
        std::fs::create_dir_all(marked.join(".git")).unwrap();
        std::fs::write(
            marked.join(".git/config"),
            "[effigy]\n\truntimeScope = ephemeral\n",
        )
        .unwrap();
        assert_ne!(
            first,
            resolve_project_name(&config, "app", "dev", 1, &marked).unwrap()
        );
    }
}
