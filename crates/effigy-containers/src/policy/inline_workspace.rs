use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;
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
    let image = container.image.as_deref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` must declare `image`"
        ))
    })?;
    let compose_dir = repo_root
        .join(".effigy")
        .join("inline-workspaces")
        .join(synthetic_name);
    ensure_effigy_ignored_in_git_root(repo_root).map_err(|error| ContainerPolicyError::Read {
        path: repo_root.join(".gitignore"),
        error,
    })?;
    std::fs::create_dir_all(&compose_dir).map_err(|error| ContainerPolicyError::Read {
        path: compose_dir.clone(),
        error,
    })?;
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
    std::fs::write(&compose_path, compose).map_err(|error| ContainerPolicyError::Read {
        path: compose_path.clone(),
        error,
    })?;
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
    )?;
    Ok(EffectiveContainerPolicy {
        name: synthetic_name.to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Attached,
        profile: DEFAULT_COLIMA_PROFILE.to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files,
        compose_file_display: compose_path
            .strip_prefix(repo_root)
            .unwrap_or(&compose_path)
            .display()
            .to_string(),
        managed_volumes: Vec::new(),
        shared_services: Vec::new(),
        project_name: format!("{repo}-{synthetic_name}-inline"),
        primary_service: "workspace".to_owned(),
        dns_domain: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: Vec::new(),
        service_aliases: Vec::new(),
        declared_ports: Vec::new(),
        ports_declared_explicitly: false,
        declared_mounts: container.mount.clone().into_iter().collect(),
        declared_media_mounts: Vec::new(),
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
    })
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
    let resolved_source = repo_root.join(source);
    let canonical_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let canonical_source = resolved_source
        .canonicalize()
        .unwrap_or_else(|_| resolved_source.clone());
    if !canonical_source.starts_with(&canonical_root) {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount source `{source}` escapes the repo root"
        )));
    }
    let mut rendered = format!("{}:{target}", canonical_source.display());
    if let Some(options) = options.filter(|value| !value.is_empty()) {
        rendered.push(':');
        rendered.push_str(options);
    }
    Ok(rendered)
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
