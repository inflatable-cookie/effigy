use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    LibraryMount, ManifestContainerConfig, ManifestSystemMount, ManifestSystemMountTable,
    ManifestWorkspaceConfig,
};

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

mod compose_rewrite;
mod host_integration;
mod isolation;
mod library_mounts;
mod ownership;
#[cfg(test)]
use compose_rewrite::inject_workspace_service_environment;
pub(crate) use compose_rewrite::normalize_runtime_rewrite_paths;
use compose_rewrite::rewrite_workspace_mounts_for_direct_compose;
pub(crate) use host_integration::{
    build_host_composer_home_mount, build_host_git_config_mount, build_host_mkcert_ca_mount,
    build_host_ssh_agent_mount, build_host_ssh_config_mount, build_host_ssh_dir_mount,
    build_host_ssh_known_hosts_mount, build_shared_composer_cache_mount,
    build_shared_composer_home_mount, build_workspace_runtime_environment,
    load_workspace_catalog_capabilities,
};
#[cfg(test)]
pub(crate) use host_integration::{
    with_test_host_composer_home, with_test_host_home, with_test_host_mkcert_root_ca,
    with_test_host_ssh_agent_socket, WorkspaceCatalogCapabilities,
};
use isolation::build_isolation_mounts;
use library_mounts::build_library_mounts;

#[derive(Clone, Copy)]
pub(crate) struct WorkspaceComposeRewrite<'a> {
    pub(crate) repo_root: &'a Path,
    pub(crate) container_name: &'a str,
    pub(crate) config: &'a ManifestContainerConfig,
    pub(crate) workspace: &'a ManifestWorkspaceConfig,
    pub(crate) working_dir: &'a Path,
    pub(crate) primary_service: &'a str,
    pub(crate) library_mounts: &'a [LibraryMount],
}

pub(crate) fn materialize_runtime_workspace_mount_rewrite(
    request: WorkspaceComposeRewrite<'_>,
    compose_files: &mut [PathBuf],
    record_optional_extra_mount_decisions: bool,
    draft: &mut crate::policy::render::Draft,
) -> Result<Vec<WorkspaceExtraMountDecision>, ContainerPolicyError> {
    if compose_files.is_empty() {
        return Ok(Vec::new());
    }
    // Every file Compose is handed is rewritten and validated, not only the
    // first: a later overlay replaces entries at the same guest target.
    let (rewritten, decisions) = rewrite_workspace_mounts_for_direct_compose(
        request,
        compose_files,
        record_optional_extra_mount_decisions,
        draft,
    )?;
    if rewritten.len() != compose_files.len() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "workspace mount rewrite produced {} file(s) for {} Compose input(s); \
             every input must be rewritten so an overlay cannot bypass mount isolation",
            rewritten.len(),
            compose_files.len(),
        )));
    }
    compose_files.clone_from_slice(&rewritten);
    Ok(decisions)
}

pub use ownership::{
    load_workspace_ownership_plan, rust_cache_kind, WorkspaceMountKind, WorkspaceOwnershipPlan,
    WorkspaceOwnershipTarget, WorkspaceRepairAuthority, WorkspaceRustCacheKind,
};

pub fn load_workspace_ownership_targets(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, ContainerPolicyError> {
    Ok(load_workspace_ownership_plan(policy)?.owned_disposable_paths())
}

#[derive(Debug, Clone)]
pub(crate) struct RenderedWorkspaceMount {
    target: String,
    rendered: String,
    source: Option<PathBuf>,
    named_volume: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceExtraMountDecisionKind {
    IncludedInComposeSpec,
    SkippedSourceAbsent,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkspaceExtraMountDecision {
    pub source: String,
    pub target: String,
    pub decision: WorkspaceExtraMountDecisionKind,
}

struct ParsedWorkspaceExtraMount {
    mount: Option<RenderedWorkspaceMount>,
    decision: Option<WorkspaceExtraMountDecision>,
}

fn build_workspace_runtime_mounts(
    request: WorkspaceComposeRewrite<'_>,
    record_optional_extra_mount_decisions: bool,
) -> Result<
    (
        Vec<RenderedWorkspaceMount>,
        Vec<WorkspaceExtraMountDecision>,
    ),
    ContainerPolicyError,
> {
    let WorkspaceComposeRewrite {
        repo_root,
        container_name,
        config,
        workspace,
        working_dir,
        primary_service,
        library_mounts,
    } = request;
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
    mounts.extend(build_worktree_git_mounts(&canonical_repo_root));
    let catalog_capabilities =
        load_workspace_catalog_capabilities(repo_root, config, primary_service)?;
    let mut extra_mount_decisions = Vec::new();
    for mount in &workspace.mounts {
        let parsed = parse_workspace_extra_mount(repo_root, container_name, workspace_root, mount)?;
        if let Some(mount) = parsed.mount {
            mounts.push(mount);
        }
        if record_optional_extra_mount_decisions {
            if let Some(decision) = parsed.decision {
                extra_mount_decisions.push(decision);
            }
        }
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
    // Masks go last so they win over every earlier mount that overlaps them,
    // including a workspace `mounts` entry nested under a masked path.
    mounts.extend(build_guest_mask_mounts(
        container_name,
        &canonical_repo_root,
        working_dir,
    ));
    enforce_guest_mount_isolation(container_name, &canonical_repo_root, working_dir, &mounts)?;
    Ok((mounts, extra_mount_decisions))
}

/// Checkout paths a container-mode guest never sees: the checkout runtime
/// store, and the private vault with its previous generation and key wraps
/// (contract 052 §7.3 families 1 and 10-11, §8.2).
///
/// The checkout itself is bind-mounted so ordinary source edits and task
/// outputs stay workspace files. These two families are host records, so each
/// is covered by its own empty read-only mount.
const GUEST_MASKED_CHECKOUT_PATHS: [&str; 2] = [".effigy/state", ".effigy/secrets"];

/// The guest's own store inside the container (§8.2, §8.3). It never comes
/// from a host mount: Prepare publishes into it and a host mount would let a
/// checkout shadow the context, the pinned executable or a capability.
const GUEST_PRIVATE_ROOT: &str = "/run/effigy";

/// Empty read-only masks over the host-only checkout families (§8.2).
///
/// The named volumes are declared in the generated Compose file with fixed
/// names, so a service that is recreated over a retained volume still finds it
/// empty and always mounts it read-only. Nothing writes to them, so they stay
/// empty across reconciles.
fn build_guest_mask_mounts(
    container_name: &str,
    canonical_repo_root: &Path,
    working_dir: &Path,
) -> Vec<RenderedWorkspaceMount> {
    GUEST_MASKED_CHECKOUT_PATHS
        .iter()
        .map(|relative| {
            let target = working_dir.join(relative).display().to_string();
            let volume_name = guest_mask_volume_name(container_name, canonical_repo_root, relative);
            RenderedWorkspaceMount {
                target: target.clone(),
                rendered: format!("{volume_name}:{target}:ro"),
                source: None,
                named_volume: Some(volume_name),
            }
        })
        .collect()
}

fn guest_mask_volume_name(
    container_name: &str,
    canonical_repo_root: &Path,
    relative: &str,
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    container_name.hash(&mut hasher);
    canonical_repo_root.hash(&mut hasher);
    relative.hash(&mut hasher);
    format!("efi-mask-{:016x}", hasher.finish())
}

/// Whether a guest mount target is one of the host state masks. The rewrite
/// keeps these last, so no retained Compose entry can be applied over them.
pub(crate) fn is_guest_mask_target(target: &str, working_dir: &Path) -> bool {
    GUEST_MASKED_CHECKOUT_PATHS
        .iter()
        .any(|relative| target == working_dir.join(relative).display().to_string())
}

/// The host paths a workspace container must never be given, whatever the
/// mount's guest target is (§7.3 families 1, 3 and 10-12).
fn guest_protected_host_paths(canonical_repo_root: &Path) -> Vec<PathBuf> {
    let mut protected = GUEST_MASKED_CHECKOUT_PATHS
        .iter()
        .map(|relative| canonical_repo_root.join(relative))
        .collect::<Vec<_>>();
    if let Some(home) = crate::effigy_home() {
        // The machine store, the gateway's certificates and sockets, and the
        // machine preferences. Shared caches (`~/.effigy/shared`) are derived
        // data a workspace legitimately mounts, so the whole home is not
        // refused here; mounting it elsewhere is refused by the ancestor rule.
        protected.push(home.join("state"));
        protected.push(home.join("gateway"));
        protected.push(home.join("config.toml"));
    }
    protected
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

/// One effective mount, as Compose will apply it.
struct EffectiveMount {
    target: String,
    /// A host path, when this mount binds one.
    source: Option<PathBuf>,
}

/// The values Compose interpolates a service mount with, plus the host paths a
/// bind-backed named volume names.
///
/// Compose expands `${NAME}` in a mount entry before it parses it and resolves
/// a named volume's `driver_opts.device` at mount time. Both can turn an entry
/// that looks harmless into a bind of a host state root, so the isolation check
/// reads exactly those two inputs here.
pub(crate) struct ComposeMountContext {
    env: std::collections::BTreeMap<String, String>,
    named_binds: std::collections::BTreeMap<String, NamedVolumeSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NamedVolumeSource {
    Host(PathBuf),
    /// A named volume whose driver the host cannot resolve to a host path. It
    /// stays refused rather than assumed harmless.
    Unprovable(String),
}

impl ComposeMountContext {
    /// Read the interpolation inputs for one rendered Compose document: the
    /// project `.env` beside the compose file, then the process environment
    /// (Compose's own order), and the document's `volumes:` driver options.
    pub(crate) fn read(compose_dir: &Path, document_volumes: Option<&serde_yaml::Value>) -> Self {
        let mut env = read_compose_env_file(compose_dir);
        for (name, value) in std::env::vars_os() {
            if let (Some(name), Some(value)) = (name.to_str(), value.to_str()) {
                env.insert(name.to_owned(), value.to_owned());
            }
        }
        Self {
            named_binds: read_named_volume_bind_sources(document_volumes, &env, compose_dir),
            env,
        }
    }

    fn expand(&self, value: &str) -> Result<String, String> {
        expand_compose_interpolation(value, &self.env)
    }
}

fn read_compose_env_file(compose_dir: &Path) -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    let Ok(raw) = std::fs::read_to_string(compose_dir.join(".env")) else {
        return env;
    };
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() {
            env.insert(
                name.to_owned(),
                value.trim().trim_matches('"').trim_matches('\'').to_owned(),
            );
        }
    }
    env
}

/// Resolve Compose interpolation: `$NAME`, `${NAME}`, `${NAME:-default}`,
/// `${NAME-default}`, `${NAME:?message}` and `${NAME:+alternate}`.
///
/// An unset name without a default, or a form this reader does not implement,
/// is an error so the caller refuses an entry it cannot prove safe.
fn expand_compose_interpolation(
    value: &str,
    env: &std::collections::BTreeMap<String, String>,
) -> Result<String, String> {
    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(dollar) = rest.find('$') {
        expanded.push_str(&rest[..dollar]);
        let after = &rest[dollar + 1..];
        if let Some(stripped) = after.strip_prefix('$') {
            expanded.push('$');
            rest = stripped;
            continue;
        }
        if let Some(inner) = after.strip_prefix('{') {
            let Some(close) = inner.find('}') else {
                return Err(format!("unterminated interpolation in `{value}`"));
            };
            expanded.push_str(&expand_compose_braced(&inner[..close], env, value)?);
            rest = &inner[close + 1..];
            continue;
        }
        let name_len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if name_len == 0 {
            expanded.push('$');
            rest = after;
            continue;
        }
        let name = &after[..name_len];
        let resolved = env
            .get(name)
            .ok_or_else(|| format!("`${{{name}}}` is unset in `{value}`"))?;
        expanded.push_str(resolved);
        rest = &after[name_len..];
    }
    expanded.push_str(rest);
    Ok(expanded)
}

fn expand_compose_braced(
    inner: &str,
    env: &std::collections::BTreeMap<String, String>,
    value: &str,
) -> Result<String, String> {
    // Compose's forms, longest marker first so `:-` is not read as `-`.
    for marker in [":-", ":?", ":+", "-", "?", "+"] {
        let Some((name, argument)) = inner.split_once(marker) else {
            continue;
        };
        let name = name.trim();
        let set = env.get(name).filter(|value| !value.is_empty());
        return match marker {
            ":-" => Ok(set.cloned().unwrap_or_else(|| argument.to_owned())),
            "-" => Ok(env
                .get(name)
                .cloned()
                .unwrap_or_else(|| argument.to_owned())),
            ":?" => set
                .cloned()
                .ok_or_else(|| format!("`{name}` is unset in `{value}`: {argument}")),
            "?" => env
                .get(name)
                .cloned()
                .ok_or_else(|| format!("`{name}` is unset in `{value}`: {argument}")),
            ":+" => Ok(if set.is_some() {
                argument.to_owned()
            } else {
                String::new()
            }),
            _ => Ok(if env.contains_key(name) {
                argument.to_owned()
            } else {
                String::new()
            }),
        };
    }
    let name = inner.trim();
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!(
            "unsupported interpolation `${{{inner}}}` in `{value}`"
        ));
    }
    env.get(name)
        .cloned()
        .ok_or_else(|| format!("`${{{name}}}` is unset in `{value}`"))
}

/// Split one Compose short-form mount into its source and target. A single
/// component is an anonymous-volume target: Compose mounts an empty volume
/// there, so it carries no host path.
fn split_short_mount_source_and_target(raw: &str) -> Option<(&str, &str)> {
    let mut parts = raw.splitn(3, ':');
    let first = parts.next()?;
    match parts.next() {
        Some(target) if !target.trim().is_empty() => Some((first, target)),
        Some(_) => None,
        None => Some(("", first)),
    }
}

/// The host paths a document's bind-backed named volumes name.
fn read_named_volume_bind_sources(
    document_volumes: Option<&serde_yaml::Value>,
    env: &std::collections::BTreeMap<String, String>,
    compose_dir: &Path,
) -> std::collections::BTreeMap<String, NamedVolumeSource> {
    let mut sources = std::collections::BTreeMap::new();
    let Some(entries) = document_volumes.and_then(serde_yaml::Value::as_mapping) else {
        return sources;
    };
    for (name, entry) in entries {
        let Some(name) = name.as_str() else {
            continue;
        };
        let Some(entry) = entry.as_mapping() else {
            continue;
        };
        let field = |field: &str| {
            entry
                .get(serde_yaml::Value::String(field.to_owned()))
                .and_then(serde_yaml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let driver = field("driver").unwrap_or("local");
        let Some(driver_opts) = entry
            .get(serde_yaml::Value::String("driver_opts".to_owned()))
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        let option = |name: &str| {
            driver_opts
                .get(serde_yaml::Value::String(name.to_owned()))
                .and_then(serde_yaml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let bind = option("o").is_some_and(|value| value.split(',').any(|part| part == "bind"))
            || option("type") == Some("bind")
            || option("type") == Some("none");
        let Some(device) = option("device") else {
            continue;
        };
        if !bind {
            continue;
        }
        let resolved = match expand_compose_interpolation(device, env) {
            Ok(device) if Path::new(&device).is_absolute() => {
                NamedVolumeSource::Host(PathBuf::from(device))
            }
            Ok(device) if driver == "local" && device.starts_with('.') => {
                NamedVolumeSource::Host(compose_dir.join(device))
            }
            Ok(_) => continue,
            Err(reason) => NamedVolumeSource::Unprovable(reason),
        };
        sources.insert(name.to_owned(), resolved);
    }
    sources
}

/// Refuse a mount that would hand the guest a host state root, expose a
/// protected checkout family outside the masked checkout mount, or shadow the
/// guest's own store (§8.1, §8.2, §8.10 R1).
///
/// `checkout_target` is the guest path the checkout is mounted at: the masks
/// cover the protected families inside that one mount, so only that mount may
/// carry them.
fn check_guest_mount(
    container_name: &str,
    protected: &[PathBuf],
    checkout_target: &str,
    mask_targets: &[String],
    mount: &EffectiveMount,
) -> Result<(), ContainerPolicyError> {
    let target = Path::new(&mount.target);
    if path_is_within(target, Path::new(GUEST_PRIVATE_ROOT))
        || path_is_within(Path::new(GUEST_PRIVATE_ROOT), target)
    {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount target `{target}` overlaps the guest's own \
             store; a host mount cannot provide or shadow `{GUEST_PRIVATE_ROOT}`",
            target = mount.target,
        )));
    }
    // The state masks own their guest paths. Only the mask itself (an empty
    // named volume, no host path) may occupy one, so no bind and no retained
    // Compose entry can mount a host path over a masked family.
    if mount.source.is_some() && mask_targets.iter().any(|mask| mask == &mount.target) {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount `{source}` targets the masked checkout path \
             `{target}`; the host state mask owns that path and only an empty read-only mask \
             may occupy it",
            source = mount
                .source
                .as_ref()
                .map(|source| source.display().to_string())
                .unwrap_or_default(),
            target = mount.target,
        )));
    }
    let Some(source) = mount.source.as_ref() else {
        return Ok(());
    };
    let resolved_source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    for root in protected {
        let root = root.canonicalize().unwrap_or_else(|_| root.clone());
        if path_is_within(&resolved_source, &root) {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount source `{source}` is a host state root; \
                 the checkout store, the local vault and the machine state are host-only and \
                 never mount into a workspace container",
                source = source.display(),
            )));
        }
        // An ancestor of a protected family (`<checkout>/.effigy`, the home
        // directory above `~/.effigy`) carries it too. The one mount allowed to
        // carry the checkout's families is the checkout mount itself, where the
        // masks apply.
        if path_is_within(&root, &resolved_source) && mount.target != checkout_target {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount source `{source}` contains a host state \
                 root and mounts it at `{target}`, outside the masked checkout mount \
                 `{checkout_target}`; host state never reaches another guest path",
                source = source.display(),
                target = mount.target,
            )));
        }
    }
    Ok(())
}

fn check_guest_mounts(
    container_name: &str,
    canonical_repo_root: &Path,
    working_dir: &Path,
    mounts: impl IntoIterator<Item = EffectiveMount>,
) -> Result<(), ContainerPolicyError> {
    let protected = guest_protected_host_paths(canonical_repo_root);
    let checkout_target = working_dir.display().to_string();
    let mask_targets = guest_mask_targets(working_dir);
    let mut targets = std::collections::BTreeSet::new();
    for mount in mounts {
        check_guest_mount(
            container_name,
            &protected,
            &checkout_target,
            &mask_targets,
            &mount,
        )?;
        // Two entries for one guest path is ambiguous: which source wins is a
        // backend detail, so an unprovable set is refused instead.
        if !targets.insert(mount.target.clone()) {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mounts two sources at guest path `{target}`; \
                 the effective mount set must name each path once",
                target = mount.target,
            )));
        }
    }
    Ok(())
}

/// The guest paths the host state masks own, in the checkout's terms.
fn guest_mask_targets(working_dir: &Path) -> Vec<String> {
    GUEST_MASKED_CHECKOUT_PATHS
        .iter()
        .map(|relative| working_dir.join(relative).display().to_string())
        .collect()
}

/// Validate the mounts the workspace rewrite injects, before they are written.
fn enforce_guest_mount_isolation(
    container_name: &str,
    canonical_repo_root: &Path,
    working_dir: &Path,
    mounts: &[RenderedWorkspaceMount],
) -> Result<(), ContainerPolicyError> {
    check_guest_mounts(
        container_name,
        canonical_repo_root,
        working_dir,
        mounts.iter().map(|mount| EffectiveMount {
            target: mount.target.clone(),
            source: mount.source.clone(),
        }),
    )
}

/// Validate the complete effective service mount list, including entries the
/// source Compose file already declared, in both the short `source:target`
/// form and the long mapping form (§8.1, §8.2).
/// Validate the complete effective service mount list against the resolved
/// sources Compose will use: interpolation expanded, bind-backed named volumes
/// resolved through the document's `volumes:` driver options, short and long
/// forms alike (§8.1, §8.2).
/// Validate one file's effective mount list with its own document's named
/// volumes. Production rewrites read every Compose input, so the merged view
/// ([`enforce_effective_guest_volume_isolation_against`]) is what they use; this
/// entry point is the single-file proof seam.
#[cfg(test)]
pub(crate) fn enforce_effective_guest_volume_isolation(
    container_name: &str,
    canonical_repo_root: &Path,
    working_dir: &Path,
    compose_dir: &Path,
    document_volumes: Option<&serde_yaml::Value>,
    volumes: &[serde_yaml::Value],
) -> Result<(), ContainerPolicyError> {
    let context = ComposeMountContext::read(compose_dir, document_volumes);
    enforce_effective_guest_volume_isolation_against(
        container_name,
        canonical_repo_root,
        working_dir,
        compose_dir,
        &context,
        volumes,
    )
}

/// Validate one effective mount list against a mount context already read from
/// the complete Compose project, so every file's entries are resolved the way
/// Compose will resolve them.
pub(crate) fn enforce_effective_guest_volume_isolation_against(
    container_name: &str,
    canonical_repo_root: &Path,
    working_dir: &Path,
    compose_dir: &Path,
    context: &ComposeMountContext,
    volumes: &[serde_yaml::Value],
) -> Result<(), ContainerPolicyError> {
    let mut mounts = Vec::with_capacity(volumes.len());
    for entry in volumes {
        mounts.push(effective_volume_mount(
            container_name,
            entry,
            compose_dir,
            context,
        )?);
    }
    check_guest_mounts(container_name, canonical_repo_root, working_dir, mounts)
}

/// Read one Compose `volumes` entry as an effective mount, resolving the source
/// Compose will actually use.
///
/// A named volume, a tmpfs and a docker-managed volume carry no host path. A
/// bind-backed named volume does, through its `driver_opts`, so it is resolved
/// here; an entry whose source cannot be resolved is refused rather than
/// assumed harmless.
fn effective_volume_mount(
    container_name: &str,
    entry: &serde_yaml::Value,
    compose_dir: &Path,
    context: &ComposeMountContext,
) -> Result<EffectiveMount, ContainerPolicyError> {
    let unprovable = |detail: String| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot prove the source of Compose mount `{detail}`; \
             write an absolute host path or remove the interpolation"
        ))
    };
    let named = |name: &str| match context.named_binds.get(name) {
        Some(NamedVolumeSource::Host(path)) => Ok(Some(path.clone())),
        Some(NamedVolumeSource::Unprovable(reason)) => {
            Err(unprovable(format!("{name}` ({reason})")))
        }
        None => Ok(None),
    };
    if let Some(raw) = entry.as_str() {
        // Compose interpolates the whole scalar before parsing it.
        let expanded = context.expand(raw).map_err(unprovable)?;
        let (source, target) = split_short_mount_source_and_target(&expanded)
            .ok_or_else(|| unprovable(format!("`{raw}` names no target")))?;
        let target = target.trim();
        if !target.starts_with('/') {
            return Err(unprovable(format!("`{raw}` names a relative target")));
        }
        let source = source.trim();
        let host_source = if source.is_empty() {
            None
        } else if Path::new(source).is_absolute() {
            Some(PathBuf::from(source))
        } else if source.starts_with('.') || source.contains('/') {
            Some(compose_dir.join(source))
        } else {
            named(source)?
        };
        return Ok(EffectiveMount {
            target: target.to_owned(),
            source: host_source,
        });
    }
    let mapping = entry.as_mapping().ok_or_else(|| {
        unprovable("a Compose mount that is neither a string nor a mapping".to_owned())
    })?;
    let field = |name: &str| {
        mapping
            .get(serde_yaml::Value::String(name.to_owned()))
            .and_then(serde_yaml::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let kind = field("type").unwrap_or("volume");
    let described = format!(
        "a long-form {kind} mount from `{}`",
        field("source").unwrap_or("no source")
    );
    let target = field("target")
        .ok_or_else(|| unprovable(described.clone()))?
        .to_owned();
    let target = context.expand(&target).map_err(unprovable)?;
    let source = field("source");
    let host_source = match kind {
        "tmpfs" => None,
        "volume" => match source {
            Some(source) if Path::new(source).is_absolute() => Some(PathBuf::from(source)),
            Some(source) if source.starts_with('.') || source.contains('/') => {
                Some(compose_dir.join(source))
            }
            Some(source) => named(source)?,
            None => None,
        },
        "bind" => {
            let source = source.ok_or_else(|| unprovable(described.clone()))?;
            let expanded = context.expand(source).map_err(unprovable)?;
            Some(if Path::new(&expanded).is_absolute() {
                PathBuf::from(expanded)
            } else {
                compose_dir.join(expanded)
            })
        }
        _ => None,
    };
    Ok(EffectiveMount {
        target,
        source: host_source,
    })
}

/// Expose the shared git directory behind a linked worktree checkout.
///
/// A linked worktree's `.git` is a file holding an absolute host path into the
/// primary checkout's `.git`. Bind-mounting only the repo root carries that
/// pointer into the container while its target does not exist there, so every
/// in-container `git` call fails with "not a git repository". Mounting the
/// shared git directory at *its own absolute path* keeps the recorded pointer
/// valid without rewriting repository state.
///
/// Returns nothing for an ordinary checkout, a bare repo, or a non-git tree.
fn build_worktree_git_mounts(canonical_repo_root: &Path) -> Vec<RenderedWorkspaceMount> {
    let Some(layout) = effigy_core::git_worktree::detect_linked_worktree(canonical_repo_root)
    else {
        return Vec::new();
    };
    let mut mounts = Vec::new();
    let mut push = |path: &Path| {
        // Already inside the repo-root mount: nothing extra to expose.
        if path.starts_with(canonical_repo_root) {
            return;
        }
        let rendered_path = path.display().to_string();
        if mounts
            .iter()
            .any(|mount: &RenderedWorkspaceMount| mount.target == rendered_path)
        {
            return;
        }
        mounts.push(RenderedWorkspaceMount {
            target: rendered_path.clone(),
            rendered: format!("{rendered_path}:{rendered_path}"),
            source: None,
            named_volume: None,
        });
    };
    push(&layout.common_git_dir);
    // Normally nested under the common dir; mount separately when it is not.
    if !layout.worktree_git_dir.starts_with(&layout.common_git_dir) {
        push(&layout.worktree_git_dir);
    }
    mounts
}

/// Resolve one `[containers.<name>.workspace].mounts` entry.
///
/// Returns `Ok(None)` when a non-catalog sibling source is simply absent on
/// this machine — the same tolerance user-global library mounts already have.
/// A `../book`-style sibling checkout that a teammate has and you do not must
/// not take down `container status` or `doctor` for the whole repo. Catalog
/// members stay a hard error: those are declared repository members, and a
/// missing one is broken state, not machine variance.
fn parse_workspace_extra_mount(
    repo_root: &Path,
    container_name: &str,
    workspace_root: &Path,
    mount: &ManifestSystemMount,
) -> Result<ParsedWorkspaceExtraMount, ContainerPolicyError> {
    match mount {
        ManifestSystemMount::Spec(raw) => {
            parse_legacy_workspace_extra_mount(repo_root, container_name, workspace_root, raw)
        }
        ManifestSystemMount::Table(table) => {
            parse_structured_workspace_extra_mount(repo_root, container_name, workspace_root, table)
        }
    }
}

fn parse_legacy_workspace_extra_mount(
    repo_root: &Path,
    container_name: &str,
    workspace_root: &Path,
    raw: &str,
) -> Result<ParsedWorkspaceExtraMount, ContainerPolicyError> {
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
    let Some(canonical_source) = canonicalize_optional_workspace_source(
        container_name,
        source_raw,
        &resolved_source,
        Path::canonicalize,
    )?
    else {
        return Ok(ParsedWorkspaceExtraMount {
            mount: None,
            decision: Some(WorkspaceExtraMountDecision {
                source: resolved_source.display().to_string(),
                target: target_raw.map(str::to_owned).unwrap_or_else(|| {
                    default_workspace_mount_target(workspace_root, &resolved_source)
                }),
                decision: WorkspaceExtraMountDecisionKind::SkippedSourceAbsent,
            }),
        });
    };
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
    Ok(ParsedWorkspaceExtraMount {
        mount: Some(RenderedWorkspaceMount {
            target: target.clone(),
            rendered,
            source: Some(canonical_source.clone()),
            named_volume: None,
        }),
        decision: Some(WorkspaceExtraMountDecision {
            source: canonical_source.display().to_string(),
            target,
            decision: WorkspaceExtraMountDecisionKind::IncludedInComposeSpec,
        }),
    })
}

fn parse_structured_workspace_extra_mount(
    repo_root: &Path,
    container_name: &str,
    workspace_root: &Path,
    mount: &ManifestSystemMountTable,
) -> Result<ParsedWorkspaceExtraMount, ContainerPolicyError> {
    let source_raw = mount.source.as_deref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace mount member was not resolved"
        ))
    })?;
    let source_path = Path::new(source_raw);
    let resolved_source = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        repo_root.join(source_path)
    };
    let catalog_member = mount.catalog || mount.member.is_some();
    let canonical_source = if catalog_member {
        Some(resolved_source.canonicalize().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` workspace extra mount source `{source_raw}` is invalid: {error}"
            ))
        })?)
    } else {
        canonicalize_optional_workspace_source(
            container_name,
            source_raw,
            &resolved_source,
            Path::canonicalize,
        )?
    };
    let Some(canonical_source) = canonical_source else {
        return Ok(ParsedWorkspaceExtraMount {
            mount: None,
            decision: Some(WorkspaceExtraMountDecision {
                source: resolved_source.display().to_string(),
                target: mount.target.clone().unwrap_or_else(|| {
                    default_workspace_mount_target(workspace_root, &resolved_source)
                }),
                decision: WorkspaceExtraMountDecisionKind::SkippedSourceAbsent,
            }),
        });
    };
    let target = match mount.target.as_deref() {
        Some(target) => target.to_owned(),
        None => {
            let basename = canonical_source
                .file_name()
                .and_then(OsStr::to_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ContainerPolicyError::TaskInvocation(format!(
                        "container `{container_name}` workspace extra mount source `{source_raw}` must declare an explicit target because the source has no basename"
                    ))
                })?;
            workspace_root.join(basename).display().to_string()
        }
    };
    let mut rendered = format!("{}:{target}", canonical_source.display());
    if !mount.options.is_empty() {
        rendered.push(':');
        rendered.push_str(&mount.options.join(","));
    }
    Ok(ParsedWorkspaceExtraMount {
        mount: Some(RenderedWorkspaceMount {
            target: target.clone(),
            rendered,
            source: Some(canonical_source.clone()),
            named_volume: None,
        }),
        decision: (!catalog_member).then(|| WorkspaceExtraMountDecision {
            source: canonical_source.display().to_string(),
            target,
            decision: WorkspaceExtraMountDecisionKind::IncludedInComposeSpec,
        }),
    })
}

fn default_workspace_mount_target(workspace_root: &Path, source: &Path) -> String {
    let basename = source.file_name().unwrap_or_default();
    workspace_root.join(basename).display().to_string()
}

fn canonicalize_optional_workspace_source(
    container_name: &str,
    source_raw: &str,
    resolved_source: &Path,
    canonicalize: impl FnOnce(&Path) -> io::Result<PathBuf>,
) -> Result<Option<PathBuf>, ContainerPolicyError> {
    match std::fs::metadata(resolved_source) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` workspace extra mount source `{source_raw}` is invalid: {error}"
            )))
        }
    }
    canonicalize(resolved_source)
        .map(Some)
        .map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` workspace extra mount source `{source_raw}` is invalid: {error}"
            ))
        })
}

#[cfg(test)]
mod optional_source_tests {
    use super::canonicalize_optional_workspace_source;
    use std::fs;
    use std::path::Path;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let path = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-workspace-optional-source-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
        .expect("fixture directory");
        fs::create_dir_all(&path).expect("mkdir temp dir");
        path
    }

    #[test]
    fn inaccessible_source_parent_is_an_error_not_an_absent_mount() {
        let root = temp_dir("unreadable-parent");
        let blocker = root.join("blocker");
        fs::write(&blocker, "file").expect("write blocker");
        let source = blocker.join("source");

        let error = canonicalize_optional_workspace_source(
            "web",
            "blocker/source",
            &source,
            Path::canonicalize,
        )
        .expect_err("ENOTDIR must fail closed");

        assert!(error
            .to_string()
            .contains("source `blocker/source` is invalid"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_removed_after_presence_check_is_an_error_not_a_skip() {
        let root = temp_dir("raced-removal");
        let source = root.join("source");
        fs::create_dir(&source).expect("create source");

        let error = canonicalize_optional_workspace_source("web", "source", &source, |path| {
            fs::remove_dir(path)?;
            path.canonicalize()
        })
        .expect_err("a source removed after metadata must fail closed");

        assert!(error.to_string().contains("source `source` is invalid"));
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
