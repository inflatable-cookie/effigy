use std::collections::BTreeSet;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    export_volume_command, import_volume_command, inspect_volume_command, list_all_volumes_command,
    list_volumes_command, merge_runtime_volume_metadata, parse_inspect_volume_metadata,
    parse_listed_volume_names, parse_volume_usage_bytes, remove_volume_command,
    volume_usage_command, DockerCommand, ManagedVolume,
};
use effigy_containers::{
    cache_list_all_report, cache_list_report, cache_prune_report, data_list_report,
    data_transfer_report, exec::colima_is_running, load_container_policy,
    validate_compose_backend_runtime, validate_container_policy, ContainerCacheGlobalEntry,
    ContainerCachePruneEntry, ContainerCacheVolumeEntry, ContainerCommandReport,
    ContainerDataTransferAction, ContainerDataVolumeEntry, EffectiveComposeSource,
    EffectiveContainerPolicy,
};

use crate::read::discover_running_environments;
use crate::EffigyRuntimeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredGatewayRoute {
    pub domain: String,
    pub target: Option<String>,
    pub dns_ip: Option<std::net::Ipv4Addr>,
    pub tls: bool,
}

pub fn run_container_data_list<F>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(&policy, "list")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

    let colima_running = colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let volumes = hydrate_managed_volumes(
        repo_root,
        &policy,
        colima_running,
        &run_runtime_volume_capture,
    )?
    .into_iter()
    .map(|volume| ContainerDataVolumeEntry {
        name: volume.name,
        service: volume.service,
        persist: volume.persist,
        size_bytes: volume.size_bytes,
        mount_point: volume.mount_point,
    })
    .collect::<Vec<_>>();

    Ok(render_container_report(
        data_list_report(&policy, colima_running, &volumes),
        output_json,
    ))
}

pub fn run_container_cache_list<F>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(&policy, "cache list")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

    let colima_running = colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let caches = hydrate_managed_volumes(
        repo_root,
        &policy,
        colima_running,
        &run_runtime_volume_capture,
    )?
    .into_iter()
    .filter_map(cache_volume_entry)
    .collect::<Vec<_>>();

    Ok(render_container_report(
        cache_list_report(&policy, colima_running, &caches),
        output_json,
    ))
}

pub fn run_container_cache_list_under_path<F>(
    scope_root: &Path,
    name: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError> + Copy,
{
    let mut rendered = Vec::new();
    for repo_root in crate::read::discover_effigy_repos_under(scope_root) {
        match run_container_cache_list(&repo_root, name, output_json, run_runtime_volume_capture) {
            Ok(output) => rendered.push(output),
            Err(error) => {
                rendered.push(format!("[warn] skipped `{}`: {error}", repo_root.display()))
            }
        }
    }
    if rendered.is_empty() {
        return Ok(format!(
            "[info] no Effigy repos found under {}",
            scope_root.display()
        ));
    }
    Ok(rendered.join("\n"))
}

pub fn run_container_cache_list_all<F>(
    cwd: &Path,
    profile: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let profile = profile.unwrap_or("effigy");
    let caches = collect_global_cache_entries(cwd, profile, &run_runtime_volume_capture)?;

    Ok(render_container_report(
        cache_list_all_report(profile, &caches),
        output_json,
    ))
}

pub fn run_container_cache_prune<F>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(&policy, "cache prune")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

    ensure_cache_prune_target_is_stopped(
        &policy.name,
        project_is_running(repo_root, &policy.project_name)?,
    )?;

    let volumes = hydrate_managed_volumes(repo_root, &policy, false, &run_runtime_volume_capture)?;
    let mut entries = Vec::new();
    for volume in volumes {
        let Some(kind) = volume.cache_kind() else {
            continue;
        };
        run_runtime_volume_capture(
            repo_root,
            &policy.profile,
            &remove_volume_command(&volume.name),
        )?;
        entries.push(ContainerCachePruneEntry {
            name: volume.name,
            kind: cache_kind_label(kind).to_owned(),
            size_bytes: volume.size_bytes,
            project_name: Some(policy.project_name.clone()),
            removed: true,
            in_use: false,
        });
    }

    Ok(render_container_report(
        cache_prune_report(&format!("container `{}`", policy.name), &entries),
        output_json,
    ))
}

pub fn run_container_cache_prune_all<F>(
    cwd: &Path,
    profile: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let profile = profile.unwrap_or("effigy");
    let caches = collect_global_cache_entries(cwd, profile, &run_runtime_volume_capture)?;
    let mut entries = Vec::new();
    for cache in caches {
        let removed = if cache.in_use {
            false
        } else {
            run_runtime_volume_capture(cwd, profile, &remove_volume_command(&cache.name))?;
            true
        };
        entries.push(ContainerCachePruneEntry {
            name: cache.name,
            kind: cache.kind,
            size_bytes: cache.size_bytes,
            project_name: cache.project_name,
            removed,
            in_use: cache.in_use,
        });
    }

    Ok(render_container_report(
        cache_prune_report("profile-wide cache inventory", &entries),
        output_json,
    ))
}

pub fn run_container_data_export<F>(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    run_container_data_transfer(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        ContainerDataTransferAction::Export,
        run_runtime_volume_capture,
    )
}

pub fn run_container_data_import<F>(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    run_container_data_transfer(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        ContainerDataTransferAction::Import,
        run_runtime_volume_capture,
    )
}

fn run_container_data_transfer<F>(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    action: ContainerDataTransferAction,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(
        &policy,
        match action {
            ContainerDataTransferAction::Export => "export",
            ContainerDataTransferAction::Import => "import",
        },
    )?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_transfer_path(archive_path, action)?;
    if !colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?
    {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }

    let volume = resolve_managed_volume(&policy, volume_name)?;
    run_volume_transfer(
        repo_root,
        &policy,
        &volume.name,
        archive_path,
        action,
        &run_runtime_volume_capture,
    )?;
    let report = data_transfer_report(
        &policy,
        action,
        &ContainerDataVolumeEntry {
            name: volume.name,
            service: volume.service,
            persist: volume.persist,
            size_bytes: volume.size_bytes,
            mount_point: volume.mount_point,
        },
        archive_path,
    );
    Ok(render_container_report(report, output_json))
}

pub fn run_container_data_pull_production<FShared, FReady, FGateway, FHook>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    ensure_shared_services_running: FShared,
    wait_for_container_ready: FReady,
    register_gateway_routes: FGateway,
    execute_pull_production_hook: FHook,
) -> Result<String, EffigyRuntimeError>
where
    FShared: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    FReady: Fn(&EffectiveContainerPolicy) -> Result<Option<&'static str>, EffigyRuntimeError>,
    FGateway: Fn(
        &Path,
        &EffectiveContainerPolicy,
    ) -> Result<Vec<RegisteredGatewayRoute>, EffigyRuntimeError>,
    FHook: Fn(&Path, &EffectiveContainerPolicy, &str) -> Result<(), EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(&policy, "pull-production")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let hook = policy.pull_production_hook.clone().ok_or_else(|| {
        EffigyRuntimeError::task_invocation(format!(
            "container `{}` does not declare `[containers.{}.data].pull_production`",
            policy.name, policy.name
        ))
    })?;

    let colima_started = effigy_containers::exec::ensure_colima_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let shared_service_notes = ensure_shared_services_running(&policy)?;
    crate::signals::run_docker_capture(
        repo_root,
        &policy,
        &effigy_containers::compose::compose_up_args(&policy),
        "docker compose up",
    )?;
    let health = wait_for_container_ready(&policy)?;
    let gateway_routes = register_gateway_routes(repo_root, &policy)?;
    execute_pull_production_hook(repo_root, &policy, &hook)?;

    let mut report = effigy_containers::data_pull_production_report(
        &policy,
        &effigy_containers::ContainerDataHookResult { hook },
        colima_started,
        health,
    );
    annotate_shared_service_notes(&mut report, &shared_service_notes);
    annotate_registered_gateway_routes(&mut report, &gateway_routes);
    annotate_warning_lines(
        &mut report,
        &effigy_containers::exec::colima_profile_warnings(&policy, repo_root),
    );
    Ok(render_container_report(report, output_json))
}

fn ensure_generated_data_path(
    policy: &EffectiveContainerPolicy,
    action: &str,
) -> Result<(), EffigyRuntimeError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `data {action}` is supported only on the generated-compose path",
            policy.name
        )));
    }
    Ok(())
}

fn run_volume_transfer<F>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
    archive_path: &Path,
    action: ContainerDataTransferAction,
    run_runtime_volume_capture: &F,
) -> Result<(), EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let command = match action {
        ContainerDataTransferAction::Export => export_volume_command(volume_name, archive_path),
        ContainerDataTransferAction::Import => import_volume_command(volume_name, archive_path),
    };
    run_runtime_volume_capture(repo_root, &policy.profile, &command)?;
    Ok(())
}

fn hydrate_managed_volumes<F>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ManagedVolume>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut volumes = policy.managed_volumes.clone();
    volumes.sort_by(|left, right| left.name.cmp(&right.name));
    if !colima_running || volumes.is_empty() {
        return Ok(volumes);
    }

    let listed = run_runtime_volume_capture(
        repo_root,
        &policy.profile,
        &list_volumes_command(&policy.project_name),
    )?;
    let listed_names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .collect::<BTreeSet<_>>();

    let mut runtime = Vec::new();
    for volume in &volumes {
        if !listed_names.contains(&volume.name) {
            continue;
        }
        let output = run_runtime_volume_capture(
            repo_root,
            &policy.profile,
            &inspect_volume_command(&volume.name),
        )?;
        if let Some(metadata) =
            parse_inspect_volume_metadata(String::from_utf8_lossy(&output.stdout).as_ref())
        {
            runtime.push(metadata);
        }
    }

    Ok(merge_runtime_volume_metadata(&volumes, &runtime))
}

fn cache_volume_entry(volume: ManagedVolume) -> Option<ContainerCacheVolumeEntry> {
    let kind = volume.cache_kind()?;
    Some(ContainerCacheVolumeEntry {
        name: volume.name,
        service: volume.service,
        kind: cache_kind_label(kind).to_owned(),
        size_bytes: volume.size_bytes,
        mount_point: volume.mount_point,
        mount_target: volume.mount_target,
    })
}

fn cache_kind_label(kind: effigy_catalog::volumes::CacheVolumeKind) -> &'static str {
    match kind {
        effigy_catalog::volumes::CacheVolumeKind::RustTarget => "rust-target",
        effigy_catalog::volumes::CacheVolumeKind::NodeModules => "node-modules",
    }
}

fn cache_kind_from_volume_name(name: &str) -> Option<String> {
    if name.contains("node_modules") || name.contains("node-modules") {
        return Some("node-modules".to_owned());
    }
    if name.contains("cargo-registry") {
        return Some("cargo-registry".to_owned());
    }
    if name.contains("cargo-git") {
        return Some("cargo-git".to_owned());
    }
    if name.contains("target") {
        return Some("rust-target".to_owned());
    }
    None
}

fn collect_global_cache_entries<F>(
    cwd: &Path,
    profile: &str,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ContainerCacheGlobalEntry>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let running_projects = discover_running_environments()?
        .into_iter()
        .map(|environment| environment.policy.project_name)
        .collect::<BTreeSet<_>>();
    let listed = run_runtime_volume_capture(cwd, profile, &list_all_volumes_command())?;
    let names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref());
    let metadata = names
        .iter()
        .filter_map(|name| {
            inspect_runtime_volume_metadata(cwd, profile, name, run_runtime_volume_capture)
                .ok()
                .flatten()
        })
        .map(|entry| (entry.name.clone(), entry))
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok(collect_global_cache_entries_from_names(
        names,
        &running_projects,
        &metadata,
    ))
}

fn inspect_runtime_volume_metadata<F>(
    cwd: &Path,
    profile: &str,
    name: &str,
    run_runtime_volume_capture: &F,
) -> Result<Option<effigy_catalog::volumes::RuntimeVolumeMetadata>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let Some(mut metadata) =
        run_runtime_volume_capture(cwd, profile, &inspect_volume_command(name))
            .ok()
            .and_then(|output| {
                parse_inspect_volume_metadata(String::from_utf8_lossy(&output.stdout).as_ref())
            })
    else {
        return Ok(None);
    };
    if metadata.size_bytes.is_none() {
        if let Some(mount_point) = metadata.mount_point.as_deref() {
            metadata.size_bytes =
                run_runtime_volume_capture(cwd, profile, &volume_usage_command(mount_point))
                    .ok()
                    .and_then(|output| {
                        parse_volume_usage_bytes(String::from_utf8_lossy(&output.stdout).as_ref())
                    });
        }
    }
    Ok(Some(metadata))
}

fn project_name_from_volume_name(name: &str) -> Option<String> {
    for marker in ["-workspace-", "_stack-iso-", "-app-", "_app-"] {
        if let Some((project, _)) = name.split_once(marker) {
            if !project.is_empty() {
                return Some(project.to_owned());
            }
        }
    }
    if let Some((project, rest)) = name.split_once('_') {
        if !project.is_empty() && rest.starts_with(project) {
            return Some(project.to_owned());
        }
    }
    None
}

fn ensure_cache_prune_target_is_stopped(
    container_name: &str,
    is_running: bool,
) -> Result<(), EffigyRuntimeError> {
    if is_running {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "container `{}` is still running; stop it before purging cache volumes",
            container_name
        )));
    }
    Ok(())
}

fn collect_global_cache_entries_from_names(
    names: Vec<String>,
    running_projects: &BTreeSet<String>,
    metadata_by_name: &std::collections::BTreeMap<
        String,
        effigy_catalog::volumes::RuntimeVolumeMetadata,
    >,
) -> Vec<ContainerCacheGlobalEntry> {
    let mut caches = Vec::new();
    for name in names {
        let Some(kind) = cache_kind_from_volume_name(&name) else {
            continue;
        };
        let project_name = project_name_from_volume_name(&name);
        let metadata = metadata_by_name.get(&name);
        let in_use = project_name
            .as_ref()
            .is_some_and(|project| running_projects.contains(project));
        caches.push(ContainerCacheGlobalEntry {
            project_name,
            in_use,
            name,
            kind,
            size_bytes: metadata.and_then(|entry| entry.size_bytes),
            mount_point: metadata.and_then(|entry| entry.mount_point.clone()),
        });
    }
    caches.sort_by(|left, right| left.name.cmp(&right.name));
    caches
}

fn project_is_running(repo_root: &Path, project_name: &str) -> Result<bool, EffigyRuntimeError> {
    let target_root = canonicalize_or_original(repo_root);
    Ok(discover_running_environments()?
        .into_iter()
        .any(|environment| {
            canonicalize_or_original(Path::new(&environment.repo_root)) == target_root
                && environment.policy.project_name == project_name
        }))
}

fn canonicalize_or_original(path: &Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use effigy_catalog::volumes::RuntimeVolumeMetadata;

    use super::{
        collect_global_cache_entries_from_names, ensure_cache_prune_target_is_stopped,
        project_name_from_volume_name,
    };

    #[test]
    fn project_name_inference_handles_workspace_cache_volumes() {
        assert_eq!(
            project_name_from_volume_name("underlay-reference-dev-workspace-acme-api-target"),
            Some("underlay-reference-dev".to_owned())
        );
        assert_eq!(
            project_name_from_volume_name("acowtancy-dev-workspace-cargo-git"),
            Some("acowtancy-dev".to_owned())
        );
    }

    #[test]
    fn project_name_inference_handles_stack_iso_cache_volumes() {
        assert_eq!(
            project_name_from_volume_name("underlay-reference-dev_stack-iso-poodle-node-modules"),
            Some("underlay-reference-dev".to_owned())
        );
    }

    #[test]
    fn project_name_inference_handles_duplicated_project_prefix_volumes() {
        assert_eq!(
            project_name_from_volume_name(
                "underlay-reference-dev_underlay-reference-dev-cargo-registry"
            ),
            Some("underlay-reference-dev".to_owned())
        );
        assert_eq!(
            project_name_from_volume_name("compli-me-dev_compli-me-dev-api-target"),
            Some("compli-me-dev".to_owned())
        );
    }

    #[test]
    fn cache_prune_rejects_running_targets() {
        let error = ensure_cache_prune_target_is_stopped("stack", true).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("container `stack` is still running"));
    }

    #[test]
    fn global_cache_entries_mark_running_projects_in_use() {
        let mut running_projects = BTreeSet::new();
        running_projects.insert("underlay-reference-dev".to_owned());

        let mut metadata = BTreeMap::new();
        metadata.insert(
            "underlay-reference-dev-workspace-acme-api-target".to_owned(),
            RuntimeVolumeMetadata {
                name: "underlay-reference-dev-workspace-acme-api-target".to_owned(),
                mount_point: Some("/var/lib/mock/target".to_owned()),
                size_bytes: Some(1024),
            },
        );
        metadata.insert(
            "contact-patch-dev-workspace-cargo-git".to_owned(),
            RuntimeVolumeMetadata {
                name: "contact-patch-dev-workspace-cargo-git".to_owned(),
                mount_point: Some("/var/lib/mock/cargo-git".to_owned()),
                size_bytes: Some(2048),
            },
        );

        let entries = collect_global_cache_entries_from_names(
            vec![
                "underlay-reference-dev-workspace-acme-api-target".to_owned(),
                "contact-patch-dev-workspace-cargo-git".to_owned(),
                "contact-patch-dev-db-data".to_owned(),
            ],
            &running_projects,
            &metadata,
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "contact-patch-dev-workspace-cargo-git");
        assert!(!entries[0].in_use);
        assert_eq!(
            entries[1].name,
            "underlay-reference-dev-workspace-acme-api-target"
        );
        assert!(entries[1].in_use);
        assert_eq!(entries[1].kind, "rust-target");
    }
}

fn resolve_managed_volume(
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
) -> Result<ManagedVolume, EffigyRuntimeError> {
    let Some(volume) = policy
        .managed_volumes
        .iter()
        .find(|volume| volume.name == volume_name)
        .cloned()
    else {
        let available = policy
            .managed_volumes
            .iter()
            .map(|volume| format!("`{}`", volume.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(EffigyRuntimeError::task_invocation(format!(
            "managed volume `{volume_name}` is not owned by container `{}` (available: {available})",
            policy.name
        )));
    };
    Ok(volume)
}

fn validate_transfer_path(
    archive_path: &Path,
    action: ContainerDataTransferAction,
) -> Result<(), EffigyRuntimeError> {
    match action {
        ContainerDataTransferAction::Export => {
            let parent = archive_path.parent().unwrap_or(Path::new("."));
            if !parent.is_dir() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "export path parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }
        ContainerDataTransferAction::Import => {
            if !archive_path.is_file() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "import archive not found: {}",
                    archive_path.display()
                )));
            }
        }
    }
    Ok(())
}

fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

fn annotate_registered_gateway_routes(
    report: &mut ContainerCommandReport,
    routes: &[RegisteredGatewayRoute],
) {
    if routes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            serde_json::json!(routes
                .iter()
                .map(|route| serde_json::json!({
                    "action": "registered",
                    "domain": route.domain,
                    "target": route.target,
                    "dns_ip": route.dns_ip.map(|value| value.to_string()),
                    "tls": route.tls,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for route in routes {
        report.success_text.push('\n');
        match route.target.as_deref() {
            Some(target) => report.success_text.push_str(&format!(
                "[gateway] registered {} -> {}",
                route.domain, target
            )),
            None => report.success_text.push_str(&format!(
                "[gateway] registered {} -> dns {}",
                route.domain,
                route
                    .dns_ip
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_owned())
            )),
        }
    }
}

fn annotate_shared_service_notes(report: &mut ContainerCommandReport, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            serde_json::json!({
                "action": "ensured",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[shared] {note}"));
    }
}

fn annotate_warning_lines(report: &mut ContainerCommandReport, warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), serde_json::json!(warnings));
    }
    for warning in warnings {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[warn] {warning}"));
    }
}
