#[cfg(test)]
use std::cell::RefCell;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_catalog::CatalogResolver;
use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;
use effigy_manifest::{
    load_task_manifest, LibraryMount, ManifestContainerConfig, ManifestContainerServiceConfig,
    ManifestWorkspaceConfig,
};

use crate::{
    mount_spec::expand_host_path, policy_support::effigy_home_dir, ContainerPolicyError,
    EffectiveContainerPolicy, PROJECT_LOCAL_CATALOG_DIR,
};

const PHP_FPM_COMPOSER_HOME_TARGET: &str = "/home/dev/.config/composer";
const PHP_FPM_COMPOSER_CACHE_TARGET: &str = "/home/dev/.cache/composer";
const SHARED_COMPOSER_HOME_VOLUME_SOURCE: &str = "effigy-shared-composer-home";

/// Container target for the host `~/.gitconfig` mount.
const HOST_GIT_CONFIG_TARGET: &str = "/home/dev/.gitconfig";

/// Container target for the host mkcert local root CA. Workspace catalog
/// images run `update-ca-certificates` on entrypoint, which scans
/// `/usr/local/share/ca-certificates/*.crt`, so HTTPS calls from inside
/// the container trust the gateway's mkcert-issued certs.
const HOST_MKCERT_ROOT_CA_TARGET: &str = "/usr/local/share/ca-certificates/effigy-mkcert.crt";

/// Container target for the host `~/.ssh/known_hosts` mount.
const HOST_SSH_KNOWN_HOSTS_TARGET: &str = "/home/dev/.ssh/known_hosts";

/// Container target for the host SSH directory mount when the caller opts
/// into reusing a full host SSH home inside the workspace container.
const HOST_SSH_DIR_TARGET: &str = "/home/dev/.ssh";

/// Container target for the SSH config file mounted into workspace
/// containers. The source may be the host's `~/.ssh/config` when explicitly
/// enabled, or a dedicated per-machine config path via `ssh_config_path`.
const HOST_SSH_CONFIG_TARGET: &str = "/home/dev/.ssh/config";

/// Container target where Colima's forwarded host SSH agent socket is bind
/// mounted. The forwarded socket inside the VM is root-owned, so the
/// non-root workspace user cannot connect to it directly; the catalog image
/// runs a `socat` bridge from this path to
/// [`WORKSPACE_SSH_AUTH_SOCK_BRIDGED`] on container startup.
const HOST_SSH_AGENT_SOCKET_TARGET: &str = "/run/host-services/ssh-auth.sock";

/// Per-developer accessible bridge socket created by the catalog image's
/// `effigy-entrypoint` wrapper. `SSH_AUTH_SOCK` is injected to point here
/// so `ssh-add`, `git push`, and friends speak to a dev-owned socket
/// regardless of how Colima hardens the forwarded original.
const WORKSPACE_SSH_AUTH_SOCK_BRIDGED: &str = "/tmp/effigy-ssh-auth.sock";

#[derive(Debug, Clone, Copy, Default)]
struct WorkspaceCatalogCapabilities {
    workspace_host_integration: bool,
    installs_mkcert_ca: bool,
}

pub(crate) fn materialize_runtime_workspace_mount_rewrite(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    workspace: &ManifestWorkspaceConfig,
    working_dir: &Path,
    primary_service: &str,
    compose_files: &mut [PathBuf],
    library_mounts: &[LibraryMount],
) -> Result<(), ContainerPolicyError> {
    let Some(source_compose) = compose_files.first().cloned() else {
        return Ok(());
    };
    let rewritten = rewrite_workspace_mounts_for_direct_compose(
        repo_root,
        container_name,
        config,
        workspace,
        primary_service,
        &source_compose,
        working_dir,
        library_mounts,
    )?;
    compose_files[0] = rewritten;
    Ok(())
}

pub fn load_workspace_ownership_targets(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut targets = std::collections::BTreeSet::new();
    if let Some(home) = policy
        .workspace_home
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        targets.insert(home.to_owned());
    }
    for compose_file in &policy.compose_files {
        let content =
            std::fs::read_to_string(compose_file).map_err(|error| ContainerPolicyError::Read {
                path: compose_file.clone(),
                error,
            })?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} for workspace ownership targets: {error}",
                compose_file.display()
            ))
        })?;
        let Some(service) = parsed
            .get("services")
            .and_then(|services| services.get(policy.primary_service.as_str()))
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        let Some(volumes) = service
            .get("volumes")
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        for target in volumes
            .iter()
            .filter_map(compose_volume_ownership_target)
            .filter(|value| !value.trim().is_empty())
        {
            targets.insert(target);
        }
    }
    Ok(targets.into_iter().collect())
}

#[derive(Debug, Clone)]
struct RenderedWorkspaceMount {
    target: String,
    rendered: String,
    source: Option<PathBuf>,
    named_volume: Option<String>,
}

fn rewrite_workspace_mounts_for_direct_compose(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    workspace: &ManifestWorkspaceConfig,
    primary_service: &str,
    source_compose: &Path,
    working_dir: &Path,
    library_mounts: &[LibraryMount],
) -> Result<PathBuf, ContainerPolicyError> {
    let workspace_root = working_dir.parent().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace exec working dir `{}` must have a parent directory",
            working_dir.display()
        ))
    })?;
    let compose_dir = source_compose
        .parent()
        .ok_or_else(|| ContainerPolicyError::Read {
            path: source_compose.to_path_buf(),
            error: std::io::Error::other("compose file has no parent directory"),
        })?;
    let content =
        std::fs::read_to_string(source_compose).map_err(|error| ContainerPolicyError::Read {
            path: source_compose.to_path_buf(),
            error,
        })?;
    let mut parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to parse compose file {} for workspace mount rewrite: {error}",
            source_compose.display()
        ))
    })?;
    let injected_mounts = build_workspace_runtime_mounts(
        repo_root,
        container_name,
        config,
        workspace,
        primary_service,
        working_dir,
        library_mounts,
    )?;
    inject_workspace_named_volumes(&mut parsed, &injected_mounts);
    let injected_env = build_workspace_runtime_environment(repo_root, config, primary_service)?;
    {
        let services = parsed
            .get_mut("services")
            .and_then(serde_yaml::Value::as_mapping_mut)
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "compose file {} is missing a `services` mapping for workspace mount rewrite",
                    source_compose.display()
                ))
            })?;
        let service = services
            .get_mut(serde_yaml::Value::String(primary_service.to_owned()))
            .and_then(serde_yaml::Value::as_mapping_mut)
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "compose file {} does not define primary service `{primary_service}` for workspace mount rewrite",
                    source_compose.display()
                ))
            })?;
        rewrite_workspace_service_volumes(
            service,
            compose_dir,
            repo_root,
            workspace_root,
            working_dir,
            &injected_mounts,
        )?;
        if !injected_env.is_empty() {
            inject_workspace_service_environment(service, &injected_env);
        }
    }
    compact_workspace_named_volume_mounts(&mut parsed, primary_service);
    normalize_runtime_rewrite_paths(&mut parsed, compose_dir);

    let rewrite_dir = repo_root.join(".effigy").join("runtime").join("compose");
    ensure_effigy_ignored_in_git_root(repo_root).map_err(|error| ContainerPolicyError::Read {
        path: repo_root.join(".gitignore"),
        error,
    })?;
    std::fs::create_dir_all(&rewrite_dir).map_err(|error| ContainerPolicyError::Read {
        path: rewrite_dir.clone(),
        error,
    })?;
    let rewrite_path = rewrite_dir.join(format!("{container_name}.workspace.compose.yml"));
    let rendered = serde_yaml::to_string(&parsed).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize workspace mount rewrite for `{container_name}`: {error}"
        ))
    })?;
    std::fs::write(&rewrite_path, rendered).map_err(|error| ContainerPolicyError::Read {
        path: rewrite_path.clone(),
        error,
    })?;
    Ok(rewrite_path)
}

fn build_workspace_runtime_mounts(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    workspace: &ManifestWorkspaceConfig,
    primary_service: &str,
    working_dir: &Path,
    library_mounts: &[LibraryMount],
) -> Result<Vec<RenderedWorkspaceMount>, ContainerPolicyError> {
    let workspace_root = working_dir.parent().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace exec working dir `{}` must have a parent directory",
            working_dir.display()
        ))
    })?;
    let canonical_repo_root =
        repo_root
            .canonicalize()
            .map_err(|error| ContainerPolicyError::Read {
                path: repo_root.to_path_buf(),
                error,
            })?;
    let mut mounts = vec![RenderedWorkspaceMount {
        target: working_dir.display().to_string(),
        rendered: format!(
            "{}:{}",
            canonical_repo_root.display(),
            working_dir.display()
        ),
        source: Some(canonical_repo_root.clone()),
        named_volume: None,
    }];
    let catalog_capabilities =
        load_workspace_catalog_capabilities(repo_root, config, primary_service)?;
    for raw in &workspace.mounts {
        mounts.push(parse_workspace_extra_mount(
            repo_root,
            container_name,
            workspace_root,
            raw,
        )?);
    }
    mounts.extend(build_library_mounts(container_name, library_mounts)?);
    mounts.extend(build_isolation_mounts(
        repo_root,
        container_name,
        workspace,
        &mounts,
    )?);
    let host_composer_home_mount = build_host_composer_home_mount(config, primary_service)?;
    if let Some(mount) = host_composer_home_mount {
        mounts.push(mount);
    } else {
        if let Some(mount) = build_shared_composer_home_mount(config, primary_service)? {
            mounts.push(mount);
        }
    }
    if let Some(mount) = build_shared_composer_cache_mount(config, primary_service)? {
        mounts.push(mount);
    }
    if let Some(mount) = build_host_git_config_mount(config, primary_service, catalog_capabilities)
    {
        mounts.push(mount);
    }
    if let Some(mount) = build_host_ssh_dir_mount(config, primary_service, catalog_capabilities) {
        mounts.push(mount);
    } else {
        if let Some(mount) =
            build_host_ssh_known_hosts_mount(config, primary_service, catalog_capabilities)
        {
            mounts.push(mount);
        }
        if let Some(mount) =
            build_host_ssh_config_mount(config, primary_service, catalog_capabilities)
        {
            mounts.push(mount);
        }
    }
    if let Some(mount) = build_host_ssh_agent_mount(config, primary_service, catalog_capabilities) {
        mounts.push(mount);
    }
    if let Some(mount) = build_host_mkcert_ca_mount(config, primary_service, catalog_capabilities) {
        mounts.push(mount);
    }
    Ok(mounts)
}

fn load_workspace_catalog_capabilities(
    repo_root: &Path,
    config: &ManifestContainerConfig,
    primary_service: &str,
) -> Result<WorkspaceCatalogCapabilities, ContainerPolicyError> {
    let Some(service) = config.services.get(primary_service) else {
        return Ok(WorkspaceCatalogCapabilities::default());
    };
    let resolver = CatalogResolver::new(
        project_local_catalog_dir(repo_root),
        user_global_catalog_dir(),
    );
    let fragment = resolver.resolve(&service.catalog)?;
    Ok(WorkspaceCatalogCapabilities {
        workspace_host_integration: fragment.schema.capabilities.workspace_host_integration,
        installs_mkcert_ca: fragment.schema.capabilities.installs_mkcert_ca,
    })
}

fn build_host_git_config_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
    catalog_capabilities: WorkspaceCatalogCapabilities,
) -> Option<RenderedWorkspaceMount> {
    let service = config.services.get(primary_service)?;
    if !catalog_capabilities.workspace_host_integration
        || !service_bool_param(service, "mount_host_git_config", true)
    {
        return None;
    }
    let host_path = host_home_dir()?.join(".gitconfig");
    if !host_path.is_file() {
        return None;
    }
    Some(RenderedWorkspaceMount {
        target: HOST_GIT_CONFIG_TARGET.to_owned(),
        rendered: format!("{}:{HOST_GIT_CONFIG_TARGET}:ro", host_path.display()),
        source: None,
        named_volume: None,
    })
}

fn build_host_ssh_known_hosts_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
    catalog_capabilities: WorkspaceCatalogCapabilities,
) -> Option<RenderedWorkspaceMount> {
    let service = config.services.get(primary_service)?;
    if resolve_host_ssh_dir_path(service)
        .as_deref()
        .is_some_and(Path::is_dir)
    {
        return None;
    }
    if !catalog_capabilities.workspace_host_integration
        || !service_bool_param(service, "mount_host_ssh_known_hosts", true)
    {
        return None;
    }
    let host_path = host_home_dir()?.join(".ssh").join("known_hosts");
    if !host_path.is_file() {
        return None;
    }
    Some(RenderedWorkspaceMount {
        target: HOST_SSH_KNOWN_HOSTS_TARGET.to_owned(),
        rendered: format!("{}:{HOST_SSH_KNOWN_HOSTS_TARGET}:ro", host_path.display()),
        source: None,
        named_volume: None,
    })
}

/// Bind-mount the host's `~/.ssh/config` read-only at
/// [`HOST_SSH_CONFIG_TARGET`] when the caller explicitly opts in.
///
/// This stays off by default. Many host SSH configs depend on local-only
/// `IdentityFile`, `Include`, or `IdentitiesOnly` rules that do not map
/// cleanly into the container. Effigy now prefers the simpler default:
/// forwarded SSH agent plus `known_hosts` plus `gitconfig`, with full SSH
/// config mounting reserved for container-safe explicit setups.
///
/// Returns `None` (silently skipped) when:
/// - the primary service is not a git-aware workspace catalog,
/// - `ssh_config_path` and `mount_host_ssh_config` are both unset,
/// - the configured path cannot be expanded,
/// - or the selected host file does not exist.
fn build_host_ssh_config_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
    catalog_capabilities: WorkspaceCatalogCapabilities,
) -> Option<RenderedWorkspaceMount> {
    let Some(service) = config.services.get(primary_service) else {
        return None;
    };
    if !catalog_capabilities.workspace_host_integration {
        return None;
    }
    if resolve_host_ssh_dir_path(service)
        .as_deref()
        .is_some_and(Path::is_dir)
    {
        return None;
    }

    let host_path = if let Some(raw) = service_string_param(service, "ssh_config_path") {
        let expanded = expand_host_path(raw).ok()?;
        PathBuf::from(expanded)
    } else if service_bool_param(service, "mount_host_ssh_config", false) {
        let home = host_home_dir()?;
        home.join(".ssh").join("config")
    } else {
        return None;
    };

    if !host_path.is_file() {
        return None;
    }
    Some(RenderedWorkspaceMount {
        target: HOST_SSH_CONFIG_TARGET.to_owned(),
        rendered: format!("{}:{HOST_SSH_CONFIG_TARGET}:ro", host_path.display()),
        source: None,
        named_volume: None,
    })
}

/// Bind-mount a full host SSH directory read-only at [`HOST_SSH_DIR_TARGET`]
/// when the caller explicitly opts into legacy or key-file-based SSH
/// behavior inside the container.
///
/// This is a trusted local-dev escape hatch, not the default path. It gives
/// container processes direct read access to whatever private keys, config,
/// and known_hosts entries live under the mounted SSH directory.
fn build_host_ssh_dir_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
    catalog_capabilities: WorkspaceCatalogCapabilities,
) -> Option<RenderedWorkspaceMount> {
    let service = config.services.get(primary_service)?;
    if !catalog_capabilities.workspace_host_integration {
        return None;
    }
    let host_path = resolve_host_ssh_dir_path(service)?;
    if !host_path.is_dir() {
        return None;
    }
    Some(RenderedWorkspaceMount {
        target: HOST_SSH_DIR_TARGET.to_owned(),
        rendered: format!("{}:{HOST_SSH_DIR_TARGET}:ro", host_path.display()),
        source: None,
        named_volume: None,
    })
}

fn build_host_ssh_agent_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
    catalog_capabilities: WorkspaceCatalogCapabilities,
) -> Option<RenderedWorkspaceMount> {
    let service = config.services.get(primary_service)?;
    if !catalog_capabilities.workspace_host_integration
        || !service_bool_param(service, "forward_host_ssh_agent", true)
    {
        return None;
    }
    let host_path = host_ssh_agent_socket()?;
    Some(RenderedWorkspaceMount {
        target: HOST_SSH_AGENT_SOCKET_TARGET.to_owned(),
        rendered: format!("{}:{HOST_SSH_AGENT_SOCKET_TARGET}", host_path.display()),
        source: None,
        named_volume: None,
    })
}

/// Mount the host's mkcert root CA into workspace catalog containers so
/// HTTPS calls from inside the container back through the host gateway
/// trust the gateway's mkcert-issued cert. The catalog image's
/// `effigy-entrypoint` wrapper runs `update-ca-certificates` on
/// container start, which folds the mounted file into the system trust
/// store.
///
/// Silently skipped when:
/// - the primary service is not a workspace-aware catalog,
/// - the per-service `mount_host_mkcert_ca` param is false,
/// - mkcert is not installed on the host or has not generated a root CA
///   (no PEM at `$(mkcert -CAROOT)/rootCA.pem`).
fn build_host_mkcert_ca_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
    catalog_capabilities: WorkspaceCatalogCapabilities,
) -> Option<RenderedWorkspaceMount> {
    let service = config.services.get(primary_service)?;
    if !catalog_capabilities.installs_mkcert_ca
        || !service_bool_param(service, "mount_host_mkcert_ca", true)
    {
        return None;
    }
    let host_path = host_mkcert_root_ca_pem()?;
    Some(RenderedWorkspaceMount {
        target: HOST_MKCERT_ROOT_CA_TARGET.to_owned(),
        rendered: format!("{}:{HOST_MKCERT_ROOT_CA_TARGET}:ro", host_path.display()),
        source: None,
        named_volume: None,
    })
}

/// Resolve the host's mkcert root CA PEM. Tests override via
/// [`with_test_host_mkcert_root_ca`] to avoid invoking the real
/// `mkcert` binary or touching the developer's actual CA root.
fn host_mkcert_root_ca_pem() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(value) = test_host_mkcert_root_ca_override() {
        return value;
    }
    effigy_gateway::tls::mkcert_root_ca_pem()
}

fn project_local_catalog_dir(repo_root: &Path) -> Option<PathBuf> {
    let path = repo_root.join(PROJECT_LOCAL_CATALOG_DIR);
    path.is_dir().then_some(path)
}

fn user_global_catalog_dir() -> Option<PathBuf> {
    let path = effigy_home_dir()?.join("catalog");
    path.is_dir().then_some(path)
}

/// Resolve the host's home directory. Tests can override via
/// [`with_test_host_home`] to avoid touching the real filesystem.
fn host_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(value) = test_host_home_override() {
        return value;
    }
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Locate the host SSH agent socket. Colima exposes it at the canonical
/// `/run/host-services/ssh-auth.sock` path *inside its VM*; that is the
/// path compose resolves the volume mount against, since compose runs in
/// the VM. We deliberately do **not** stat that path from the Effigy
/// process on macOS — the path only exists VM-side, so a `.exists()`
/// check from the host would always return false and silently disable
/// the mount.
///
/// Behaviour:
/// - When `forward_host_ssh_agent = true` (the default) and the catalog
///   is git-aware, we emit the mount unconditionally and trust Colima to
///   honour it. If the socket isn't actually forwarded (e.g. Colima
///   started without `--ssh-agent`), compose-up fails loudly with a
///   clear error rather than ssh later returning "Permission denied
///   (publickey)" with no agent in sight.
/// - Users who don't want this behaviour set `forward_host_ssh_agent =
///   false` per service.
fn host_ssh_agent_socket() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(value) = test_host_ssh_agent_socket_override() {
        return value;
    }
    Some(PathBuf::from(HOST_SSH_AGENT_SOCKET_TARGET))
}

#[cfg(test)]
thread_local! {
    static TEST_HOST_HOME: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
    static TEST_HOST_SSH_AGENT_SOCKET: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
    static TEST_HOST_MKCERT_ROOT_CA: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_test_host_home<T>(path: Option<&Path>, run: impl FnOnce() -> T) -> T {
    struct ResetGuard(Option<Option<PathBuf>>);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_HOST_HOME.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }
    let previous =
        TEST_HOST_HOME.with(|slot| slot.borrow_mut().replace(path.map(Path::to_path_buf)));
    let _guard = ResetGuard(previous);
    run()
}

#[cfg(test)]
fn test_host_home_override() -> Option<Option<PathBuf>> {
    TEST_HOST_HOME.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_test_host_ssh_agent_socket<T>(
    path: Option<&Path>,
    run: impl FnOnce() -> T,
) -> T {
    struct ResetGuard(Option<Option<PathBuf>>);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_HOST_SSH_AGENT_SOCKET.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }
    let previous = TEST_HOST_SSH_AGENT_SOCKET
        .with(|slot| slot.borrow_mut().replace(path.map(Path::to_path_buf)));
    let _guard = ResetGuard(previous);
    run()
}

#[cfg(test)]
fn test_host_ssh_agent_socket_override() -> Option<Option<PathBuf>> {
    TEST_HOST_SSH_AGENT_SOCKET.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
pub(crate) fn with_test_host_mkcert_root_ca<T>(path: Option<&Path>, run: impl FnOnce() -> T) -> T {
    struct ResetGuard(Option<Option<PathBuf>>);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_HOST_MKCERT_ROOT_CA.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }
    let previous = TEST_HOST_MKCERT_ROOT_CA
        .with(|slot| slot.borrow_mut().replace(path.map(Path::to_path_buf)));
    let _guard = ResetGuard(previous);
    run()
}

#[cfg(test)]
fn test_host_mkcert_root_ca_override() -> Option<Option<PathBuf>> {
    TEST_HOST_MKCERT_ROOT_CA.with(|slot| slot.borrow().clone())
}

fn build_host_composer_home_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
) -> Result<Option<RenderedWorkspaceMount>, ContainerPolicyError> {
    let Some(service) = config.services.get(primary_service) else {
        return Ok(None);
    };
    if service.catalog != "php-fpm"
        || !service_bool_param(service, "mount_host_composer_home", false)
    {
        return Ok(None);
    }
    let Some(host_composer_home) = detect_host_composer_home()? else {
        return Ok(None);
    };
    Ok(Some(RenderedWorkspaceMount {
        target: PHP_FPM_COMPOSER_HOME_TARGET.to_owned(),
        rendered: format!(
            "{}:{}",
            host_composer_home.display(),
            PHP_FPM_COMPOSER_HOME_TARGET
        ),
        source: None,
        named_volume: None,
    }))
}

fn build_shared_composer_home_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
) -> Result<Option<RenderedWorkspaceMount>, ContainerPolicyError> {
    let Some(service) = config.services.get(primary_service) else {
        return Ok(None);
    };
    if service.catalog != "php-fpm"
        || !service_bool_param(service, "mount_shared_composer_auth", true)
    {
        return Ok(None);
    }
    Ok(Some(RenderedWorkspaceMount {
        target: PHP_FPM_COMPOSER_HOME_TARGET.to_owned(),
        rendered: format!("{SHARED_COMPOSER_HOME_VOLUME_SOURCE}:{PHP_FPM_COMPOSER_HOME_TARGET}"),
        source: None,
        named_volume: Some(SHARED_COMPOSER_HOME_VOLUME_SOURCE.to_owned()),
    }))
}

fn build_shared_composer_cache_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
) -> Result<Option<RenderedWorkspaceMount>, ContainerPolicyError> {
    let Some(service) = config.services.get(primary_service) else {
        return Ok(None);
    };
    if service.catalog != "php-fpm"
        || !service_bool_param(service, "mount_shared_composer_cache", true)
    {
        return Ok(None);
    }
    let Some(effigy_home) = effigy_home_dir() else {
        return Ok(None);
    };
    let source = effigy_home.join("shared").join("composer-cache");
    std::fs::create_dir_all(&source).map_err(|error| ContainerPolicyError::Read {
        path: source.clone(),
        error,
    })?;
    Ok(Some(RenderedWorkspaceMount {
        target: PHP_FPM_COMPOSER_CACHE_TARGET.to_owned(),
        rendered: format!("{}:{}", source.display(), PHP_FPM_COMPOSER_CACHE_TARGET),
        source: None,
        named_volume: None,
    }))
}

fn build_isolation_mounts(
    repo_root: &Path,
    container_name: &str,
    workspace: &ManifestWorkspaceConfig,
    current_mounts: &[RenderedWorkspaceMount],
) -> Result<Vec<RenderedWorkspaceMount>, ContainerPolicyError> {
    let mut mounts = Vec::new();
    let mut adopted_roots: Vec<(String, PathBuf)> = Vec::new();
    for adoption in &workspace.isolation {
        let producer_root =
            resolve_adopted_isolation_repo(repo_root, container_name, &adoption.repo)?;
        adopted_roots.push((adoption.repo.clone(), producer_root));
    }

    for mount in current_mounts {
        let Some(source) = mount.source.as_ref() else {
            continue;
        };
        if source == repo_root {
            continue;
        }
        let manifest_path = source.join("effigy.toml");
        if !manifest_path.is_file() {
            continue;
        }
        if adopted_roots.iter().any(|(_, root)| root == source) {
            continue;
        }
        adopted_roots.push((source.display().to_string(), source.clone()));
    }

    for (adoption_repo, producer_root) in adopted_roots {
        let producer_manifest_path = producer_root.join("effigy.toml");
        let manifest = load_task_manifest(&producer_manifest_path).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to load isolation contract from `{}` for container `{container_name}`: {error}",
                producer_manifest_path.display()
            ))
        })?;
        let isolation_paths = manifest
            .isolation
            .as_ref()
            .map(|config| config.paths.as_slice())
            .unwrap_or(&[]);
        if isolation_paths.is_empty() {
            continue;
        }
        let target_root = current_mounts
            .iter()
            .find_map(|mount| {
                mount.source
                    .as_ref()
                    .filter(|source| **source == producer_root)
                    .map(|_| mount.target.as_str())
            })
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "system isolation repo `{}` for container `{container_name}` is not mounted into the workspace runtime",
                    adoption_repo
                ))
            })?;

        for relative in isolation_paths {
            let normalized = normalize_isolation_relative_path(
                &producer_manifest_path,
                relative,
                container_name,
            )?;
            let volume_name = isolation_volume_name(container_name, &producer_root, &normalized);
            let target = Path::new(target_root)
                .join(&normalized)
                .display()
                .to_string();
            mounts.push(RenderedWorkspaceMount {
                target: target.clone(),
                rendered: format!("{volume_name}:{target}"),
                source: None,
                named_volume: Some(volume_name),
            });
        }
    }
    Ok(mounts)
}

fn resolve_adopted_isolation_repo(
    repo_root: &Path,
    container_name: &str,
    raw_repo: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let repo = raw_repo.trim();
    if repo.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` system isolation repo entry must not be empty"
        )));
    }
    let resolved = if Path::new(repo).is_absolute() {
        PathBuf::from(repo)
    } else {
        repo_root.join(repo)
    };
    resolved.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` system isolation repo `{repo}` is invalid: {error}"
        ))
    })
}

fn normalize_isolation_relative_path(
    manifest_path: &Path,
    raw_path: &str,
    container_name: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "isolation path in {} for container `{container_name}` must not be empty",
            manifest_path.display()
        )));
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "isolation path `{trimmed}` in {} for container `{container_name}` must be relative",
            manifest_path.display()
        )));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(value) => normalized.push(value),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(ContainerPolicyError::TaskInvocation(format!(
                    "isolation path `{trimmed}` in {} for container `{container_name}` must stay under the producer repo root",
                    manifest_path.display()
                )))
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "isolation path `{trimmed}` in {} for container `{container_name}` resolved to an empty path",
            manifest_path.display()
        )));
    }
    Ok(normalized)
}

fn isolation_volume_name(
    container_name: &str,
    producer_root: &Path,
    relative_path: &Path,
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    container_name.hash(&mut hasher);
    producer_root.hash(&mut hasher);
    relative_path.hash(&mut hasher);
    format!("efi-iso-{:016x}", hasher.finish())
}

fn service_bool_param(service: &ManifestContainerServiceConfig, key: &str, default: bool) -> bool {
    service
        .params
        .get(key)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

fn resolve_host_ssh_dir_path(service: &ManifestContainerServiceConfig) -> Option<PathBuf> {
    if let Some(raw) = service_string_param(service, "ssh_dir_path") {
        let expanded = expand_host_path(raw).ok()?;
        return Some(PathBuf::from(expanded));
    }
    if service_bool_param(service, "mount_host_ssh_dir", false) {
        let home = host_home_dir()?;
        return Some(home.join(".ssh"));
    }
    None
}

fn service_string_param<'a>(
    service: &'a ManifestContainerServiceConfig,
    key: &str,
) -> Option<&'a str> {
    service
        .params
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn detect_host_composer_home() -> Result<Option<PathBuf>, ContainerPolicyError> {
    #[cfg(test)]
    if let Some(value) = test_host_composer_home_override() {
        return Ok(value);
    }

    let output = match ProcessCommand::new("composer")
        .args(["global", "config", "home", "--absolute"])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "failed to probe host composer home: {error}"
            )))
        }
    };

    if !output.status.success() {
        return Ok(None);
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if raw.is_empty() {
        return Ok(None);
    }

    let path = PathBuf::from(raw);
    if !path.exists() {
        return Ok(None);
    }

    path.canonicalize().map(Some).or(Ok(Some(path)))
}

#[cfg(test)]
thread_local! {
    static TEST_HOST_COMPOSER_HOME: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_test_host_composer_home<T>(path: Option<&Path>, run: impl FnOnce() -> T) -> T {
    struct ResetGuard(Option<Option<PathBuf>>);

    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_HOST_COMPOSER_HOME.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }

    let previous =
        TEST_HOST_COMPOSER_HOME.with(|slot| slot.borrow_mut().replace(path.map(Path::to_path_buf)));
    let _guard = ResetGuard(previous);
    run()
}

#[cfg(test)]
fn test_host_composer_home_override() -> Option<Option<PathBuf>> {
    TEST_HOST_COMPOSER_HOME.with(|slot| slot.borrow().clone())
}

/// Container path under which user-global library mounts are exposed.
///
/// Stable convention so the legacy `effigy mount` command (which lives inside
/// the decodelabs bundle container) has a predictable place to point at when
/// resolving Composer `path` repositories — e.g. `~/Dev/legacy/libraries/decodelabs`
/// becomes available at `/workspace-libraries/decodelabs/<package>` inside
/// the workspace container.
const WORKSPACE_LIBRARIES_ROOT: &str = "/workspace-libraries";

/// Render user-global library mounts as bind-mount entries on the workspace
/// container. Missing host paths are skipped silently — a developer's
/// `~/.effigy/config.toml` may declare a parent directory that is only
/// present on some machines, and a hard error there would block every
/// `effigy container up` until the file is hand-edited. Hard errors are
/// reserved for genuinely broken state (basename collisions between two
/// declared parents).
fn build_library_mounts(
    container_name: &str,
    library_mounts: &[LibraryMount],
) -> Result<Vec<RenderedWorkspaceMount>, ContainerPolicyError> {
    let mut rendered: Vec<RenderedWorkspaceMount> = Vec::new();
    let mut targets_seen: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for entry in library_mounts {
        let host_path = &entry.host_path;
        if !host_path.exists() {
            // Best-effort: skip silently. A warning channel here would be
            // welcome but doesn't have a natural sink at this layer.
            continue;
        }
        let canonical_source = host_path.canonicalize().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` library mount source `{}` is invalid: {error}",
                entry.raw
            ))
        })?;
        let basename = canonical_source
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "container `{container_name}` library mount `{}` resolves to a path with no basename",
                    entry.raw
                ))
            })?;
        let target = format!("{WORKSPACE_LIBRARIES_ROOT}/{basename}");
        if let Some(previous) = targets_seen.get(&target) {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` library mounts `{previous}` and `{}` both resolve to container path `{target}`; rename one or pick a non-colliding parent directory",
                entry.raw
            )));
        }
        targets_seen.insert(target.clone(), entry.raw.clone());
        let rendered_entry = format!("{}:{target}", canonical_source.display());
        rendered.push(RenderedWorkspaceMount {
            target,
            rendered: rendered_entry,
            source: Some(canonical_source),
            named_volume: None,
        });
    }
    Ok(rendered)
}

fn parse_workspace_extra_mount(
    repo_root: &Path,
    container_name: &str,
    workspace_root: &Path,
    raw: &str,
) -> Result<RenderedWorkspaceMount, ContainerPolicyError> {
    let mut parts = raw.splitn(3, ':');
    let source_raw = parts.next().unwrap_or_default().trim();
    let target_raw = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let options = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source_raw.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace extra mount `{raw}` is invalid: source path is empty"
        )));
    }
    let source_path = Path::new(source_raw);
    let resolved_source = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        repo_root.join(source_path)
    };
    let canonical_source = resolved_source.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace extra mount source `{source_raw}` is invalid: {error}"
        ))
    })?;
    let target = if let Some(target) = target_raw {
        target.to_owned()
    } else {
        let basename = canonical_source
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "container `{container_name}` workspace extra mount `{raw}` must declare an explicit target because the source has no basename"
                ))
            })?;
        workspace_root.join(basename).display().to_string()
    };
    let mut rendered = format!("{}:{target}", canonical_source.display());
    if let Some(options) = options {
        rendered.push(':');
        rendered.push_str(options);
    }
    Ok(RenderedWorkspaceMount {
        target,
        rendered,
        source: Some(canonical_source),
        named_volume: None,
    })
}

fn rewrite_workspace_service_volumes(
    service: &mut serde_yaml::Mapping,
    compose_dir: &Path,
    repo_root: &Path,
    workspace_root: &Path,
    working_dir: &Path,
    injected_mounts: &[RenderedWorkspaceMount],
) -> Result<(), ContainerPolicyError> {
    let key = serde_yaml::Value::String("volumes".to_owned());
    let volumes = match service.get_mut(&key) {
        Some(serde_yaml::Value::Sequence(sequence)) => sequence,
        Some(_) => {
            return Err(ContainerPolicyError::TaskInvocation(
                "workspace mount rewrite only supports sequence `volumes` entries".to_owned(),
            ))
        }
        None => {
            service.insert(key.clone(), serde_yaml::Value::Sequence(Vec::new()));
            service
                .get_mut(&key)
                .and_then(serde_yaml::Value::as_sequence_mut)
                .expect("volumes sequence should exist after insertion")
        }
    };

    let canonical_repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let workspace_root_rendered = workspace_root.display().to_string();
    let working_dir_rendered = working_dir.display().to_string();
    let injected_targets = injected_mounts
        .iter()
        .map(|mount| mount.target.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut rewritten = injected_mounts
        .iter()
        .map(|mount| serde_yaml::Value::String(mount.rendered.clone()))
        .collect::<Vec<_>>();

    for entry in volumes.iter() {
        let Some(raw) = entry.as_str() else {
            rewritten.push(entry.clone());
            continue;
        };
        let Some((source, target, _options)) = parse_mount_parts(raw) else {
            rewritten.push(entry.clone());
            continue;
        };
        if injected_targets.contains(target) {
            continue;
        }
        if target == workspace_root_rendered {
            if let Some(canonical_source) = resolve_bind_mount_source(compose_dir, source) {
                if canonical_repo_root.starts_with(&canonical_source) {
                    continue;
                }
            }
        }
        if target == working_dir_rendered {
            if let Some(canonical_source) = resolve_bind_mount_source(compose_dir, source) {
                if canonical_source == canonical_repo_root {
                    continue;
                }
            }
        }
        rewritten.push(entry.clone());
    }

    *volumes = rewritten;
    Ok(())
}

/// Build the env vars Effigy needs to inject on the workspace primary
/// service alongside the rewritten volume list. Currently just
/// `SSH_AUTH_SOCK` when host SSH agent forwarding is active.
fn build_workspace_runtime_environment(
    repo_root: &Path,
    config: &ManifestContainerConfig,
    primary_service: &str,
) -> Result<std::collections::BTreeMap<String, String>, ContainerPolicyError> {
    let mut env = std::collections::BTreeMap::new();
    let catalog_capabilities =
        load_workspace_catalog_capabilities(repo_root, config, primary_service)?;
    if build_host_ssh_agent_mount(config, primary_service, catalog_capabilities).is_some() {
        // Point at the catalog image's socat-bridged socket rather than
        // the raw bind-mounted path: the forwarded original is root-owned
        // inside the Colima VM and the bridge is what the workspace user
        // can actually connect to.
        env.insert(
            "SSH_AUTH_SOCK".to_owned(),
            WORKSPACE_SSH_AUTH_SOCK_BRIDGED.to_owned(),
        );
    }
    Ok(env)
}

/// Merge `additions` into the primary service's `environment:` block,
/// supporting both compose forms (mapping or list of `KEY=VALUE` strings).
/// Existing keys are preserved — user-declared values win over Effigy's
/// runtime defaults.
fn inject_workspace_service_environment(
    service: &mut serde_yaml::Mapping,
    additions: &std::collections::BTreeMap<String, String>,
) {
    if additions.is_empty() {
        return;
    }
    let key = serde_yaml::Value::String("environment".to_owned());
    let entry = service
        .entry(key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    match entry {
        serde_yaml::Value::Mapping(mapping) => {
            for (name, value) in additions {
                let env_key = serde_yaml::Value::String(name.clone());
                if !mapping.contains_key(&env_key) {
                    mapping.insert(env_key, serde_yaml::Value::String(value.clone()));
                }
            }
        }
        serde_yaml::Value::Sequence(sequence) => {
            let existing: std::collections::BTreeSet<String> = sequence
                .iter()
                .filter_map(|item| item.as_str())
                .map(|raw| {
                    raw.split_once('=')
                        .map(|(k, _)| k.to_owned())
                        .unwrap_or_else(|| raw.to_owned())
                })
                .collect();
            for (name, value) in additions {
                if !existing.contains(name) {
                    sequence.push(serde_yaml::Value::String(format!("{name}={value}")));
                }
            }
        }
        _ => {
            // `environment` is set to something other than a list/mapping
            // (e.g. null). Replace with a fresh mapping carrying our entries.
            let mut mapping = serde_yaml::Mapping::new();
            for (name, value) in additions {
                mapping.insert(
                    serde_yaml::Value::String(name.clone()),
                    serde_yaml::Value::String(value.clone()),
                );
            }
            *entry = serde_yaml::Value::Mapping(mapping);
        }
    }
}

fn inject_workspace_named_volumes(
    parsed: &mut serde_yaml::Value,
    mounts: &[RenderedWorkspaceMount],
) {
    let names = mounts
        .iter()
        .filter_map(|mount| mount.named_volume.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    if names.is_empty() {
        return;
    }
    let Some(root) = parsed.as_mapping_mut() else {
        return;
    };
    let key = serde_yaml::Value::String("volumes".to_owned());
    let volumes = root
        .entry(key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let Some(mapping) = volumes.as_mapping_mut() else {
        return;
    };
    for name in names {
        let entry_key = serde_yaml::Value::String(name.to_owned());
        mapping.entry(entry_key).or_insert_with(|| {
            let mut entry = serde_yaml::Mapping::new();
            entry.insert(
                serde_yaml::Value::String("name".to_owned()),
                serde_yaml::Value::String(name.to_owned()),
            );
            serde_yaml::Value::Mapping(entry)
        });
    }
}

fn compact_workspace_named_volume_mounts(parsed: &mut serde_yaml::Value, primary_service: &str) {
    let Some(root) = parsed.as_mapping_mut() else {
        return;
    };
    let service_key = serde_yaml::Value::String(primary_service.to_owned());
    let mut renamed: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    {
        let Some(service) = root
            .get_mut(serde_yaml::Value::String("services".to_owned()))
            .and_then(serde_yaml::Value::as_mapping_mut)
            .and_then(|services| services.get_mut(&service_key))
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            return;
        };
        let Some(volumes) = service
            .get_mut(serde_yaml::Value::String("volumes".to_owned()))
            .and_then(serde_yaml::Value::as_sequence_mut)
        else {
            return;
        };
        for entry in volumes.iter_mut() {
            let Some(raw) = entry.as_str() else {
                continue;
            };
            let Some((source, target, options)) = parse_mount_parts(raw) else {
                continue;
            };
            if looks_like_bind_mount_source(source) {
                continue;
            }
            let short = renamed
                .entry(source.to_owned())
                .or_insert_with(|| compact_named_volume_name(source))
                .clone();
            let rendered = match options.filter(|value| !value.is_empty()) {
                Some(options) => format!("{short}:{target}:{options}"),
                None => format!("{short}:{target}"),
            };
            *entry = serde_yaml::Value::String(rendered);
        }
    }
    if renamed.is_empty() {
        return;
    }
    let volumes_key = serde_yaml::Value::String("volumes".to_owned());
    let Some(volumes_root) = root
        .entry(volumes_key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
        .as_mapping_mut()
    else {
        return;
    };
    for (original, short) in renamed {
        let original_key = serde_yaml::Value::String(original);
        let short_key = serde_yaml::Value::String(short.clone());
        let mut entry = volumes_root
            .get(&original_key)
            .and_then(serde_yaml::Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        entry.insert(
            serde_yaml::Value::String("name".to_owned()),
            serde_yaml::Value::String(short.clone()),
        );
        volumes_root.insert(short_key, serde_yaml::Value::Mapping(entry));
    }
}

fn compact_named_volume_name(source: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("efv-{:016x}", hasher.finish())
}

fn parse_mount_parts(mount: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = mount.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

fn compose_volume_ownership_target(entry: &serde_yaml::Value) -> Option<String> {
    let raw = entry.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((source, target, _options)) = parse_mount_parts(raw) {
        if looks_like_bind_mount_source(source) {
            return None;
        }
        return Some(target.to_owned());
    }
    raw.starts_with('/').then(|| raw.to_owned())
}

fn resolve_bind_mount_source(compose_dir: &Path, source: &str) -> Option<PathBuf> {
    if !looks_like_bind_mount_source(source) {
        return None;
    }
    let source_path = Path::new(source);
    let resolved = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        compose_dir.join(source_path)
    };
    Some(resolved.canonicalize().unwrap_or(resolved))
}

fn looks_like_bind_mount_source(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
        || source == "."
        || source == ".."
        || source.contains('/')
}

fn normalize_runtime_rewrite_paths(parsed: &mut serde_yaml::Value, compose_dir: &Path) {
    let Some(services) = parsed
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return;
    };
    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        normalize_service_build_paths(service, compose_dir);
        normalize_service_env_file_paths(service, compose_dir);
        normalize_service_volume_sources(service, compose_dir);
    }
}

fn normalize_service_build_paths(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(build) = service.get_mut(serde_yaml::Value::String("build".to_owned())) else {
        return;
    };
    match build {
        serde_yaml::Value::String(path) => {
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *path = normalized;
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            let key = serde_yaml::Value::String("context".to_owned());
            if let Some(serde_yaml::Value::String(path)) = mapping.get_mut(&key) {
                if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                    *path = normalized;
                }
            }
        }
        _ => {}
    }
}

fn normalize_service_env_file_paths(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(env_file) = service.get_mut(serde_yaml::Value::String("env_file".to_owned())) else {
        return;
    };
    match env_file {
        serde_yaml::Value::String(path) => {
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *path = normalized;
            }
        }
        serde_yaml::Value::Sequence(entries) => {
            for entry in entries {
                if let serde_yaml::Value::String(path) = entry {
                    if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                        *path = normalized;
                    }
                }
            }
        }
        _ => {}
    }
}

fn normalize_service_volume_sources(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(serde_yaml::Value::Sequence(volumes)) =
        service.get_mut(serde_yaml::Value::String("volumes".to_owned()))
    else {
        return;
    };
    for volume in volumes {
        let Some(raw) = volume.as_str() else {
            continue;
        };
        let Some((source, target, options)) = parse_mount_parts(raw) else {
            continue;
        };
        let Some(normalized_source) = normalize_bind_mount_source(source, compose_dir) else {
            continue;
        };
        let mut rendered = format!("{normalized_source}:{target}");
        if let Some(options) = options.filter(|value| !value.is_empty()) {
            rendered.push(':');
            rendered.push_str(options);
        }
        *volume = serde_yaml::Value::String(rendered);
    }
}

fn normalize_bind_mount_source(source: &str, compose_dir: &Path) -> Option<String> {
    looks_like_bind_mount_source(source).then(|| render_compose_relative_path(source, compose_dir))
}

fn normalize_compose_relative_path(path: &str, compose_dir: &Path) -> Option<String> {
    (!path.is_empty() && !Path::new(path).is_absolute() && !looks_like_remote_build_context(path))
        .then(|| render_compose_relative_path(path, compose_dir))
}

fn render_compose_relative_path(path: &str, compose_dir: &Path) -> String {
    let resolved = compose_dir.join(path);
    resolved
        .canonicalize()
        .unwrap_or(resolved)
        .display()
        .to_string()
}

fn looks_like_remote_build_context(path: &str) -> bool {
    path.contains("://") || path.starts_with("git@")
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
