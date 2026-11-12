use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup, ManifestInlineWorkspaceContainerConfig,
};

use crate::runtime::dns::{materialize_runtime_dns_override, RuntimeDnsOverrideRoutes};
use crate::{DEFAULT_ATTACH_TIMEOUT_SECS, DEFAULT_COLIMA_PROFILE, DEFAULT_HEALTH_TIMEOUT_SECS};

use super::model::{ContainerPolicyError, EffectiveComposeSource, EffectiveContainerPolicy};

pub fn load_inline_workspace_container_policy(
    repo_root: &Path,
    synthetic_name: &str,
    container: &ManifestInlineWorkspaceContainerConfig,
    workdir: Option<&str>,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let canonical_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.into(),
            error,
        })?;
    let repo_root = canonical_root.as_path();
    let image = container.image.as_deref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` must declare `image`"
        ))
    })?;
    let compose_dir = repo_root
        .join(".effigy")
        .join("inline-workspaces")
        .join(synthetic_name);
    let mut draft = super::render::Draft::default();
    let manifest = repo_root.join("effigy.toml");
    if manifest.is_file() {
        let loaded = effigy_manifest::load_task_manifest_with_inspection(&manifest)?;
        draft.declarations(&loaded.evaluation_order)?;
    }
    let compose_path = compose_dir.join("docker-compose.yml");
    let effective_workdir =
        resolve_inline_workspace_exec_working_dir(repo_root, synthetic_name, container, workdir)?;
    let volume_mount = container
        .mount
        .as_deref()
        .map(|mount| inline_workspace_compose_mount(repo_root, synthetic_name, mount))
        .transpose()?;
    let compose = render_inline_workspace_compose(
        image,
        effective_workdir.as_path(),
        volume_mount.as_deref(),
    );
    draft.insert(compose_path.clone(), compose.into_bytes());
    let repo = repo_root
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("repo")
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    let mut compose_files = vec![compose_path.clone()];
    materialize_runtime_dns_override(
        repo_root,
        synthetic_name,
        DEFAULT_COLIMA_PROFILE,
        &RuntimeDnsOverrideRoutes::default(),
        &mut compose_files,
        &mut draft,
    )?;
    let mut policy = EffectiveContainerPolicy {
        render: None,
        repo_root: repo_root.to_path_buf(),
        name: synthetic_name.to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Attached,
        profile: DEFAULT_COLIMA_PROFILE.to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files,
        managed_volumes: Vec::new(),
        borrowed_volumes: Vec::new(),
        data_peers: Vec::new(),
        data_from: None,
        shared_services: Vec::new(),
        project_name: super::project::scope_project_name(
            repo_root,
            format!("{repo}-{synthetic_name}-inline"),
            false,
        )?,
        primary_service: "workspace".to_owned(),
        dns_domain: None,
        dns_base_domain: None,
        runtime_host_key: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: Vec::new(),
        service_aliases: Vec::new(),
        declared_ports: Vec::new(),
        ports_declared_explicitly: false,
        declared_mounts: container.mount.clone().into_iter().collect(),
        declared_media_mounts: Vec::new(),
        // An inline workspace container is declared by its workspace entry, not
        // by a container environment, so it never declares source staging.
        workspace_source: crate::artifact_source::WorkspaceSource::Checkout,
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: DEFAULT_HEALTH_TIMEOUT_SECS,
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: DEFAULT_ATTACH_TIMEOUT_SECS,
        host_processes: Vec::new(),
    };
    // Render provenance, not a message: this string becomes the closure's
    // `inputs/manifest.toml`, and it must distinguish one inline config from
    // another so two configs never share a render digest. An inline workspace
    // container has no manifest file to name, so the config itself is the
    // input. No command prints it, which is why the Debug ratchet records it
    // by file rather than asking for a `Display`.
    draft.finish(&mut policy, &format!("{container:?}"))?;
    if let Some(render) = &mut policy.render {
        render.request = super::render::Request::Inline {
            container: container.clone(),
            workdir: workdir.map(str::to_owned),
        };
    }
    Ok(policy)
}

pub fn resolve_inline_workspace_exec_working_dir(
    repo_root: &Path,
    synthetic_name: &str,
    container: &ManifestInlineWorkspaceContainerConfig,
    workdir: Option<&str>,
) -> Result<PathBuf, ContainerPolicyError> {
    if let Some(workdir) = workdir.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(workdir));
    }
    let mount = container.mount.as_deref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` must declare `mount` or workspace `working_dir` for exec CWD mapping"
        ))
    })?;
    let (_source, target, _options) = parse_mount_parts(mount).ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount `{mount}` must use `source:target` form"
        ))
    })?;
    if target.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount `{mount}` must declare a non-empty target path"
        )));
    }
    let _ = repo_root;
    Ok(PathBuf::from(target))
}

fn inline_workspace_compose_mount(
    repo_root: &Path,
    synthetic_name: &str,
    mount: &str,
) -> Result<String, ContainerPolicyError> {
    let (source, target, options) = parse_mount_parts(mount).ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount `{mount}` must use `source:target` form"
        ))
    })?;
    let resolved_source = resolve_mount_source(repo_root, synthetic_name, source)?;
    let mut rendered = format!("{}:{target}", resolved_source.display());
    if let Some(options) = options.filter(|value| !value.is_empty()) {
        rendered.push(':');
        rendered.push_str(options);
    }
    Ok(rendered)
}

/// The canonical path one declared mount source names, or a refusal.
///
/// The declaration is only a string: `../shared` and a symlink out of the
/// checkout both name a directory this environment does not own, and a source
/// Compose would create for the first time must be judged before it exists. So
/// the source is resolved against the canonical repo root — lexically first, so
/// a `..` above the root refuses, then through the filesystem, so a link in the
/// middle of the path refuses too. A source that does not exist yet resolves
/// through its nearest existing ancestor and keeps the remaining components;
/// an unreadable ancestor is an error, never an assumption.
fn resolve_mount_source(
    repo_root: &Path,
    synthetic_name: &str,
    source: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.into(),
            error,
        })?;
    let escape = || {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount source `{source}` resolves outside the repo root `{}`, so the environment cannot mount it",
            root.display()
        ))
    };
    let declared = Path::new(source);
    if source.is_empty() || declared.is_absolute() {
        return Err(escape());
    }
    let mut resolved = root.clone();
    for component in declared.components() {
        match component {
            std::path::Component::Normal(part) => resolved.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                // A `..` that would climb above the root is an escape even
                // when the directory it names happens to exist.
                if !resolved.pop() || !resolved.starts_with(&root) {
                    return Err(escape());
                }
            }
            _ => return Err(escape()),
        }
    }
    let mut existing = resolved.as_path();
    let mut remainder = Vec::new();
    loop {
        match existing.canonicalize() {
            Ok(canonical) => {
                let mut path = canonical;
                for part in remainder.iter().rev() {
                    path.push(part);
                }
                return if path.starts_with(&root) {
                    Ok(path)
                } else {
                    Err(escape())
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name() else {
                    return Err(escape());
                };
                remainder.push(name.to_owned());
                let Some(parent) = existing.parent() else {
                    return Err(escape());
                };
                existing = parent;
            }
            // An ancestor that exists but cannot be resolved is not proof that
            // the source stays inside the checkout.
            Err(error) => {
                return Err(ContainerPolicyError::Read {
                    path: existing.into(),
                    error,
                })
            }
        }
    }
}

fn parse_mount_parts(mount: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = mount.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

fn render_inline_workspace_compose(
    image: &str,
    workdir: &Path,
    volume_mount: Option<&str>,
) -> String {
    let workdir = workdir.display().to_string();
    let mut out = String::new();
    out.push_str("services:\n");
    out.push_str("  workspace:\n");
    out.push_str(&format!("    image: \"{}\"\n", image.replace('"', "\\\"")));
    out.push_str(&format!(
        "    working_dir: \"{}\"\n",
        workdir.replace('"', "\\\"")
    ));
    out.push_str("    command:\n");
    out.push_str("      - sh\n");
    out.push_str("      - -lc\n");
    out.push_str("      - while true; do sleep 3600; done\n");
    if let Some(volume_mount) = volume_mount {
        out.push_str("    volumes:\n");
        out.push_str(&format!(
            "      - \"{}\"\n",
            volume_mount.replace('"', "\\\"")
        ));
    }
    out
}
