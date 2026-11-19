use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup,
};
use std::path::PathBuf;

pub(in crate::runner) fn effective_container_policy(
    name: &str,
    project_name: &str,
    primary_service: &str,
    compose_file: impl Into<PathBuf>,
) -> EffectiveContainerPolicy {
    let compose_file = compose_file.into();
    EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: name.to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![compose_file],
        managed_volumes: vec![],
        borrowed_volumes: Vec::new(),
        data_peers: Vec::new(),
        data_from: None,
        shared_services: vec![],
        project_name: project_name.to_owned(),
        primary_service: primary_service.to_owned(),
        dns_domain: None,
        dns_base_domain: None,
        runtime_host_key: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: vec![],
        service_aliases: vec![],
        declared_ports: vec![],
        ports_declared_explicitly: false,
        declared_mounts: vec![],
        declared_media_mounts: vec![],
        workspace_source: effigy_containers::artifact_source::WorkspaceSource::Checkout,
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: 60,
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: Vec::new(),
    }
}

/// Write the catalog fragments and manifest rewrite `effigy container migrate`
/// would produce, for every environment that still declares the retired
/// `compose_file` key. Fixtures predate the v0.16 retirement; migrating them
/// before they resolve a policy is the same repository state a migrated
/// consumer has. An unreadable manifest is left to the caller's own loader so
/// the real error surfaces.
pub(crate) fn migrate_declared_compose_files(repo_root: &std::path::Path) {
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let Ok(manifest) = effigy_manifest::load_task_manifest(&manifest_path) else {
        return;
    };
    let Some(containers) = manifest.containers.as_ref() else {
        return;
    };
    let declared = containers
        .environments
        .iter()
        .filter(|(_, config)| config.compose_file.is_some())
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for name in declared {
        migrate_compose_file(repo_root, &name);
    }
}

/// Migration `effigy container migrate` performs for one environment.
pub(crate) fn migrate_compose_file(repo_root: &std::path::Path, container: &str) {
    let plan = effigy_containers::policy::compose_file::plan_migration(repo_root, Some(container))
        .unwrap_or_else(|error| {
            panic!("{}: {error}", repo_root.display());
        });
    for fragment in &plan.migration.fragments {
        std::fs::create_dir_all(&fragment.directory).expect("fragment directory");
        std::fs::write(
            fragment.directory.join("service.toml"),
            &fragment.service_toml,
        )
        .expect("service.toml");
        std::fs::write(
            fragment.directory.join("compose.fragment.yml"),
            &fragment.compose_fragment,
        )
        .expect("compose fragment");
    }
    std::fs::write(&plan.manifest, &plan.manifest_after).expect("migrated manifest");
}

/// A fixture policy load: migrate any retired `compose_file` declaration first.
pub(crate) fn load_fixture_policy(
    repo_root: &std::path::Path,
    requested_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, effigy_containers::ContainerPolicyError> {
    migrate_declared_compose_files(repo_root);
    effigy_containers::load_container_policy(repo_root, requested_name)
}

/// A fixture prepared policy: migrate any retired `compose_file` declaration first.
pub(crate) fn prepare_fixture_policy(
    repo_root: &std::path::Path,
    requested_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, effigy_containers::ContainerPolicyError> {
    migrate_declared_compose_files(repo_root);
    effigy_containers::prepare_container_policy(repo_root, requested_name)
}

/// A fixture creation-time policy: migrate any retired `compose_file` first.
pub(crate) fn load_fixture_policy_for_creation(
    repo_root: &std::path::Path,
    requested_name: Option<&str>,
) -> Result<
    (
        EffectiveContainerPolicy,
        Vec<effigy_containers::WorkspaceExtraMountDecision>,
    ),
    effigy_containers::ContainerPolicyError,
> {
    migrate_declared_compose_files(repo_root);
    effigy_containers::load_container_policy_for_creation(repo_root, requested_name)
}
