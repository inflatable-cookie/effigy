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
    let Some(source_compose) = compose_files.first().cloned() else {
        return Ok(Vec::new());
    };
    let (rewritten, decisions) = rewrite_workspace_mounts_for_direct_compose(
        request,
        &source_compose,
        record_optional_extra_mount_decisions,
        draft,
    )?;
    compose_files[0] = rewritten;
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
    enforce_guest_mount_isolation(container_name, &canonical_repo_root, &mounts)?;
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

/// Refuse a mount that would hand the guest a host state root or shadow the
/// guest's own store (§8.1, §8.2, §8.10 R1).
///
/// The masks above cover the checkout's own families. A declared mount that
/// names the guest store, the checkout store or the machine state root is a
/// mistake this refuses by name rather than mounting and hoping.
fn enforce_guest_mount_isolation(
    container_name: &str,
    canonical_repo_root: &Path,
    mounts: &[RenderedWorkspaceMount],
) -> Result<(), ContainerPolicyError> {
    let checkout_state = canonical_repo_root.join(GUEST_MASKED_CHECKOUT_PATHS[0]);
    let checkout_vault = canonical_repo_root.join(GUEST_MASKED_CHECKOUT_PATHS[1]);
    let machine_state = crate::effigy_home().map(|home| home.join("state"));
    for mount in mounts {
        if mount.target == GUEST_PRIVATE_ROOT
            || mount.target.starts_with(&format!("{GUEST_PRIVATE_ROOT}/"))
        {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount target `{target}` is the guest's own store; \
                 a host mount cannot provide `{GUEST_PRIVATE_ROOT}`",
                target = mount.target,
            )));
        }
        let Some(source) = mount.source.as_ref() else {
            continue;
        };
        let host_state = [
            Some(&checkout_state),
            Some(&checkout_vault),
            machine_state.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|root| source == root || source.starts_with(root));
        if host_state {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount source `{source}` is a host state root; \
                 the checkout store, the local vault and the machine state root are host-only \
                 and never mount into a workspace container",
                source = source.display(),
            )));
        }
    }
    Ok(())
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
