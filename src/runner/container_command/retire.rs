//! Retire this checkout's runtime resources, by label.
//!
//! Ownership is the runtime's own labels: the Compose project plus Effigy's
//! checkout and scope labels. Retirement asks the backend what carries them,
//! deletes all of it, releases the checkout's gateway routes, ports and
//! loopback addresses by checkout path, and reports exactly what went, what was
//! kept on purpose and what could not be deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_catalog::volumes::{
    inspect_volumes_command, parse_inspect_volume_metadata_list_strict,
    parse_listed_resource_names, remove_volume_command, DockerCommand,
};
use effigy_containers::{
    retire_report, FailedResource, GatewayRelease, KeptResource, OwnedResource, ResourceKind,
    RetireOutcome, CHECKOUT_LABEL, COMPOSE_PROJECT_LABEL, PROJECT_LABEL, REPO_ROOT_LABEL,
    SCOPE_LABEL,
};
use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::registration::cleanup_owned_routes_and_certs;

use super::data::maybe_confirm_destructive_container_action;
use super::support::run_runtime_volume_capture;
use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;
use crate::runner::gateway_command::{gateway_dir, remove_gateway_tls_cert};

mod dead;

const NAMED_CONTAINER_RETIRE_ERROR: &str = "`effigy container <NAME> retire` is unsupported; use `effigy container retire` for the checkout or `effigy container retire --dead` for a checkout that is gone";

const UNLABELLED_KEPT: &str =
    "not labelled as this checkout's; `effigy container down` then `up` relabels it";
const DATA_KEPT: &str = "named data volume (`com.effigy.persist=true`)";
const REQUESTED_KEPT: &str = "kept by `--keep-data`";

pub(super) fn run_container_retire(
    repo_override: Option<PathBuf>,
    name: Option<&str>,
    scope: Option<&str>,
    keep_data: bool,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if name.is_some() {
        return Err(RunnerError::task_invocation(
            NAMED_CONTAINER_RETIRE_ERROR.to_owned(),
        ));
    }
    let checkout = retire_checkout(repo_override)?;
    let checkout_label = checkout.display().to_string();
    let inventory = Inventory::read(&checkout)?;
    let owned = inventory.owned_by(&checkout_label, scope);
    let mut kept = inventory.unlabelled_kept(&owned.projects);
    if owned.resources.is_empty() && kept.is_empty() {
        let outcome = RetireOutcome::default();
        return Ok(render_container_report(
            retire_report(Some(&checkout_label), &outcome),
            output_json,
        ));
    }
    if !owned.resources.is_empty() {
        maybe_confirm_destructive_container_action(
            "effigy container retire",
            "Remove every container, volume and network labelled for this checkout, then release its gateway routes, ports and loopback addresses.",
            output_json,
            yes,
        )?;
    }
    let mut outcome = retire_owned(&checkout, &owned.resources, keep_data)?;
    outcome.kept.splice(0..0, kept.drain(..));
    outcome.gateway = release_gateway(&checkout_label)?;
    finish(outcome, Some(&checkout_label), output_json)
}

/// Retire every labelled stack whose checkout path is gone.
pub(super) fn run_container_retire_dead(
    repo_override: Option<PathBuf>,
    keep_data: bool,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    dead::run(repo_override, keep_data, yes, output_json)
}

pub(super) fn retire_checkout(repo_override: Option<PathBuf>) -> Result<PathBuf, RunnerError> {
    match resolve_active_command_context(repo_override.clone()) {
        Ok(context) => Ok(context.resolved.resolved_root),
        Err(error) => match repo_override {
            Some(path) if path.is_dir() => Ok(path),
            _ => Err(error),
        },
    }
}

/// One labelled runtime object with the checkout and scope its labels name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Labelled {
    pub checkout: String,
    pub scope: Option<String>,
    pub project: Option<String>,
    pub resource: OwnedResource,
}

/// Everything the queried backends carry, read once.
#[derive(Debug, Default)]
pub(super) struct Inventory {
    pub labelled: Vec<Labelled>,
    /// Containers and volumes of a labelled project that carry no Effigy
    /// checkout label of their own. Reported, never adopted.
    pub unlabelled: Vec<(String, OwnedResource)>,
}

/// The resources this checkout owns, and every project name they name.
#[derive(Debug, Default)]
pub(super) struct Owned {
    pub resources: Vec<OwnedResource>,
    pub projects: BTreeSet<String>,
}

impl Inventory {
    /// Read every queried backend once: containers and volumes by their
    /// labels, networks by the project label of the objects that name them.
    pub(super) fn read(cwd: &Path) -> Result<Self, RunnerError> {
        let cwd = observation_cwd(cwd);
        let profiles = query_profiles();
        let mut labelled = Vec::new();
        let mut unlabelled = Vec::new();
        let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for profile in &profiles {
            for row in list_containers(&cwd, profile)? {
                match row.owner_path() {
                    Some(checkout) => {
                        if let Some(project) = &row.project {
                            owners
                                .entry(project.clone())
                                .or_default()
                                .insert(checkout.clone());
                        }
                        labelled.push(Labelled {
                            checkout,
                            scope: row.scope.clone(),
                            project: row.project.clone(),
                            resource: row.resource(ResourceKind::Container, profile),
                        });
                    }
                    None => {
                        if let Some(project) = row.project.clone() {
                            unlabelled.push((
                                project,
                                row.resource(ResourceKind::Container, profile),
                            ));
                        }
                    }
                }
            }
            for row in list_volumes(&cwd, profile)? {
                match row.owner_path() {
                    Some(checkout) => {
                        if let Some(project) = &row.project {
                            owners
                                .entry(project.clone())
                                .or_default()
                                .insert(checkout.clone());
                        }
                        labelled.push(Labelled {
                            checkout,
                            scope: row.scope.clone(),
                            project: row.project.clone(),
                            resource: row.resource(profile),
                        });
                    }
                    None => {
                        if let Some(project) = row.project.clone() {
                            unlabelled.push((project, row.resource(profile)));
                        }
                    }
                }
            }
        }
        let projects = owners.keys().cloned().collect::<Vec<_>>();
        for profile in &profiles {
            for name in list_networks(&cwd, profile, &projects)? {
                let Some(project) = network_project(&name, &projects) else {
                    continue;
                };
                let Some(checkout) = owners
                    .get(&project)
                    .filter(|paths| paths.len() == 1)
                    .and_then(|paths| paths.iter().next().cloned())
                else {
                    continue;
                };
                labelled.push(Labelled {
                    checkout,
                    scope: None,
                    project: Some(project),
                    resource: OwnedResource {
                        kind: ResourceKind::Network,
                        name,
                        profile: profile.clone(),
                        persist: false,
                    },
                });
            }
        }
        Ok(Self { labelled, unlabelled })
    }

    /// The labelled resources of one checkout path, optionally of one scope.
    pub(super) fn owned_by(&self, checkout: &str, scope: Option<&str>) -> Owned {
        let wanted = trim_path(checkout);
        let mut owned = Owned::default();
        owned
            .projects
            .extend(self.unlabelled.iter().map(|(project, _)| project.clone()));
        for entry in &self.labelled {
            if trim_path(&entry.checkout) != wanted {
                continue;
            }
            if let Some(scope) = scope {
                // A network carries no scope label; it is owned through the
                // project its labelled containers and volumes name.
                if matches!(entry.resource.kind, ResourceKind::Container | ResourceKind::Volume)
                    && entry.scope.as_deref().map(str::trim) != Some(scope.trim())
                {
                    continue;
                }
            }
            if let Some(project) = &entry.project {
                owned.projects.insert(project.clone());
            }
            owned.resources.push(entry.resource.clone());
        }
        owned.resources = effigy_containers::unique_resources(owned.resources);
        owned
    }

    /// Unlabelled objects of the given projects, which retirement reports and
    /// keeps so the caller knows to relabel them.
    pub(super) fn unlabelled_kept(&self, projects: &BTreeSet<String>) -> Vec<KeptResource> {
        self.unlabelled
            .iter()
            .filter(|(project, _)| projects.contains(project))
            .map(|(_, resource)| KeptResource {
                resource: resource.clone(),
                reason: UNLABELLED_KEPT,
            })
            .collect()
    }

    /// Every checkout path the labels name, with one resource group each.
    pub(super) fn groups(&self) -> Vec<Group> {
        let mut groups: BTreeMap<String, Group> = BTreeMap::new();
        for entry in &self.labelled {
            let group = groups
                .entry(trim_path(&entry.checkout))
                .or_insert_with(|| Group {
                    checkout: entry.checkout.clone(),
                    ..Group::default()
                });
            group.resources.push(entry.resource.clone());
            if let Some(project) = &entry.project {
                group.projects.insert(project.clone());
            }
        }
        for group in groups.values_mut() {
            group.resources = effigy_containers::unique_resources(group.resources.clone());
        }
        groups.into_values().collect()
    }
}

/// One checkout path's labelled resources.
#[derive(Debug, Default, Clone)]
pub(super) struct Group {
    pub checkout: String,
    pub projects: BTreeSet<String>,
    pub resources: Vec<OwnedResource>,
}

/// Profiles to query: every profile the runtime reports a container for, plus
/// the configured endpoint. A stopped profile is named in a warning and never
/// blocks the rest.
pub(super) fn query_profiles() -> Vec<String> {
    let mut profiles = BTreeSet::new();
    profiles.insert(effigy_containers::backend::global_endpoint(None));
    let inventory = effigy_containers::exec::discover_compose_containers_all_states();
    for row in &inventory.rows {
        profiles.insert(row.profile.clone());
    }
    if let Some(summary) = inventory.incomplete_failure_summary() {
        eprintln!("[warn] runtime inventory is incomplete: {summary}");
    }
    for profile in inventory.stopped_profiles() {
        eprintln!(
            "[warn] Colima profile `{profile}` is stopped; anything it holds cannot be seen or retired until it starts"
        );
    }
    profiles.into_iter().collect()
}

fn trim_path(path: &str) -> String {
    path.trim_end_matches('/').to_owned()
}

struct ContainerRow {
    name: String,
    project: Option<String>,
    checkout: Option<String>,
    repo_root: Option<String>,
    scope: Option<String>,
}

impl ContainerRow {
    fn owner_path(&self) -> Option<String> {
        self.checkout.clone().or_else(|| self.repo_root.clone())
    }

    fn resource(&self, kind: ResourceKind, profile: &str) -> OwnedResource {
        OwnedResource {
            kind,
            name: self.name.clone(),
            profile: profile.to_owned(),
            persist: false,
        }
    }
}

const CONTAINER_LISTING_FORMAT: &str = "{{.Names}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.effigy.checkout\"}}\t{{.Label \"com.effigy.repo-root\"}}\t{{.Label \"com.effigy.scope\"}}";

/// Every container in every state, with the labels ownership is read from.
fn list_containers(cwd: &Path, profile: &str) -> Result<Vec<ContainerRow>, RunnerError> {
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
            }
        })
        .collect())
}

struct VolumeRow {
    name: String,
    project: Option<String>,
    checkout: Option<String>,
    repo_root: Option<String>,
    scope: Option<String>,
    persist: bool,
}

impl VolumeRow {
    fn owner_path(&self) -> Option<String> {
        self.checkout.clone().or_else(|| self.repo_root.clone())
    }

    fn resource(&self, profile: &str) -> OwnedResource {
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
fn list_volumes(cwd: &Path, profile: &str) -> Result<Vec<VolumeRow>, RunnerError> {
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
            });
        }
    }
    Ok(rows)
}

/// The backend's networks of the given projects. A Compose network carries no
/// Effigy label of its own, so the project label is the ownership proof.
fn list_networks(cwd: &Path, profile: &str, projects: &[String]) -> Result<Vec<String>, RunnerError> {
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

/// The project a Compose network belongs to, from its `<project>_<network>`
/// name. Only a project this inventory already knows can match.
fn network_project(name: &str, projects: &[String]) -> Option<String> {
    projects
        .iter()
        .filter(|project| {
            name.strip_prefix(project.as_str())
                .and_then(|rest| rest.strip_prefix('_'))
                .is_some_and(|network| !network.is_empty())
        })
        .max_by_key(|project| project.len())
        .cloned()
}

fn is_builtin_network(name: &str) -> bool {
    matches!(name, "bridge" | "host" | "none")
}

/// Delete every listed resource, keep named data and `--keep-data` volumes, and
/// re-read the backend so only what is really gone counts as removed.
pub(super) fn retire_owned(
    checkout: &Path,
    owned: &[OwnedResource],
    keep_data: bool,
) -> Result<RetireOutcome, RunnerError> {
    let cwd = observation_cwd(checkout);
    let mut outcome = RetireOutcome::default();
    let mut deleted = Vec::new();
    for resource in owned {
        if resource.kind == ResourceKind::Volume {
            if resource.persist {
                outcome.kept.push(KeptResource {
                    resource: resource.clone(),
                    reason: DATA_KEPT,
                });
                continue;
            }
            if keep_data {
                outcome.kept.push(KeptResource {
                    resource: resource.clone(),
                    reason: REQUESTED_KEPT,
                });
                continue;
            }
        }
        match delete_resource(&cwd, resource) {
            Ok(()) => deleted.push(resource.clone()),
            Err(error) => outcome.failed.push(FailedResource {
                resource: resource.clone(),
                error,
            }),
        }
    }
    let remaining = still_there(&cwd, &deleted)?;
    for resource in deleted {
        match remaining.contains(&resource) {
            true => outcome.failed.push(FailedResource {
                resource,
                error: "the backend still lists it after deletion".to_owned(),
            }),
            false => outcome.removed.push(resource),
        }
    }
    Ok(outcome)
}

/// Re-read the backend and report which deleted resources are still listed.
fn still_there(
    cwd: &Path,
    deleted: &[OwnedResource],
) -> Result<BTreeSet<OwnedResource>, RunnerError> {
    let mut present = BTreeSet::new();
    for resource in deleted {
        let (program, args) = match resource.kind {
            ResourceKind::Container => ("ps", vec!["-a"]),
            ResourceKind::Volume => ("volume", vec!["ls"]),
            ResourceKind::Network => ("network", vec!["ls"]),
        };
        let mut command_args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        command_args.extend([
            "--filter".to_owned(),
            format!("name={}", resource.name),
            "--format".to_owned(),
            if resource.kind == ResourceKind::Container {
                "{{.Names}}".to_owned()
            } else {
                "{{.Name}}".to_owned()
            },
        ]);
        let command = DockerCommand {
            program: "docker".to_owned(),
            args: std::iter::once(program.to_owned())
                .chain(command_args)
                .collect(),
            description: format!("confirm removal of {}", resource.name),
        };
        let output = run_runtime_volume_capture(cwd, &resource.profile, &command)?;
        let names = parse_listed_resource_names(&String::from_utf8_lossy(&output.stdout))
            .unwrap_or_default();
        if names.iter().any(|name| name == &resource.name) {
            present.insert(resource.clone());
        }
    }
    Ok(present)
}

fn delete_resource(cwd: &Path, resource: &OwnedResource) -> Result<(), String> {
    let command = match resource.kind {
        ResourceKind::Container => DockerCommand {
            program: "docker".to_owned(),
            args: vec!["rm".to_owned(), "-f".to_owned(), resource.name.clone()],
            description: format!("remove owned container {}", resource.name),
        },
        ResourceKind::Volume => remove_volume_command(&resource.name),
        ResourceKind::Network => DockerCommand {
            program: "docker".to_owned(),
            args: vec![
                "network".to_owned(),
                "rm".to_owned(),
                resource.name.clone(),
            ],
            description: format!("remove owned network {}", resource.name),
        },
    };
    let runtime_error = |error: &str| {
        let text = error.trim();
        if text.is_empty() {
            format!("could not remove {} {}", resource.kind.label(), resource.name)
        } else {
            text.lines().last().unwrap_or(text).to_owned()
        }
    };
    match run_runtime_volume_capture(cwd, &resource.profile, &command) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(runtime_error(&String::from_utf8_lossy(&output.stderr))),
        Err(error) => Err(runtime_error(&error.to_string())),
    }
}

/// Release the checkout's gateway state by checkout path alone: routes and
/// their certificates, port ranges, loopback addresses.
pub(super) fn release_gateway(checkout: &str) -> Result<GatewayRelease, RunnerError> {
    let mut release = GatewayRelease::default();
    let path = super::gateway_registration::gateway_route_table_path()?;
    let removed = cleanup_owned_routes_and_certs(&path, checkout, None, &[], delete_tls_cert)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    release.routes = removed.into_iter().map(|route| route.domain).collect();
    let Some(home) = effigy_containers::effigy_home() else {
        return Ok(release);
    };
    EndpointStore::for_effigy_home(&home).transaction(None, |snapshot| -> Result<_, RunnerError> {
        let mut registry = snapshot.ports()?;
        release.ports = release_names(
            registry
                .allocations
                .iter()
                .filter(|(_, allocation)| allocation.project == checkout)
                .map(|(name, _)| name.clone())
                .collect(),
            |name| {
                registry.deallocate(name);
            },
        );
        snapshot.set_ports(&registry)?;
        Ok(())
    })?;
    EndpointStore::for_routes(&path).transaction(None, |snapshot| -> Result<_, RunnerError> {
        let mut registry = snapshot.loopback()?;
        release.loopbacks = release_names(
            registry
                .assignments
                .iter()
                .filter(|(_, assignment)| assignment.scope == checkout)
                .map(|(identity, _)| identity.clone())
                .collect(),
            |identity| {
                registry.deallocate(identity);
            },
        );
        snapshot.set_loopback(&registry)?;
        Ok(())
    })?;
    Ok(release)
}

fn release_names(names: Vec<String>, mut deallocate: impl FnMut(&str)) -> Vec<String> {
    let mut names = names;
    names.sort();
    for name in &names {
        deallocate(name);
    }
    names
}

fn delete_tls_cert(domain: &str) -> Result<(), effigy_gateway::GatewayError> {
    if remove_gateway_tls_cert(domain).is_ok() {
        return Ok(());
    }
    let certs_dir = gateway_dir()
        .map_err(|error| effigy_gateway::GatewayError::TlsError {
            domain: domain.to_owned(),
            reason: error.to_string(),
        })?
        .join("certs");
    let pem = certs_dir.join(format!("{domain}.pem"));
    let key = certs_dir.join(format!("{domain}-key.pem"));
    let _ = std::fs::remove_file(&pem);
    let _ = std::fs::remove_file(&key);
    if pem.exists() || key.exists() {
        return Err(effigy_gateway::GatewayError::TlsError {
            domain: domain.to_owned(),
            reason: "certificate files remain".to_owned(),
        });
    }
    Ok(())
}

pub(super) fn finish(
    outcome: RetireOutcome,
    checkout: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let ok = outcome.ok();
    let rendered = render_container_report(retire_report(checkout, &outcome), output_json);
    if ok {
        Ok(rendered)
    } else if output_json {
        Err(RunnerError::CommandJsonFailure { rendered })
    } else {
        Err(RunnerError::task_invocation(rendered))
    }
}

fn observation_cwd(checkout: &Path) -> PathBuf {
    if checkout.is_dir() {
        checkout.to_path_buf()
    } else {
        std::env::temp_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labelled(checkout: &str, scope: &str, project: &str, name: &str) -> Labelled {
        Labelled {
            checkout: checkout.to_owned(),
            scope: Some(scope.to_owned()),
            project: Some(project.to_owned()),
            resource: OwnedResource {
                kind: ResourceKind::Container,
                name: name.to_owned(),
                profile: "effigy".to_owned(),
                persist: false,
            },
        }
    }

    #[test]
    fn owned_by_selects_one_checkout_path_and_scope() {
        let inventory = Inventory {
            labelled: vec![
                labelled("/w/one", "aaaa", "app-one", "web"),
                labelled("/w/one", "bbbb", "app-one-b", "jobs"),
                labelled("/w/two", "aaaa", "app-two", "web"),
            ],
            unlabelled: Vec::new(),
        };
        let owned = inventory.owned_by("/w/one", None);
        assert_eq!(owned.resources.len(), 2);
        assert_eq!(
            owned.projects,
            BTreeSet::from(["app-one".to_owned(), "app-one-b".to_owned()])
        );
        let scoped = inventory.owned_by("/w/one/", Some("bbbb"));
        assert_eq!(scoped.resources.len(), 1);
        assert_eq!(scoped.resources[0].name, "jobs");
        assert!(inventory.owned_by("/w/three", None).resources.is_empty());
    }

    #[test]
    fn unlabelled_lookalikes_are_reported_not_owned() {
        let inventory = Inventory {
            labelled: vec![labelled("/w/one", "aaaa", "app-one", "web")],
            unlabelled: vec![(
                "app-one".to_owned(),
                OwnedResource {
                    kind: ResourceKind::Container,
                    name: "legacy".to_owned(),
                    profile: "effigy".to_owned(),
                    persist: false,
                },
            )],
        };
        let owned = inventory.owned_by("/w/one", None);
        assert_eq!(owned.resources.len(), 1);
        let kept = inventory.unlabelled_kept(&owned.projects);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].resource.name, "legacy");
        assert_eq!(kept[0].reason, UNLABELLED_KEPT);
    }

    #[test]
    fn groups_collect_one_entry_per_checkout_path() {
        let inventory = Inventory {
            labelled: vec![
                labelled("/w/one", "aaaa", "app-one", "web"),
                labelled("/w/one/", "aaaa", "app-one", "db"),
                labelled("/w/two", "bbbb", "app-two", "web"),
            ],
            unlabelled: Vec::new(),
        };
        let groups = inventory.groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].resources.len(), 2);
        assert_eq!(groups[1].resources.len(), 1);
    }

    #[test]
    fn network_project_matches_the_longest_known_prefix() {
        let projects = vec!["app".to_owned(), "app-dev".to_owned()];
        assert_eq!(
            network_project("app-dev_wt_default", &projects),
            Some("app-dev".to_owned())
        );
        assert_eq!(network_project("app_default", &projects), Some("app".to_owned()));
        assert_eq!(network_project("other_default", &projects), None);
        assert_eq!(network_project("app_", &projects), None);
    }

    #[test]
    fn released_names_are_sorted_and_deallocated() {
        let mut released = Vec::new();
        let names = release_names(vec!["b".to_owned(), "a".to_owned()], |name| {
            released.push(name.to_owned())
        });
        assert_eq!(names, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(released, names);
    }

    #[test]
    fn a_missing_checkout_still_has_an_observation_cwd() {
        let cwd = observation_cwd(Path::new("/tmp/missing-checkout-for-retire"));
        assert!(cwd.is_dir());
    }
}
