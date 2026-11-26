use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup,
};
use std::path::{Path, PathBuf};

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
        fs_events: Default::default(),
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

/// A private Docker-backend runtime: only a stub `docker` on `PATH`, and no
/// `colima` binary anywhere.
///
/// This is the shape of the Native Linux qualification host (2026-11-20 run):
/// Docker Engine is the resolved backend, so every `colima` invocation is an
/// `ENOENT` launch failure rather than a runtime answer (effigy#1087). The stub
/// records its arguments and answers `ps` from fixture markers, so a caller can
/// prove the backend actually selected was the one probed.
#[cfg(unix)]
pub(in crate::runner) struct DockerBackendFixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    _env: crate::contract_test_support::EnvGuard,
}

#[cfg(unix)]
impl DockerBackendFixture {
    pub(in crate::runner) fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical fixture");
        let bin = root.join("bin");
        let home = root.join("home");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        std::fs::create_dir_all(&home).expect("mkdir home");
        let docker = bin.join("docker");
        std::fs::write(
            &docker,
            DOCKER_BACKEND_STUB.replace("__FIXTURE__", &root.to_string_lossy()),
        )
        .expect("write stub docker");
        let mut permissions = std::fs::metadata(&docker).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&docker, permissions).expect("chmod stub docker");
        // The stub is the whole `PATH`: `colima` is absent, not merely unused.
        let env = crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(bin.display().to_string())),
            ("HOME", Some(home.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ]);
        Self {
            _dir: dir,
            root,
            _env: env,
        }
    }

    /// The fixture directory, which is also the repo root and Compose root.
    pub(in crate::runner) fn root(&self) -> &Path {
        &self.root
    }

    /// A Direct-Compose policy whose repo root is this fixture.
    pub(in crate::runner) fn policy(
        &self,
        project_name: &str,
        primary_service: &str,
    ) -> EffectiveContainerPolicy {
        let compose = self.root.join("compose.yml");
        std::fs::write(&compose, COMPOSE_DECLARATION).expect("write compose declaration");
        let mut policy = effective_container_policy("web", project_name, primary_service, compose);
        policy.repo_root = self.root.clone();
        policy
    }

    /// Fail every `docker` invocation, as a stopped engine does.
    pub(in crate::runner) fn state_engine_stopped(&self) {
        std::fs::write(self.root.join("ps-fails"), "").expect("fixture state");
    }

    /// Answer the Compose `ps` probe with one running row for this project and
    /// service, in the exact `--format` the production parser reads.
    pub(in crate::runner) fn state_running_row(&self, project_name: &str, service: &str) {
        let row = format!(
            "{project_name}-{service}-1\tUp 10 seconds\t\t{project_name}\t{}\t{service}\tFalse\n",
            self.root.display()
        );
        std::fs::write(self.root.join("compose-ps-row"), row).expect("write compose ps row");
    }

    /// Every argument list the stub `docker` received, one call per line.
    pub(in crate::runner) fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("docker-calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[cfg(unix)]
const COMPOSE_DECLARATION: &str =
    "services:\n  app:\n    image: busybox\n  workspace:\n    image: busybox\n";

/// Private Docker CLI double. It records its arguments, then answers from the
/// fixture's markers: `ps-fails` fails every call, and `compose-ps-row` is the
/// inventory row printed for the project-filtered `ps` every inventory probe
/// asks for (the bare liveness `ps` stays empty). Only shell builtins are
/// used, because the fixture's `PATH` deliberately holds nothing else.
#[cfg(unix)]
const DOCKER_BACKEND_STUB: &str = "#!/bin/sh
printf '%s\\n' \"$*\" >> '__FIXTURE__/docker-calls'
if [ -f '__FIXTURE__/ps-fails' ]; then
    echo 'Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?' >&2
    exit 1
fi
case \"$*\" in
    *--filter*)
        if [ -f '__FIXTURE__/compose-ps-row' ]; then
            while IFS= read -r row; do
                printf '%s\\n' \"$row\"
            done < '__FIXTURE__/compose-ps-row'
        fi
        ;;
esac
exit 0
";
