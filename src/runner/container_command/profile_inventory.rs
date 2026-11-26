//! Profile-scoped runtime inventory: the containers, volumes and networks one
//! backend profile carries, read with the labels ownership is decided from.
//!
//! Retirement and the project teardown share these reads, so an ownership
//! label is interpreted in one place. Every read names the profile it queries
//! and nothing else.

use std::collections::BTreeSet;
use std::path::Path;

use effigy_catalog::volumes::{
    inspect_volumes_command, parse_inspect_volume_metadata_list_strict,
    parse_listed_resource_names, DockerCommand,
};
use effigy_containers::{
    OwnedResource, ResourceKind, CHECKOUT_LABEL, COMPOSE_PROJECT_LABEL, PROJECT_LABEL,
    REPO_ROOT_LABEL, SCOPE_LABEL,
};

use super::support::run_runtime_volume_capture;
use super::RunnerError;

/// Effigy's managed marker, set on every container and volume Effigy creates.
pub(in crate::runner::container_command) const MANAGED_LABEL: &str = "com.effigy.managed";

/// One container with the labels ownership is read from.
pub(in crate::runner::container_command) struct ContainerRow {
    pub(in crate::runner::container_command) name: String,
    /// `com.docker.compose.project`.
    pub(in crate::runner::container_command) project: Option<String>,
    /// `com.effigy.project`, which Effigy sets alongside the Compose label.
    pub(in crate::runner::container_command) effigy_project: Option<String>,
    pub(in crate::runner::container_command) checkout: Option<String>,
    pub(in crate::runner::container_command) repo_root: Option<String>,
    pub(in crate::runner::container_command) scope: Option<String>,
    pub(in crate::runner::container_command) managed: bool,
    pub(in crate::runner::container_command) barge_lease: Option<String>,
}

impl ContainerRow {
    pub(in crate::runner::container_command) fn owner_path(&self) -> Option<String> {
        self.checkout.clone().or_else(|| self.repo_root.clone())
    }

    /// Whether Effigy or Barge put a labelled ownership mark on this container.
    pub(in crate::runner::container_command) fn carries_effigy_ownership(&self) -> bool {
        self.owner_path().is_some() || self.managed || self.barge_lease.is_some()
    }

    pub(in crate::runner::container_command) fn resource(
        &self,
        kind: ResourceKind,
        profile: &str,
    ) -> OwnedResource {
        OwnedResource {
            kind,
            name: self.name.clone(),
            profile: profile.to_owned(),
            persist: false,
        }
    }
}

const CONTAINER_LISTING_FORMAT: &str = "{{.Names}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.effigy.checkout\"}}\t{{.Label \"com.effigy.repo-root\"}}\t{{.Label \"com.effigy.scope\"}}\t{{.Label \"com.effigy.project\"}}\t{{.Label \"com.effigy.managed\"}}\t{{.Label \"barge.lease\"}}";

/// Every container in every state, with the labels ownership is read from.
/// Columns are read by position and appended only, so a row from a shorter
/// listing still parses: a missing label is simply absent.
pub(in crate::runner::container_command) fn list_containers(
    cwd: &Path,
    profile: &str,
) -> Result<Vec<ContainerRow>, RunnerError> {
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: vec![
            "ps".to_owned(),
            "-a".to_owned(),
            "--format".to_owned(),
            CONTAINER_LISTING_FORMAT.to_owned(),
        ],
        description: "list containers by ownership label".to_owned(),
    };
    let output = run_runtime_volume_capture(cwd, profile, &command)?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next().unwrap_or_default().trim().to_owned();
            let mut next = || {
                fields
                    .next()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            };
            ContainerRow {
                name,
                project: next(),
                checkout: next(),
                repo_root: next(),
                scope: next(),
                effigy_project: next(),
                managed: next().is_some_and(|value| value.eq_ignore_ascii_case("true")),
                barge_lease: next(),
            }
        })
        .collect())
}

pub(in crate::runner::container_command) struct VolumeRow {
    pub(in crate::runner::container_command) name: String,
    pub(in crate::runner::container_command) project: Option<String>,
    pub(in crate::runner::container_command) checkout: Option<String>,
    pub(in crate::runner::container_command) repo_root: Option<String>,
    pub(in crate::runner::container_command) scope: Option<String>,
    pub(in crate::runner::container_command) persist: bool,
    pub(in crate::runner::container_command) managed: bool,
}

impl VolumeRow {
    pub(in crate::runner::container_command) fn owner_path(&self) -> Option<String> {
        self.checkout.clone().or_else(|| self.repo_root.clone())
    }

    pub(in crate::runner::container_command) fn resource(&self, profile: &str) -> OwnedResource {
        OwnedResource {
            kind: ResourceKind::Volume,
            name: self.name.clone(),
            profile: profile.to_owned(),
            persist: self.persist,
        }
    }
}

/// Every volume on the backend, with the labels and persist flag ownership is
/// read from. `nerdctl volume ls` prints no labels, so the names are inspected
/// once per chunk.
pub(in crate::runner::container_command) fn list_volumes(
    cwd: &Path,
    profile: &str,
) -> Result<Vec<VolumeRow>, RunnerError> {
    let list = DockerCommand {
        program: "docker".to_owned(),
        args: vec![
            "volume".to_owned(),
            "ls".to_owned(),
            "--format".to_owned(),
            "{{.Name}}".to_owned(),
        ],
        description: "list volumes by ownership label".to_owned(),
    };
    let output = run_runtime_volume_capture(cwd, profile, &list)?;
    let names = parse_listed_resource_names(&String::from_utf8_lossy(&output.stdout))
        .map_err(|error| RunnerError::task_invocation(format!("list volumes: {error}")))?;
    let mut rows = Vec::with_capacity(names.len());
    for chunk in names.chunks(64) {
        let inspect = DockerCommand {
            program: "docker".to_owned(),
            args: inspect_volumes_command(chunk).args,
            description: "inspect volumes for ownership labels".to_owned(),
        };
        let output = run_runtime_volume_capture(cwd, profile, &inspect)?;
        let metadata = parse_inspect_volume_metadata_list_strict(
            &String::from_utf8_lossy(&output.stdout),
            chunk,
        )
        .map_err(RunnerError::task_invocation)?;
        for entry in metadata {
            let label = |name: &str| entry.labels.get(name).cloned();
            rows.push(VolumeRow {
                name: entry.name,
                project: label(PROJECT_LABEL).or_else(|| label(COMPOSE_PROJECT_LABEL)),
                checkout: label(CHECKOUT_LABEL),
                repo_root: label(REPO_ROOT_LABEL),
                scope: label(SCOPE_LABEL),
                persist: OwnedResource::labels_mark_persistent(&entry.labels),
                managed: label(MANAGED_LABEL)
                    .is_some_and(|value| value.eq_ignore_ascii_case("true")),
            });
        }
    }
    Ok(rows)
}

/// The backend's networks of the given projects. A Compose network carries no
/// Effigy label of its own, so the project label is the ownership proof.
pub(in crate::runner::container_command) fn list_networks(
    cwd: &Path,
    profile: &str,
    projects: &[String],
) -> Result<Vec<String>, RunnerError> {
    let mut found = BTreeSet::new();
    for project in projects {
        let command = DockerCommand {
            program: "docker".to_owned(),
            args: vec![
                "network".to_owned(),
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label={COMPOSE_PROJECT_LABEL}={project}"),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            description: format!("list networks of project {project}"),
        };
        let output = run_runtime_volume_capture(cwd, profile, &command)?;
        for name in parse_listed_resource_names(&String::from_utf8_lossy(&output.stdout))
            .map_err(|error| RunnerError::task_invocation(format!("list networks: {error}")))?
        {
            if !is_builtin_network(&name) {
                found.insert(name);
            }
        }
    }
    Ok(found.into_iter().collect())
}

fn is_builtin_network(name: &str) -> bool {
    matches!(name, "bridge" | "host" | "none")
}
