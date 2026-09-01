#[cfg(test)]
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_manifest::{ManifestContainerConfig, ManifestContainerServiceConfig};

use crate::{
    layered_catalog_resolver, mount_spec::expand_host_path, policy_support::effigy_home_dir,
    ContainerPolicyError,
};

use super::RenderedWorkspaceMount;

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
pub(crate) struct WorkspaceCatalogCapabilities {
    workspace_host_integration: bool,
    installs_mkcert_ca: bool,
}

pub(crate) fn load_workspace_catalog_capabilities(
    repo_root: &Path,
    config: &ManifestContainerConfig,
    primary_service: &str,
) -> Result<WorkspaceCatalogCapabilities, ContainerPolicyError> {
    let Some(service) = config.services.get(primary_service) else {
        return Ok(WorkspaceCatalogCapabilities::default());
    };
    let fragment = layered_catalog_resolver(Some(repo_root)).resolve(&service.catalog)?;
    Ok(WorkspaceCatalogCapabilities {
        workspace_host_integration: fragment.schema.capabilities.workspace_host_integration,
        installs_mkcert_ca: fragment.schema.capabilities.installs_mkcert_ca,
    })
}

pub(crate) fn build_host_git_config_mount(
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

pub(crate) fn build_host_ssh_known_hosts_mount(
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
pub(crate) fn build_host_ssh_config_mount(
    config: &ManifestContainerConfig,
    primary_service: &str,
    catalog_capabilities: WorkspaceCatalogCapabilities,
) -> Option<RenderedWorkspaceMount> {
    let service = config.services.get(primary_service)?;
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
pub(crate) fn build_host_ssh_dir_mount(
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

pub(crate) fn build_host_ssh_agent_mount(
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
pub(crate) fn build_host_mkcert_ca_mount(
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

pub(crate) fn build_host_composer_home_mount(
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

pub(crate) fn build_shared_composer_home_mount(
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

pub(crate) fn build_shared_composer_cache_mount(
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

/// Build the env vars Effigy needs to inject on the workspace primary
/// service alongside the rewritten volume list. Currently just
/// `SSH_AUTH_SOCK` when host SSH agent forwarding is active.
pub(crate) fn build_workspace_runtime_environment(
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
/// process on macOS - the path only exists VM-side, so a `.exists()`
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
    static TEST_HOST_HOME: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
    static TEST_HOST_SSH_AGENT_SOCKET: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
    static TEST_HOST_MKCERT_ROOT_CA: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
    static TEST_HOST_COMPOSER_HOME: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
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
