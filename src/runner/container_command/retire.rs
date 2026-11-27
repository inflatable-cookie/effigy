//! Retire this checkout's runtime resources, by label.
//!
//! Ownership is the runtime's own labels: the Compose project plus Effigy's
//! checkout and scope labels. Retirement asks the backend what carries them,
//! deletes all of it, releases the checkout's gateway routes by checkout path,
//! and reports exactly what went, what was kept on purpose and what could not
//! be deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_catalog::volumes::{parse_listed_resource_names, DockerCommand};
use effigy_containers::{
    retire_report, FailedResource, GatewayRelease, KeptResource, OwnedResource, ResourceKind,
    RetireOutcome,
};
use effigy_gateway::registration::withdraw_owned_routes;

use super::barge_lifecycle;
use super::data::maybe_confirm_destructive_container_action;
use super::profile_inventory::{list_containers, list_networks, list_volumes};
use super::support::{remove_named_batch, run_runtime_volume_capture, stop_and_remove_containers};
use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;

mod dead;

const NAMED_CONTAINER_RETIRE_ERROR: &str = "`effigy container <NAME> retire` is unsupported; use `effigy container retire` for the checkout or `effigy container retire --dead` for a checkout that is gone";

const UNLABELLED_KEPT: &str =
    "not labelled as this checkout's; `effigy container down` then `up` relabels it";
const DATA_KEPT: &str = "named data volume (`com.effigy.persist=true`), kept by `--keep-data`";
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
    let declared = checkout_projects(&checkout);
    let owned = inventory.owned_by(&checkout_label, scope, &declared);
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
            "Remove every container, volume and network labelled for this checkout, then release its gateway routes.",
            output_json,
            yes,
        )?;
    }
    crate::runner::fs_relay::stop_checkout_stack_relays(&checkout, &owned.projects, None);
    let mut outcome = retire_owned(&checkout, &owned.resources, keep_data)?;
    return_retired_leases(&inventory, &outcome);
    outcome.kept.splice(0..0, kept.drain(..));
    outcome.gateway = release_gateway(&checkout_label)?;
    finish(outcome, Some(&checkout_label), output_json)
}

/// Return the Barge lease of every project this retirement removed from a
/// profile, once no container of it remains there.
fn return_retired_leases(inventory: &Inventory, outcome: &RetireOutcome) {
    for (profile, project) in inventory.emptied_stacks(&outcome.removed) {
        barge_lifecycle::return_torn_down_stack(&profile, &project);
    }
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
    /// Containers of a labelled project that carry no Effigy checkout label of
    /// their own. Reported, never adopted.
    pub unlabelled: Vec<(String, OwnedResource)>,
    /// Volumes that carry only their Compose project label, with the project
    /// they name. `nerdctl compose` leaves a named volume with nothing else, so
    /// the project plus the `<project>-<volume>` name is its ownership proof.
    pub by_project: Vec<(String, OwnedResource)>,
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
        let mut by_project = Vec::new();
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
                            unlabelled
                                .push((project, row.resource(ResourceKind::Container, profile)));
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
                            by_project.push((project, row.resource(profile)));
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
        Ok(Self {
            labelled,
            unlabelled,
            by_project,
        })
    }

    /// The resources of one checkout path, optionally of one scope.
    ///
    /// `declared` are the checkout's own Compose project names, resolved from
    /// its manifest, so a stack whose containers are already gone still gives
    /// up its project-labelled volumes and networks.
    pub(super) fn owned_by(
        &self,
        checkout: &str,
        scope: Option<&str>,
        declared: &[String],
    ) -> Owned {
        let wanted = checkout;
        let mut owned = Owned::default();
        for entry in &self.labelled {
            if !same_path(&entry.checkout, wanted) {
                continue;
            }
            if let Some(scope) = scope {
                // A network carries no scope label; it is owned through the
                // project its labelled containers and volumes name.
                if matches!(
                    entry.resource.kind,
                    ResourceKind::Container | ResourceKind::Volume
                ) && entry.scope.as_deref().map(str::trim) != Some(scope.trim())
                {
                    continue;
                }
            }
            if let Some(project) = &entry.project {
                owned.projects.insert(project.clone());
            }
            owned.resources.push(entry.resource.clone());
        }
        owned.projects.extend(declared.iter().cloned());
        self.claim_named_volumes(&mut owned);
        owned.resources = effigy_containers::unique_resources(owned.resources);
        owned
    }

    /// Volumes whose only label is their Compose project, claimed by the name
    /// Compose gave them.
    fn claim_named_volumes(&self, owned: &mut Owned) {
        let projects = owned.projects.iter().cloned().collect::<Vec<_>>();
        for (project, resource) in &self.by_project {
            if !owned.projects.contains(project)
                || !effigy_containers::named_volume_belongs_to(&resource.name, project)
            {
                continue;
            }
            if owns_longest_project(&resource.name, &projects) != Some(project.as_str()) {
                continue;
            }
            owned.resources.push(resource.clone());
        }
    }

    /// Unlabelled containers of the given projects, which retirement reports
    /// and keeps so the caller knows to relabel them.
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

    /// The `(profile, project)` stacks that lost something to `removed` and
    /// have no container left, labelled or not.
    pub(super) fn emptied_stacks(&self, removed: &[OwnedResource]) -> BTreeSet<(String, String)> {
        let labelled = self
            .labelled
            .iter()
            .filter_map(|entry| Some((entry.project.as_ref()?, &entry.resource)));
        let others = self
            .unlabelled
            .iter()
            .chain(&self.by_project)
            .map(|entry| (&entry.0, &entry.1));
        let mut touched = BTreeSet::new();
        let mut standing = BTreeSet::new();
        for (project, resource) in labelled.chain(others) {
            let stack = (resource.profile.clone(), project.clone());
            if removed.contains(resource) {
                touched.insert(stack);
            } else if resource.kind == ResourceKind::Container {
                standing.insert(stack);
            }
        }
        touched.retain(|stack| !standing.contains(stack));
        touched
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
        // A project-labelled volume joins the one group that names its project.
        for (project, resource) in &self.by_project {
            let naming = groups
                .values()
                .filter(|group| group.projects.contains(project))
                .map(|group| trim_path(&group.checkout))
                .collect::<Vec<_>>();
            let [owner] = naming.as_slice() else {
                continue;
            };
            if !effigy_containers::named_volume_belongs_to(&resource.name, project) {
                continue;
            }
            if let Some(group) = groups.get_mut(owner) {
                group.resources.push(resource.clone());
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
/// the configured endpoint. A stopped profile publishes nothing and cannot be
/// read without starting its VM, so it is named in a warning and left out
/// instead of failing the retirement (effigy#376).
pub(super) fn query_profiles() -> Vec<String> {
    let inventory = effigy_containers::exec::discover_compose_containers_all_states();
    if let Some(summary) = inventory.incomplete_failure_summary() {
        eprintln!("[warn] runtime inventory is incomplete: {summary}");
    }
    let stopped = inventory.stopped_profiles();
    for profile in &stopped {
        eprintln!(
            "[warn] Colima profile `{profile}` is stopped; anything it holds cannot be seen or retired until it starts"
        );
    }
    let running_profiles =
        effigy_containers::exec::running_colima_profiles(Path::new(".")).unwrap_or_default();
    queried_profiles(
        &effigy_containers::backend::global_endpoint(None),
        inventory
            .rows
            .iter()
            .map(|row| row.profile.as_str())
            .chain(running_profiles.iter().map(String::as_str)),
        &stopped,
    )
}

/// [`query_profiles`]' decision, apart from the runtime it reads.
fn queried_profiles<'a>(
    configured: &str,
    reported: impl IntoIterator<Item = &'a str>,
    stopped: &[&str],
) -> Vec<String> {
    let mut profiles = BTreeSet::from([configured.to_owned()]);
    profiles.extend(reported.into_iter().map(str::to_owned));
    for profile in stopped {
        profiles.remove(*profile);
    }
    profiles.into_iter().collect()
}

fn trim_path(path: &str) -> String {
    path.trim_end_matches('/').to_owned()
}

/// Whether two labelled checkout paths name one directory. The label carries
/// the canonical path Effigy rendered, so a textual match is enough; a path
/// that only differs by a symlinked prefix (`/var` and `/private/var` on
/// macOS) still names this checkout and is compared canonically. A path that
/// no longer exists (a dead checkout) only ever compares textually.
pub(super) fn same_path(label: &str, checkout: &str) -> bool {
    if trim_path(label) == trim_path(checkout) {
        return true;
    }
    let canonical = |value: &str| std::fs::canonicalize(value).ok();
    matches!(
        (canonical(label), canonical(checkout)),
        (Some(left), Some(right)) if left == right
    )
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

/// The checkout's own Compose project names, resolved from its manifest. A
/// manifest that cannot be resolved is a warning: the labels still identify
/// everything except a Compose network or a project-labelled volume of a stack
/// whose containers are already gone.
pub(super) fn checkout_projects(checkout: &Path) -> Vec<String> {
    match effigy_containers::load_all_container_policies(checkout) {
        Ok(policies) => policies
            .into_iter()
            .map(|policy| policy.project_name)
            .collect(),
        Err(error) => {
            eprintln!(
                "[warn] cannot resolve this checkout's container projects ({error}); a project-labelled volume or network of a stack with no containers left cannot be retired"
            );
            Vec::new()
        }
    }
}

/// The project a named volume is built for, from its `<project>-<volume>` name.
/// Only a project the caller already knows can match.
fn owns_longest_project<'a>(name: &str, projects: &'a [String]) -> Option<&'a str> {
    projects
        .iter()
        .filter(|project| effigy_containers::named_volume_belongs_to(name, project))
        .max_by_key(|project| project.len())
        .map(String::as_str)
}

/// Delete every listed resource. `--keep-data` keeps the volumes and names
/// them. Deletion runs in dependency order, containers then networks then
/// volumes, with one batched command per profile and kind. The backend is
/// listed afterwards, once per profile and kind, so only what is really gone
/// counts as removed. The outcome keeps the order of `owned`.
pub(super) fn retire_owned(
    checkout: &Path,
    owned: &[OwnedResource],
    keep_data: bool,
) -> Result<RetireOutcome, RunnerError> {
    let cwd = observation_cwd(checkout);
    let keeps = |resource: &OwnedResource| keep_data && resource.kind == ResourceKind::Volume;
    let attempted = owned
        .iter()
        .filter(|resource| !keeps(resource))
        .cloned()
        .collect::<Vec<_>>();
    let errors = delete_in_phases(&cwd, &attempted);
    let remaining = still_there(&cwd, &attempted)?;
    let mut outcome = RetireOutcome::default();
    for resource in owned {
        if keeps(resource) {
            outcome.kept.push(KeptResource {
                resource: resource.clone(),
                reason: if resource.persist {
                    DATA_KEPT
                } else {
                    REQUESTED_KEPT
                },
            });
        } else if remaining.contains(resource) {
            outcome.failed.push(FailedResource {
                resource: resource.clone(),
                error: errors
                    .get(resource)
                    .cloned()
                    .unwrap_or_else(|| "the backend still lists it".to_owned()),
            });
        } else {
            outcome.removed.push(resource.clone());
        }
    }
    Ok(outcome)
}

/// Delete `resources` in dependency order: containers, networks, then volumes.
/// Each phase issues one batch per profile. Returns the reason of every
/// resource a batch, or its one-at-a-time retry, could not remove.
fn delete_in_phases(cwd: &Path, resources: &[OwnedResource]) -> BTreeMap<OwnedResource, String> {
    let mut errors = BTreeMap::new();
    for kind in [
        ResourceKind::Container,
        ResourceKind::Network,
        ResourceKind::Volume,
    ] {
        let mut by_profile: BTreeMap<&str, Vec<&OwnedResource>> = BTreeMap::new();
        for resource in resources.iter().filter(|resource| resource.kind == kind) {
            by_profile
                .entry(resource.profile.as_str())
                .or_default()
                .push(resource);
        }
        for (profile, batch) in by_profile {
            errors.extend(delete_batch(cwd, profile, kind, &batch));
        }
    }
    errors
}

/// Delete one profile's resources of one kind in a single batch.
fn delete_batch(
    cwd: &Path,
    profile: &str,
    kind: ResourceKind,
    batch: &[&OwnedResource],
) -> Vec<(OwnedResource, String)> {
    let names = batch
        .iter()
        .map(|resource| resource.name.clone())
        .collect::<Vec<_>>();
    let failures = match kind {
        // The stop-then-remove order is the container-removal rule (effigy#376).
        ResourceKind::Container => stop_and_remove_containers(cwd, profile, &names),
        ResourceKind::Network => remove_named_batch(cwd, profile, &["network", "rm"], &names),
        ResourceKind::Volume => remove_named_batch(cwd, profile, &["volume", "rm"], &names),
    };
    failures
        .into_iter()
        .filter_map(|(name, error)| {
            let resource = batch.iter().find(|resource| resource.name == name)?;
            Some((
                (*resource).clone(),
                delete_error(resource, &error.to_string()),
            ))
        })
        .collect()
}

/// The reason one resource could not be deleted, named so a person can act on it.
fn delete_error(resource: &OwnedResource, error: &str) -> String {
    let reason = runtime_reason(error).unwrap_or_else(|| {
        format!(
            "could not remove {} {}",
            resource.kind.label(),
            resource.name
        )
    });
    if reason.contains(&resource.name) {
        reason
    } else {
        format!("{}: {reason}", resource.name)
    }
}

/// Re-read the backend once per `(profile, kind)` and report which attempted
/// resources are still listed.
fn still_there(
    cwd: &Path,
    resources: &[OwnedResource],
) -> Result<BTreeSet<OwnedResource>, RunnerError> {
    let mut listed: BTreeMap<(&str, ResourceKind), BTreeSet<String>> = BTreeMap::new();
    for resource in resources {
        let key = (resource.profile.as_str(), resource.kind);
        if listed.contains_key(&key) {
            continue;
        }
        let output = run_runtime_volume_capture(cwd, &resource.profile, &listing_command(key.1))?;
        let names = parse_listed_resource_names(&String::from_utf8_lossy(&output.stdout))
            .unwrap_or_default();
        listed.insert(key, names.into_iter().collect());
    }
    Ok(resources
        .iter()
        .filter(|resource| {
            listed
                .get(&(resource.profile.as_str(), resource.kind))
                .is_some_and(|names| names.contains(&resource.name))
        })
        .cloned()
        .collect())
}

/// The one listing that names every object of one kind on a profile.
fn listing_command(kind: ResourceKind) -> DockerCommand {
    let args: &[&str] = match kind {
        ResourceKind::Container => &["ps", "-a", "--format", "{{.Names}}"],
        ResourceKind::Volume => &["volume", "ls", "--format", "{{.Name}}"],
        ResourceKind::Network => &["network", "ls", "--format", "{{.Name}}"],
    };
    DockerCommand {
        program: "docker".to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        description: format!("confirm removal of owned {}", kind.label()),
    }
}

/// Release the checkout's gateway state by checkout path alone: its routes.
/// The route table is the only record of loopback ownership, so removing the
/// routes frees the checkout's alias address too. Ports belong to the OS or
/// Compose, so retirement releases none, and
/// certificates are a cache keyed by domain: they stay for the next `up` and
/// the gateway daemon prunes the ones no route names.
pub(super) fn release_gateway(checkout: &str) -> Result<GatewayRelease, RunnerError> {
    let path = super::gateway_registration::gateway_route_table_path()?;
    let removed = withdraw_owned_routes(&path, checkout)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(GatewayRelease {
        routes: removed.into_iter().map(|route| route.domain).collect(),
    })
}

/// The runtime's own sentence from a failed deletion. A wrapped CLI prints the
/// cause first and then its own `fatal` summary and exit line, so those are
/// dropped and the last remaining cause is the reason a person can act on.
fn runtime_reason(stderr: &str) -> Option<String> {
    let lines = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let causes = lines
        .iter()
        .copied()
        .filter(|line| !line.to_ascii_lowercase().contains("level=fatal"))
        .collect::<Vec<_>>();
    let reason = causes.last().or_else(|| lines.last()).copied()?;
    let reason = match (reason.find("msg=\""), reason.rfind('"')) {
        (Some(start), Some(end)) if end > start + 5 => &reason[start + 5..end],
        _ => reason,
    };
    Some(reason.chars().take(400).collect())
}

pub(super) fn finish(
    outcome: RetireOutcome,
    checkout: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let ok = outcome.ok();
    let human = outcome.failed.iter().fold(
        effigy_ui::HumanError::new("container retirement did not complete"),
        |human, failure| human.evidence(&failure.error),
    );
    let report = retire_report(checkout, &outcome);
    if ok {
        Ok(render_container_report(report, output_json))
    } else if output_json {
        Err(RunnerError::structured_json(
            report.json,
            false,
            human,
            None,
        ))
    } else {
        Err(RunnerError::task_invocation(render_container_report(
            report, false,
        )))
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
    use effigy_gateway::endpoint_store::EndpointStore;

    /// A lease is returned only for a stack that lost its last container.
    #[test]
    fn a_stack_is_emptied_only_when_no_container_of_it_remains() {
        let gone = labelled("/work/a", "one", "emptied", "emptied-app-1");
        let removed_sibling = labelled("/work/a", "one", "shared", "shared-app-1");
        let standing = labelled("/work/a", "two", "shared", "shared-app-2");
        // A project with an unlabelled container still has one standing.
        let legacy = labelled("/work/a", "one", "legacy", "legacy-db-1");
        let kept_unlabelled = OwnedResource {
            name: "legacy-app-1".to_owned(),
            ..legacy.resource.clone()
        };
        let removed = [
            gone.resource.clone(),
            removed_sibling.resource.clone(),
            legacy.resource.clone(),
        ];
        let inventory = Inventory {
            labelled: vec![gone, removed_sibling, standing, legacy],
            unlabelled: vec![("legacy".to_owned(), kept_unlabelled)],
            by_project: Vec::new(),
        };

        assert_eq!(
            inventory.emptied_stacks(&removed),
            BTreeSet::from([("effigy".to_owned(), "emptied".to_owned())])
        );
    }

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
            by_project: Vec::new(),
        };
        let owned = inventory.owned_by("/w/one", None, &[]);
        assert_eq!(owned.resources.len(), 2);
        assert_eq!(
            owned.projects,
            BTreeSet::from(["app-one".to_owned(), "app-one-b".to_owned()])
        );
        let scoped = inventory.owned_by("/w/one/", Some("bbbb"), &[]);
        assert_eq!(scoped.resources.len(), 1);
        assert_eq!(scoped.resources[0].name, "jobs");
        assert!(inventory
            .owned_by("/w/three", None, &[])
            .resources
            .is_empty());
    }

    #[test]
    fn unlabelled_lookalikes_are_reported_not_owned() {
        let inventory = Inventory {
            labelled: vec![labelled("/w/one", "aaaa", "app-one", "web")],
            by_project: Vec::new(),
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
        let owned = inventory.owned_by("/w/one", None, &[]);
        assert_eq!(owned.resources.len(), 1);
        let kept = inventory.unlabelled_kept(&owned.projects);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].resource.name, "legacy");
        assert_eq!(kept[0].reason, UNLABELLED_KEPT);
    }

    fn project_volume(project: &str, name: &str) -> (String, OwnedResource) {
        (
            project.to_owned(),
            OwnedResource {
                kind: ResourceKind::Volume,
                name: name.to_owned(),
                profile: "effigy".to_owned(),
                persist: false,
            },
        )
    }

    #[test]
    fn a_volume_labelled_only_by_its_compose_project_is_owned_by_its_name() {
        // Exactly what `nerdctl compose` leaves behind: the named volume has
        // its Compose project label and the name `<project>-<volume>`.
        let inventory = Inventory {
            labelled: vec![labelled("/w/one", "aaaa", "c2raed464", "web-1")],
            unlabelled: Vec::new(),
            by_project: vec![
                project_volume("c2raed464", "c2raed464-cache-data"),
                project_volume("c2raed464", "efv-1234"),
                project_volume("other", "other-cache-data"),
            ],
        };
        let owned = inventory.owned_by("/w/one", None, &[]);
        let volumes = owned
            .resources
            .iter()
            .filter(|resource| resource.kind == ResourceKind::Volume)
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(volumes, vec!["c2raed464-cache-data"]);
        // It is owned, not "kept as unlabelled".
        assert!(inventory.unlabelled_kept(&owned.projects).is_empty());
    }

    #[test]
    fn a_declared_project_claims_its_volumes_when_no_container_is_left() {
        let inventory = Inventory {
            labelled: Vec::new(),
            unlabelled: Vec::new(),
            by_project: vec![project_volume("c2raed464", "c2raed464-cache-data")],
        };
        assert!(inventory.owned_by("/w/one", None, &[]).resources.is_empty());
        let owned = inventory.owned_by("/w/one", None, &["c2raed464".to_owned()]);
        assert_eq!(owned.resources.len(), 1);
        assert_eq!(owned.resources[0].name, "c2raed464-cache-data");
    }

    #[test]
    fn a_project_two_checkouts_share_claims_no_project_labelled_volume() {
        let inventory = Inventory {
            labelled: vec![
                labelled("/w/one", "aaaa", "shared", "web-1"),
                labelled("/w/two", "bbbb", "shared", "web-2"),
            ],
            unlabelled: Vec::new(),
            by_project: vec![project_volume("shared", "shared-cache-data")],
        };
        for group in inventory.groups() {
            assert!(
                !group
                    .resources
                    .iter()
                    .any(|resource| resource.name == "shared-cache-data"),
                "{group:?}"
            );
        }
    }

    #[test]
    fn a_dead_group_claims_the_project_labelled_volume_of_its_stack() {
        let inventory = Inventory {
            labelled: vec![labelled(
                "/w/gone",
                "aaaa",
                "app-dev-wt-aaaaaaaaaaaa",
                "web-1",
            )],
            unlabelled: Vec::new(),
            by_project: vec![
                project_volume(
                    "app-dev-wt-aaaaaaaaaaaa",
                    "app-dev-wt-aaaaaaaaaaaa-cache-data",
                ),
                project_volume("app-dev-wt-aaaaaaaaaaaa", "efv-9999"),
            ],
        };
        let groups = inventory.groups();
        assert_eq!(groups.len(), 1);
        let names = groups[0]
            .resources
            .iter()
            .filter(|resource| resource.kind == ResourceKind::Volume)
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["app-dev-wt-aaaaaaaaaaaa-cache-data"]);
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
            by_project: Vec::new(),
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
        assert_eq!(
            network_project("app_default", &projects),
            Some("app".to_owned())
        );
        assert_eq!(network_project("other_default", &projects), None);
        assert_eq!(network_project("app_", &projects), None);
    }

    #[test]
    fn a_symlinked_prefix_still_names_the_same_checkout() {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let real = temp.path().join("checkout");
        std::fs::create_dir(&real).expect("checkout");
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        assert!(same_path(
            &real.display().to_string(),
            &real.display().to_string()
        ));
        assert!(same_path(
            &link.display().to_string(),
            &real.display().to_string()
        ));
        assert!(!same_path(
            &link.display().to_string(),
            &temp.path().join("other").display().to_string()
        ));
        // A dead checkout only compares textually.
        let gone = temp.path().join("gone");
        assert!(same_path(
            &gone.display().to_string(),
            &gone.display().to_string()
        ));
        assert!(!same_path(
            &link.display().to_string(),
            &gone.display().to_string()
        ));
    }

    #[test]
    fn retirement_releases_the_checkouts_routes_only() {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let gateway_home = temp.path().join("gateway");
        std::fs::create_dir_all(&gateway_home).expect("gateway home");
        let _gateway = crate::runner::gateway_command::set_test_gateway_home(&gateway_home);
        let effigy_home = temp.path().join("effigy");
        std::fs::create_dir_all(&effigy_home).expect("isolated effigy home");
        let checkout = temp.path().join("worker");
        std::fs::create_dir(&checkout).expect("checkout");
        let checkout_label = checkout.display().to_string();
        let other_checkout = temp.path().join("other");
        std::fs::create_dir(&other_checkout).expect("other checkout");

        effigy_containers::with_test_effigy_home(&effigy_home, || {
            let path = super::super::gateway_registration::gateway_route_table_path()
                .expect("route table path");
            for (domain, project) in [
                ("dormant.test", checkout_label.clone()),
                ("other.test", other_checkout.display().to_string()),
            ] {
                effigy_gateway::registration::register_route(
                    &path,
                    &effigy_gateway::registration::RouteRegistration {
                        domain: domain.to_owned(),
                        target: Some("127.0.0.1:19900".to_owned()),
                        dns_ip: None,
                        tcp_port: None,
                        tcp_target: None,
                        tls: false,
                        project_path: project,
                        source: effigy_gateway::routes::RouteSource::Container,
                    },
                )
                .expect("register route");
            }

            let certs = crate::runner::gateway_command::gateway_dir()
                .expect("gateway directory")
                .join("certs");
            std::fs::create_dir_all(&certs).expect("certs directory");
            for domain in ["dormant.test", "other.test"] {
                std::fs::write(certs.join(format!("{domain}.pem")), "cert").expect("cert file");
                std::fs::write(certs.join(format!("{domain}-key.pem")), "key").expect("key file");
            }

            let release =
                release_gateway(&checkout_label).expect("release the checkout's gateway state");
            assert_eq!(release.routes, vec!["dormant.test".to_owned()]);
            for domain in ["dormant.test", "other.test"] {
                assert!(
                    certs.join(format!("{domain}.pem")).exists(),
                    "a certificate is a cache and survives retirement"
                );
                assert!(certs.join(format!("{domain}-key.pem")).exists());
            }

            let snapshot = EndpointStore::for_routes(&path)
                .snapshot()
                .expect("endpoint store");
            let domains = snapshot
                .routes()
                .expect("routes")
                .all_routes()
                .into_iter()
                .map(|route| route.domain.clone())
                .collect::<Vec<_>>();
            assert_eq!(domains, vec!["other.test".to_owned()]);
        });
    }

    #[test]
    fn a_stopped_profile_is_never_queried_even_when_it_is_the_endpoint() {
        assert_eq!(
            queried_profiles(
                "effigy-release",
                ["effigy", "effigy-release"],
                &["effigy-release"]
            ),
            vec!["effigy".to_owned()]
        );
    }
}
