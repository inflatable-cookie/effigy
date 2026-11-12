//! Resolve the declared workspace and close its pinned manifest and route inputs.

use crate::runner::workspace_client::context::{
    snapshot_path, WorkspaceEnvironmentIdentity, WorkspacePathMap, WorkspacePinnedSource,
    WorkspaceRoute, COMPOSED_MANIFEST_KIND, HOST_REPREPARE_COMMAND,
};
use crate::runner::RunnerError;
use effigy_containers::EffectiveContainerPolicy;
use effigy_manifest::TASK_MANIFEST_FILE;
use std::path::{Path, PathBuf};
/// The declared container-mode workspace (§8.1); hybrid mode has none.
/// Names and user-exported variables never infer a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct OperatorWorkspace {
    pub(in crate::runner) user: String,
    pub(in crate::runner) home: String,
    /// Where the checkout is mounted inside the container.
    pub(in crate::runner) guest_checkout: String,
}

pub(in crate::runner) fn operator_workspace_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<OperatorWorkspace>, RunnerError> {
    // Only a declared operator workspace crosses the mode boundary (§8.1).
    if !environment_declares_operator_workspace(repo_root, policy)? {
        return Ok(None);
    }
    let user = policy
        .workspace_user
        .as_deref()
        .map(str::trim)
        .filter(|user| !user.is_empty());
    let home = policy
        .workspace_home
        .as_deref()
        .map(str::trim)
        .filter(|home| Path::new(home).is_absolute());
    let (Some(user), Some(home)) = (user, home) else {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` declares an operator workspace but has no usable workspace identity; \
             configure a non-empty `user` and absolute `home` in its system/workspace declaration, \
             then run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            policy.name,
        )));
    };
    let guest_checkout =
        effigy_containers::load_container_exec_working_dir(repo_root, Some(&policy.name))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(Some(OperatorWorkspace {
        user: user.to_owned(),
        home: home.to_owned(),
        guest_checkout: guest_checkout.display().to_string(),
    }))
}

/// Ask the manifest resolver whether a system workspace names this container.
/// A catalog default user never promotes a hybrid environment (§8.1).
fn environment_declares_operator_workspace(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    let manifest =
        crate::runner::manifest::load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    effigy_containers::resolve_operator_workspace_for_container(&manifest, Some(&policy.name))
        .map(|workspace| workspace.is_some())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(in crate::runner) fn workspace_path_map(
    repo_root: &Path,
    workspace: &OperatorWorkspace,
) -> Result<WorkspacePathMap, RunnerError> {
    let host_checkout = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    Ok(WorkspacePathMap {
        host_checkout: host_checkout.display().to_string(),
        guest_checkout: workspace.guest_checkout.clone(),
        workdir: workspace.guest_checkout.clone(),
        user: workspace.user.clone(),
        home: workspace.home.clone(),
    })
}

pub(in crate::runner) fn host_user() -> String {
    std::env::var("USER")
        .ok()
        .map(|user| user.trim().to_owned())
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Read the engine's completed activation generation (§8.2, §8.11).
/// Even an activation with unchanged files advances this fence.
pub(super) fn environment_generation(
    policy: &EffectiveContainerPolicy,
) -> Result<WorkspaceEnvironmentIdentity, RunnerError> {
    let record = crate::runner::environment::record(policy).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "environment `{}` has no activation record, so the workspace context has no \
             environment generation to fence; run `{HOST_REPREPARE_COMMAND}` on the host",
            policy.name
        ))
    })?;
    if record.state != effigy_containers::environment::EnvState::Active {
        return Err(RunnerError::task_invocation(format!(
            "environment `{}` has no completed activation; run `{HOST_REPREPARE_COMMAND}` on the host",
            policy.name
        )));
    }
    Ok(WorkspaceEnvironmentIdentity {
        resource_id: format!("env:{}", policy.name),
        generation: record.generation.get().to_string(),
    })
}

/// The complete approved source closure (§8.2): checkout manifests, source lock,
/// bundles, catalogs, fragments and their composed snapshot. The guest reads
/// the immutable snapshot instead of refreshing or trusting host caches.
pub(in crate::runner) struct WorkspaceSourceClosure {
    pub(in crate::runner) pins: Vec<WorkspacePinnedSource>,
    pub(in crate::runner) snapshots: Vec<(String, Vec<u8>)>,
}

pub(in crate::runner) fn workspace_source_closure(
    repo_root: &Path,
    paths: &WorkspacePathMap,
) -> Result<WorkspaceSourceClosure, RunnerError> {
    let host_checkout = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let manifest_path = host_checkout.join(TASK_MANIFEST_FILE);
    let mut pins = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    // Checkout-visible inputs the local task read verifies before parsing.
    for (kind, relative) in [
        ("manifest", TASK_MANIFEST_FILE.to_owned()),
        ("source-lock", ".effigy/sources.lock.json".to_owned()),
    ] {
        let path = host_checkout.join(&relative);
        let Some(bytes) = read_optional_bytes(&path)? else {
            continue;
        };
        seen.insert(relative.clone());
        pins.push(WorkspacePinnedSource {
            kind: kind.to_owned(),
            name: relative.clone(),
            digest: crate::runner::workspace_client::context::sha256_text(&bytes),
            snapshot: Some(guest_checkout_path(paths, &relative)),
        });
    }

    // Publish the complete composed closure without refreshing sources or host state.
    let composed = crate::runner::manifest::load_task_manifest_with_inspection(&manifest_path)?;
    let composed_bytes = composed.effective_manifest.clone().into_bytes();
    let composed_digest = crate::runner::workspace_client::context::sha256_text(&composed_bytes);
    let composed_snapshot = snapshot_path(&composed_digest);
    pins.push(WorkspacePinnedSource {
        kind: COMPOSED_MANIFEST_KIND.to_owned(),
        name: TASK_MANIFEST_FILE.to_owned(),
        digest: composed_digest.clone(),
        snapshot: Some(composed_snapshot.clone()),
    });

    // Each pin names its source bytes and their readable guest path.
    let canonical = |path: PathBuf| path.canonicalize().unwrap_or(path);
    let catalog_roots = effigy_manifest::catalog_source_roots(&manifest_path)
        .map(|roots| {
            let mut declared = roots
                .declared
                .into_iter()
                .map(|(_, path)| canonical(path))
                .collect::<Vec<_>>();
            if let Some(bundle) = roots.bundle {
                declared.push(canonical(bundle));
            }
            declared
        })
        .unwrap_or_default();
    let bundle_root = composed
        .bundle_root
        .as_ref()
        .map(|root| canonical(root.clone()));
    for fragment in &composed.evaluation_order {
        let canonical = fragment
            .canonicalize()
            .unwrap_or_else(|_| fragment.to_path_buf());
        let Ok(relative) = canonical.strip_prefix(&host_checkout) else {
            continue;
        };
        let relative = relative.display().to_string();
        if !seen.insert(relative.clone()) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&canonical) else {
            continue;
        };
        let kind = if bundle_root
            .as_ref()
            .is_some_and(|root| canonical.starts_with(root))
        {
            "bundle"
        } else if catalog_roots.iter().any(|root| canonical.starts_with(root)) {
            "catalog"
        } else {
            "manifest"
        };
        pins.push(WorkspacePinnedSource {
            kind: kind.to_owned(),
            name: relative.clone(),
            digest: crate::runner::workspace_client::context::sha256_text(&bytes),
            snapshot: Some(guest_checkout_path(paths, &relative)),
        });
    }

    // Bundle and catalog identities point to the composed snapshot.
    if let Some(root) = bundle_root.as_ref() {
        if let Some(pin) = source_directory_pin("bundle", root, &host_checkout, &composed_snapshot)
        {
            pins.push(pin);
        }
    }
    for root in &catalog_roots {
        if let Some(pin) = source_directory_pin("catalog", root, &host_checkout, &composed_snapshot)
        {
            pins.push(pin);
        }
    }

    Ok(WorkspaceSourceClosure {
        pins,
        snapshots: vec![(composed_digest, composed_bytes)],
    })
}

/// Pin one source directory's manifest digest and composed snapshot.
fn source_directory_pin(
    kind: &str,
    root: &Path,
    host_checkout: &Path,
    rendered_snapshot: &str,
) -> Option<WorkspacePinnedSource> {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !canonical.is_dir() {
        return None;
    }
    let name = canonical
        .strip_prefix(host_checkout)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| canonical.display().to_string());
    Some(WorkspacePinnedSource {
        kind: kind.to_owned(),
        name,
        digest: manifest_fragment_digest(&canonical)?,
        snapshot: Some(rendered_snapshot.to_owned()),
    })
}

/// Hash sorted relative TOML paths, lengths and bytes for one source directory.
pub(super) fn manifest_fragment_digest(root: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut fragments = Vec::new();
    collect_toml_files(root, root, &mut fragments)?;
    fragments.sort();
    let mut digest = Sha256::new();
    for relative in fragments {
        let path = root.join(&relative);
        let bytes = std::fs::read(&path).ok()?;
        digest.update(relative.display().to_string().as_bytes());
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(&bytes);
    }
    let bytes: [u8; 32] = digest.finalize().into();
    Some(crate::runner::workspace_client::context::render_digest(
        bytes,
    ))
}

fn collect_toml_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Option<()> {
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let file_type = entry.file_type().ok()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_toml_files(root, &path, out)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
            out.push(path.strip_prefix(root).ok()?.to_path_buf());
        }
    }
    Some(())
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, RunnerError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RunnerError::task_invocation(format!(
            "cannot read workspace input `{}`: {error}",
            path.display()
        ))),
    }
}

/// The guest path a checkout-relative file is readable at.
fn guest_checkout_path(paths: &WorkspacePathMap, relative: &str) -> String {
    format!(
        "{checkout}/{relative}",
        checkout = paths.guest_checkout.trim_end_matches('/'),
        relative = relative.trim_start_matches('/')
    )
}

pub(in crate::runner) fn workspace_routes(
    policy: &EffectiveContainerPolicy,
) -> Vec<WorkspaceRoute> {
    policy
        .dns_routes
        .iter()
        .map(|route| WorkspaceRoute {
            declared_domain: route.declared().to_owned(),
            domain: route.domain.clone(),
            service: route.service.clone(),
            port: route.port,
            tls: route.tls,
        })
        .collect()
}
