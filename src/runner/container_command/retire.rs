use std::path::PathBuf;

use effigy_catalog::volumes::{
    inspect_volumes_command, parse_inspect_volume_metadata_list_strict,
    parse_listed_resource_names, remove_volume_command, DockerCommand,
};
use effigy_containers::{
    load_for_checkout, load_scope_record, plan_retirement, remaining_after, remove_scope_record,
    retire_report, retire_report_unverified, upsert_scope_record, volume_has_ownership_proof,
    ObservedKind, ObservedResource, ScopeComposeKind, ScopeRecord, COMPOSE_PROJECT_LABEL,
    PROJECT_LABEL, SCOPE_LABEL,
};
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::ports::PortRegistry;
use effigy_gateway::registration::{cleanup_owned_routes_and_certs, owned_by};
use effigy_gateway::routes::{RouteTable, RouteTableLock};

use super::data::maybe_confirm_destructive_container_action;
use super::support::run_runtime_volume_capture;
use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;
use crate::runner::gateway_command::{gateway_dir, remove_gateway_tls_cert};

pub(super) fn run_container_retire(
    repo_override: Option<PathBuf>,
    name: Option<&str>,
    scope: Option<&str>,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let records = resolve_retire_records(repo_override, name, scope)?;
    if records.is_empty() {
        return Ok(render_container_report(
            retire_report(None, None, &[], &[], false),
            output_json,
        ));
    }
    if !records
        .iter()
        .all(|record| record.compose_kind == ScopeComposeKind::SharedIdentity)
    {
        maybe_confirm_destructive_container_action(
            "effigy container retire",
            "Remove owned containers, networks, mutable volumes, routes and ports for this runtime scope.",
            output_json,
            yes,
        )?;
    }
    let mut last_report = None;
    for record in records {
        last_report = Some(retire_one_record(&record, output_json)?);
    }
    last_report.ok_or_else(|| RunnerError::task_invocation("no runtime scope to retire"))
}

fn resolve_retire_records(
    repo_override: Option<PathBuf>,
    _name: Option<&str>,
    scope: Option<&str>,
) -> Result<Vec<ScopeRecord>, RunnerError> {
    if let Some(token) = scope {
        return Ok(load_scope_record(token)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            .into_iter()
            .collect());
    }
    let checkout = match resolve_active_command_context(repo_override.clone()) {
        Ok(context) => context.resolved.resolved_root,
        Err(_) => {
            if let Some(path) = repo_override {
                return load_for_checkout(&path)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()));
            }
            return Ok(Vec::new());
        }
    };
    load_for_checkout(&checkout).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn retire_one_record(record: &ScopeRecord, output_json: bool) -> Result<String, RunnerError> {
    let record = remember_pending_tls(record)?;
    let (live, stopped) = split_stopped_profiles(&record);
    let observed = observe_scope(&record, &live)?;
    let plan = plan_retirement(&record, &observed);
    let mut removed = Vec::new();
    let mut routes_cleared = false;
    let mut ports_cleared = false;
    let mut loopbacks_cleared = false;
    for resource in &plan.delete {
        let deleted = match resource.kind {
            ObservedKind::Route | ObservedKind::TlsCert if routes_cleared => true,
            ObservedKind::Port if ports_cleared => true,
            ObservedKind::Loopback if loopbacks_cleared => true,
            ObservedKind::Route | ObservedKind::TlsCert => {
                routes_cleared = true;
                delete_routes_and_owned_certs(&record)?
            }
            ObservedKind::Port => {
                ports_cleared = true;
                delete_ports(&record)?
            }
            ObservedKind::Loopback => {
                loopbacks_cleared = true;
                delete_loopbacks(&record)?
            }
            ObservedKind::Container => delete_container(&record, resource)?,
            ObservedKind::Volume => delete_volume(&record, resource)?,
            ObservedKind::Network => delete_network(&record, resource)?,
        };
        if deleted {
            removed.push(resource.clone());
        }
    }
    let remaining = remaining_after(&record, &observe_scope(&record, &live)?);
    let record_removed = remaining.is_empty() && stopped.is_empty();
    if record_removed {
        remove_scope_record(&record.token)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    let report = retire_report_unverified(
        Some(&record),
        Some(&plan),
        &removed,
        &remaining,
        record_removed,
        &stopped,
    );
    if remaining.is_empty() && stopped.is_empty() {
        Ok(render_container_report(report, output_json))
    } else {
        let rendered = render_container_report(report, output_json);
        // JSON mode keeps the structured report in `error.details`.
        if output_json {
            Err(RunnerError::CommandJsonFailure { rendered })
        } else {
            Err(RunnerError::task_invocation(rendered))
        }
    }
}

/// Splits recorded profiles into queryable and stopped. A stopped profile is
/// never skipped silently: the caller keeps the record and reports it
/// unverified. Other probe failures stay live so the real query surfaces them.
fn split_stopped_profiles(record: &ScopeRecord) -> (Vec<String>, Vec<String>) {
    let mut live = Vec::new();
    let mut stopped = Vec::new();
    for profile in record.observation_profiles() {
        let probe = DockerCommand {
            program: "docker".to_owned(),
            args: vec!["ps".to_owned(), "-q".to_owned(), "--latest".to_owned()],
            description: format!("probe runtime profile {profile}"),
        };
        match run_runtime_volume_capture(&observation_cwd(record), &profile, &probe) {
            Err(error) if runtime_stopped_message(&error.to_string()) => stopped.push(profile),
            _ => live.push(profile),
        }
    }
    (live, stopped)
}

fn runtime_stopped_message(message: &str) -> bool {
    message.contains("is not running")
        || message.contains("Cannot connect to the Docker daemon")
        || message.contains("daemon is not running")
}

fn observe_scope(
    record: &ScopeRecord,
    live_profiles: &[String],
) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut scoped = record.clone();
    scoped.profiles = live_profiles.to_vec();
    let mut observed = Vec::new();
    if !live_profiles.is_empty() {
        observed.extend(observe_labeled_containers(&scoped)?);
        observed.extend(observe_networks(&scoped)?);
        observed.extend(observe_volumes(&scoped)?);
    }
    observed.extend(observe_routes(record)?);
    observed.extend(observe_ports(record)?);
    observed.extend(observe_loopbacks(record)?);
    observed.extend(observe_tls_certs(record)?);
    Ok(observed)
}

fn observe_labeled_containers(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    for filter in ownership_label_filters(record) {
        found.extend(list_named_resources(
            record,
            ObservedKind::Container,
            "ps",
            &container_ps_args(&filter.label),
            &filter.description("containers"),
            filter.scope_label.as_deref(),
            filter.project_label.clone(),
        )?);
    }
    Ok(unique_resources(found))
}

fn observe_networks(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    for filter in ownership_label_filters(record) {
        found.extend(list_named_resources(
            record,
            ObservedKind::Network,
            "network",
            &[
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label={}", filter.label),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            &filter.description("networks"),
            filter.scope_label.as_deref(),
            filter.project_label.clone(),
        )?);
    }
    found.retain(|resource| !is_builtin_network(&resource.name));
    Ok(unique_resources(found))
}

fn observe_volumes(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    for profile in record.observation_profiles() {
        found.extend(observe_volumes_for_profile(record, &profile)?);
    }
    Ok(unique_resources(found))
}

fn observe_volumes_for_profile(
    record: &ScopeRecord,
    profile: &str,
) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut names = Vec::new();
    for filter in ownership_label_filters(record) {
        names.extend(list_names(
            record,
            profile,
            "volume",
            &[
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label={}", filter.label),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            &filter.description("volumes"),
        )?);
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let output = run_runtime_volume_capture(
        &observation_cwd(record),
        profile,
        &inspect_volumes_command(&names),
    )?;
    let metadata =
        parse_inspect_volume_metadata_list_strict(&String::from_utf8_lossy(&output.stdout), &names)
            .map_err(RunnerError::task_invocation)?;
    let mut observed = Vec::with_capacity(metadata.len());
    for entry in metadata {
        if !volume_has_ownership_proof(&entry.labels) {
            return Err(RunnerError::task_invocation(format!(
                "volume inspect for `{}` has no scope or compose project label",
                entry.name
            )));
        }
        let persist = record.volume_is_persistent(&entry.name, &entry.labels);
        let external = entry
            .labels
            .get("com.docker.compose.external")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        observed.push(ObservedResource {
            kind: ObservedKind::Volume,
            name: entry.name,
            scope_label: entry.labels.get(SCOPE_LABEL).cloned(),
            project_label: entry
                .labels
                .get(PROJECT_LABEL)
                .cloned()
                .or_else(|| entry.labels.get(COMPOSE_PROJECT_LABEL).cloned()),
            persist,
            external,
            profile: Some(profile.to_owned()),
        });
    }
    Ok(observed)
}

fn list_named_resources(
    record: &ScopeRecord,
    kind: ObservedKind,
    program: &str,
    args: &[String],
    description: &str,
    scope_label: Option<&str>,
    project_label: Option<String>,
) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    for profile in record.observation_profiles() {
        found.extend(
            list_names(record, &profile, program, args, description)?
                .into_iter()
                .map(|name| ObservedResource {
                    kind,
                    name,
                    scope_label: scope_label.map(str::to_owned),
                    project_label: project_label.clone(),
                    persist: false,
                    external: false,
                    profile: Some(profile.clone()),
                }),
        );
    }
    Ok(found)
}

fn list_names(
    record: &ScopeRecord,
    profile: &str,
    program: &str,
    args: &[String],
    description: &str,
) -> Result<Vec<String>, RunnerError> {
    let mut command_args = vec![program.to_owned()];
    command_args.extend(args.iter().cloned());
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: command_args,
        description: description.to_owned(),
    };
    let output = run_runtime_volume_capture(&observation_cwd(record), profile, &command)?;
    parse_listed_resource_names(&String::from_utf8_lossy(&output.stdout))
        .map_err(|error| RunnerError::task_invocation(format!("{description}: {error}")))
}

fn unique_resources(mut found: Vec<ObservedResource>) -> Vec<ObservedResource> {
    found.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.name.cmp(&right.name))
            .then(left.profile.cmp(&right.profile))
    });
    found.dedup_by(|left, right| {
        left.kind == right.kind && left.name == right.name && left.profile == right.profile
    });
    found
}

fn is_builtin_network(name: &str) -> bool {
    matches!(name, "bridge" | "host" | "none")
}

struct OwnershipLabelFilter {
    label: String,
    scope_label: Option<String>,
    project_label: Option<String>,
    kind: &'static str,
}

impl OwnershipLabelFilter {
    fn description(&self, resource: &str) -> String {
        match self.kind {
            "scope" => format!("list {resource} by runtime scope label"),
            "compose" => format!(
                "list {resource} for compose project {}",
                self.project_label.as_deref().unwrap_or("unknown")
            ),
            _ => format!(
                "list {resource} for project {}",
                self.project_label.as_deref().unwrap_or("unknown")
            ),
        }
    }
}

fn ownership_label_filters(record: &ScopeRecord) -> Vec<OwnershipLabelFilter> {
    let mut filters = vec![OwnershipLabelFilter {
        label: format!("{SCOPE_LABEL}={}", record.token),
        scope_label: Some(record.token.clone()),
        project_label: record.project_names.first().cloned(),
        kind: "scope",
    }];
    for project in &record.project_names {
        filters.push(OwnershipLabelFilter {
            label: format!("{COMPOSE_PROJECT_LABEL}={project}"),
            scope_label: None,
            project_label: Some(project.clone()),
            kind: "compose",
        });
        filters.push(OwnershipLabelFilter {
            label: format!("{PROJECT_LABEL}={project}"),
            scope_label: None,
            project_label: Some(project.clone()),
            kind: "project",
        });
    }
    filters
}

/// `docker ps -a` so Created and exited containers are retired with running ones.
fn container_ps_args(label: &str) -> Vec<String> {
    vec![
        "-a".to_owned(),
        "--filter".to_owned(),
        format!("label={label}"),
        "--format".to_owned(),
        "{{.Names}}".to_owned(),
    ]
}

fn observe_routes(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let path = super::gateway_registration::gateway_route_table_path()?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let table = effigy_gateway::routes::RouteTable::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(table
        .all_routes()
        .into_iter()
        .filter(|route| {
            route.project == record.checkout
                && route.scope.as_deref() == Some(record.token.as_str())
        })
        .map(|route| ObservedResource {
            kind: ObservedKind::Route,
            name: route.domain.clone(),
            scope_label: route.scope.clone(),
            project_label: Some(route.project.clone()),
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn observe_ports(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let Some(home) = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".effigy"))
    else {
        return Ok(Vec::new());
    };
    let path = home.join("ports.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let registry = PortRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(record
        .project_names
        .iter()
        .filter(|name| registry.get(name).is_some())
        .map(|name| ObservedResource {
            kind: ObservedKind::Port,
            name: name.clone(),
            scope_label: Some(record.token.clone()),
            project_label: Some(name.clone()),
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn observe_loopbacks(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let path = gateway_dir()?.join("loopback-ips.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let registry = LoopbackRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut identities = record
        .loopback_identities
        .iter()
        .chain(record.retain_loopback_identities.iter())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    identities.extend(
        record
            .project_names
            .iter()
            .filter(|identity| {
                registry
                    .get(identity)
                    .is_some_and(|assignment| assignment.scope == record.checkout)
            })
            .cloned(),
    );
    Ok(identities
        .iter()
        .filter(|identity| registry.get(identity).is_some())
        .map(|identity| ObservedResource {
            kind: ObservedKind::Loopback,
            name: identity.clone(),
            scope_label: Some(record.token.clone()),
            project_label: record.project_names.first().cloned(),
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn observe_tls_certs(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    if record.pending_tls_certs.is_empty() {
        return Ok(Vec::new());
    }
    let certs_dir = gateway_dir()?.join("certs");
    let path = super::gateway_registration::gateway_route_table_path()?;
    let table = if path.is_file() {
        RouteTable::load(&path).map_err(|error| RunnerError::task_invocation(error.to_string()))?
    } else {
        RouteTable::new()
    };
    Ok(record
        .pending_tls_certs
        .iter()
        .filter(|domain| {
            let files_exist = certs_dir.join(format!("{domain}.pem")).exists()
                || certs_dir.join(format!("{domain}-key.pem")).exists();
            if !files_exist {
                return false;
            }
            match table.lookup(domain) {
                Some(route) => owned_by(route, &record.checkout, Some(&record.token)),
                None => true,
            }
        })
        .map(|domain| ObservedResource {
            kind: ObservedKind::TlsCert,
            name: domain.clone(),
            scope_label: Some(record.token.clone()),
            project_label: None,
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn remember_pending_tls(record: &ScopeRecord) -> Result<ScopeRecord, RunnerError> {
    let mut domains = record.pending_tls_certs.clone();
    let path = super::gateway_registration::gateway_route_table_path()?;
    if path.is_file() {
        let table = RouteTable::load(&path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        for route in table.all_routes() {
            if route.tls
                && route.project == record.checkout
                && route.scope.as_deref() == Some(record.token.as_str())
                && record.routes.iter().any(|domain| domain == &route.domain)
                && !domains.iter().any(|domain| domain == &route.domain)
            {
                domains.push(route.domain.clone());
            }
        }
    }
    if domains == record.pending_tls_certs {
        return Ok(record.clone());
    }
    let mut next = record.clone();
    next.pending_tls_certs = domains;
    upsert_scope_record(&next).map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    load_scope_record(&record.token)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .ok_or_else(|| {
            RunnerError::task_invocation("runtime-scope record vanished after TLS inventory write")
        })
}

fn delete_container(
    record: &ScopeRecord,
    resource: &ObservedResource,
) -> Result<bool, RunnerError> {
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["rm".to_owned(), "-f".to_owned(), resource.name.clone()],
        description: format!("remove owned container {}", resource.name),
    };
    Ok(delete_on_observed_profile(record, resource, &command))
}

fn delete_network(record: &ScopeRecord, resource: &ObservedResource) -> Result<bool, RunnerError> {
    if is_builtin_network(&resource.name) {
        return Ok(true);
    }
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["network".to_owned(), "rm".to_owned(), resource.name.clone()],
        description: format!("remove owned network {}", resource.name),
    };
    Ok(delete_on_observed_profile(record, resource, &command))
}

fn delete_volume(record: &ScopeRecord, resource: &ObservedResource) -> Result<bool, RunnerError> {
    Ok(delete_on_observed_profile(
        record,
        resource,
        &remove_volume_command(&resource.name),
    ))
}

fn delete_on_observed_profile(
    record: &ScopeRecord,
    resource: &ObservedResource,
    command: &DockerCommand,
) -> bool {
    let Some(profile) = resource.profile.as_deref() else {
        return false;
    };
    run_runtime_volume_capture(&observation_cwd(record), profile, command)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn delete_tls_cert(domain: &str) -> Result<bool, RunnerError> {
    if remove_gateway_tls_cert(domain).is_ok() {
        return Ok(true);
    }
    let certs_dir = gateway_dir()?.join("certs");
    let pem = certs_dir.join(format!("{domain}.pem"));
    let key = certs_dir.join(format!("{domain}-key.pem"));
    let _ = std::fs::remove_file(&pem);
    let _ = std::fs::remove_file(&key);
    Ok(!pem.exists() && !key.exists())
}

fn delete_routes_and_owned_certs(record: &ScopeRecord) -> Result<bool, RunnerError> {
    let path = super::gateway_registration::gateway_route_table_path()?;
    cleanup_owned_routes_and_certs(
        &path,
        &record.checkout,
        Some(&record.token),
        Some(&record.routes),
        &record.pending_tls_certs,
        |domain| match delete_tls_cert(domain) {
            Ok(true) => Ok(()),
            Ok(false) => Err(effigy_gateway::GatewayError::TlsError {
                domain: domain.to_owned(),
                reason: "certificate files remain".to_owned(),
            }),
            Err(error) => Err(effigy_gateway::GatewayError::TlsError {
                domain: domain.to_owned(),
                reason: error.to_string(),
            }),
        },
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(true)
}

fn delete_ports(record: &ScopeRecord) -> Result<bool, RunnerError> {
    let Some(home) = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".effigy"))
    else {
        return Ok(true);
    };
    let path = home.join("ports.json");
    if !path.is_file() {
        return Ok(true);
    }
    let _lock = RouteTableLock::acquire(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut registry = PortRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    for project in &record.project_names {
        registry.deallocate(project);
    }
    registry
        .save(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(true)
}

fn delete_loopbacks(record: &ScopeRecord) -> Result<bool, RunnerError> {
    let path = gateway_dir()?.join("loopback-ips.json");
    if !path.is_file() {
        return Ok(true);
    }
    let _lock = RouteTableLock::acquire(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut registry = LoopbackRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut identities = record
        .loopback_identities
        .iter()
        .filter(|identity| {
            registry
                .get(identity)
                .is_some_and(|assignment| assignment.scope == record.checkout)
        })
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    identities.extend(
        record
            .project_names
            .iter()
            .filter(|identity| {
                registry
                    .get(identity)
                    .is_some_and(|assignment| assignment.scope == record.checkout)
            })
            .cloned(),
    );
    for identity in identities {
        registry.deallocate(&identity);
    }
    registry
        .save(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(true)
}

fn observation_cwd(record: &ScopeRecord) -> PathBuf {
    let path = PathBuf::from(&record.checkout);
    if path.is_dir() {
        path
    } else {
        std::env::temp_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retire_of_unactivated_checkout_does_not_create_scope_or_probe_profile() {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let checkout = temp.path().join("worker");
        let home = temp.path().join("home");
        std::fs::create_dir_all(checkout.join(".git")).expect("checkout git metadata");
        std::fs::create_dir_all(&home).expect("temporary home");
        std::fs::write(
            checkout.join(".git/config"),
            "[core]\n\trepositoryformatversion = 0\n[effigy]\n\truntimeScope = ephemeral\n",
        )
        .expect("mark checkout as ephemeral");
        std::fs::write(
            checkout.join("effigy.toml"),
            "[containers]\ndefault = \"web\"\n\n[containers.web]\nprofile = \"private-stopped-profile\"\ncompose_file = \"compose.yaml\"\nprimary_service = \"app\"\n",
        )
        .expect("write manifest");
        std::fs::write(
            checkout.join("compose.yaml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose file");

        let output = effigy_containers::with_test_effigy_home(&home.join(".effigy"), || {
            run_container_retire(Some(checkout.clone()), None, None, true, true)
        })
        .expect("unactivated scope retirement should be a no-op");

        let report: serde_json::Value = serde_json::from_str(&output).expect("retire report JSON");
        assert_eq!(report["ok"], true);
        assert!(!home.join(".effigy/runtime-scopes").exists());
    }

    #[test]
    fn checkout_retirement_lookup_preserves_existing_unknown_profile_record() {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let checkout = temp.path().join("worker");
        let home = temp.path().join("home");
        std::fs::create_dir_all(checkout.join(".git")).expect("checkout git metadata");
        std::fs::create_dir_all(&home).expect("temporary home");
        std::fs::write(
            checkout.join(".git/config"),
            "[core]\n\trepositoryformatversion = 0\n[effigy]\n\truntimeScope = ephemeral\n",
        )
        .expect("mark checkout as ephemeral");
        std::fs::write(
            checkout.join("effigy.toml"),
            "[containers]\ndefault = \"web\"\n\n[containers.web]\nprofile = \"private-stopped-profile\"\ncompose_file = \"compose.yaml\"\nprimary_service = \"app\"\n",
        )
        .expect("write manifest");
        std::fs::write(
            checkout.join("compose.yaml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose file");
        let checkout = checkout.canonicalize().expect("canonical checkout path");

        effigy_containers::with_test_effigy_home(&home.join(".effigy"), || {
            let policy =
                effigy_containers::load_container_policy(&checkout, None).expect("resolve policy");
            effigy_containers::register_container_runtime_scope(&policy)
                .expect("record activated scope fixture");
            let token = effigy_core::worktree_scope::load_or_create(&checkout)
                .expect("resolve fixture token")
                .expect("ephemeral fixture scope");
            let mut record = load_scope_record(&token)
                .expect("load scope")
                .expect("record exists");
            record.profile = "unknown-stopped-profile".to_owned();
            record.profiles = vec!["unknown-stopped-profile".to_owned()];
            upsert_scope_record(&record).expect("preserve unknown profile in record");
            let record_path = home
                .join(".effigy/runtime-scopes")
                .join(format!("{token}.json"));
            let before = std::fs::read(&record_path).expect("read record before lookup");

            let records = resolve_retire_records(Some(checkout), None, None)
                .expect("resolve existing retirement record");
            let after = std::fs::read(&record_path).expect("read record after lookup");

            assert_eq!(records.len(), 1);
            assert_eq!(records[0].profile, "unknown-stopped-profile");
            assert_eq!(after, before);
        });
    }

    fn sample_record() -> ScopeRecord {
        ScopeRecord {
            schema: "effigy.runtime-scope.v1".to_owned(),
            schema_version: 1,
            token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            host_key: "aaaaaaaa".to_owned(),
            checkout: "/tmp/missing-worker".to_owned(),
            updated_unix: 1,
            compose_kind: ScopeComposeKind::Generated,
            profile: "effigy".to_owned(),
            profiles: vec!["effigy".to_owned(), "jobs".to_owned()],
            project_names: vec!["app-dev-wt-aaaaaaaaaaaa".to_owned()],
            retain_project_names: vec![],
            repo_owned_projects: vec![],
            owned_volumes: vec![],
            retain_volumes: vec![],
            routes: vec![],
            retain_routes: vec![],
            loopback_identities: vec![],
            retain_loopback_identities: vec![],
            pending_tls_certs: vec![],
        }
    }

    #[test]
    fn container_listing_includes_created_and_stopped() {
        let args = container_ps_args("com.effigy.scope=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(args[0], "-a");
        assert!(args.contains(&"--filter".to_owned()));
        assert!(args.iter().any(|arg| arg.starts_with("label=")));
    }

    #[test]
    fn ownership_filters_cover_scope_compose_and_project_labels() {
        let filters = ownership_label_filters(&sample_record());
        let labels = filters
            .iter()
            .map(|filter| filter.label.as_str())
            .collect::<Vec<_>>();
        assert!(labels.contains(&"com.effigy.scope=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(labels.contains(&"com.docker.compose.project=app-dev-wt-aaaaaaaaaaaa"));
        assert!(labels.contains(&"com.effigy.project=app-dev-wt-aaaaaaaaaaaa"));
    }

    #[test]
    fn missing_checkout_still_has_an_observation_cwd() {
        let cwd = observation_cwd(&sample_record());
        assert!(cwd.is_dir());
        assert_ne!(cwd, PathBuf::from("/tmp/missing-worker"));
    }

    #[test]
    fn retirement_clears_qualified_and_owned_legacy_loopbacks_only() {
        let home = std::env::temp_dir().join(format!(
            "effigy-loopback-retirement-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&home).expect("create test home");
        let _gateway_home = crate::runner::gateway_command::set_test_gateway_home(&home);
        let path = gateway_dir()
            .expect("gateway dir")
            .join("loopback-ips.json");
        let mut registry = LoopbackRegistry::new();
        registry
            .allocate("project:demo:/tmp/missing-worker", "/tmp/missing-worker")
            .expect("qualified assignment");
        registry
            .allocate("demo", "/tmp/missing-worker")
            .expect("legacy assignment");
        registry
            .allocate("project:foreign:/tmp/foreign", "/tmp/foreign")
            .expect("foreign assignment");
        registry
            .allocate("shared:demo:/tmp/missing-worker", "/tmp/missing-worker")
            .expect("shared assignment");
        registry.save(&path).expect("save registry");

        let mut record = sample_record();
        record.project_names = vec!["demo".to_owned()];
        record.loopback_identities = vec!["project:demo:/tmp/missing-worker".to_owned()];
        let observed = observe_loopbacks(&record).expect("observe loopbacks");
        let observed_names = observed
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(observed_names.contains("project:demo:/tmp/missing-worker"));
        assert!(observed_names.contains("demo"));
        assert!(!observed_names.contains("project:foreign:/tmp/foreign"));

        delete_loopbacks(&record).expect("retire owned loopbacks");
        let remaining = LoopbackRegistry::load(&path).expect("reload registry");
        assert!(remaining.get("project:demo:/tmp/missing-worker").is_none());
        assert!(remaining.get("demo").is_none());
        assert!(remaining.get("project:foreign:/tmp/foreign").is_some());
        assert!(remaining.get("shared:demo:/tmp/missing-worker").is_some());
        let _ = std::fs::remove_dir_all(home);
    }
}

#[cfg(test)]
mod stopped_profile_tests {
    use super::runtime_stopped_message;
    use crate::runner::RunnerError;
    use effigy_containers::retire_report_unverified;

    #[test]
    fn stopped_colima_and_docker_daemon_are_stopped() {
        assert!(runtime_stopped_message(
            "stderr:\ntime level=fatal msg=\"colima [profile=effigy-release] is not running\""
        ));
        assert!(runtime_stopped_message(
            "Cannot connect to the Docker daemon at unix:///x"
        ));
    }

    #[test]
    fn other_failures_are_not_stopped() {
        assert!(!runtime_stopped_message("permission denied"));
    }

    #[test]
    fn json_failure_exposes_structured_report_in_error_details() {
        let stopped = vec!["stopped-profile".to_owned()];
        let report = retire_report_unverified(None, None, &[], &[], false, &stopped);
        let error = RunnerError::CommandJsonFailure {
            rendered: report.json.to_string(),
        };
        let details: serde_json::Value =
            serde_json::from_str(error.json_error_details().expect("details")).unwrap();
        assert_eq!(details["schema"], "effigy.container.retire.v1");
        assert_eq!(details["idempotent"], false);
        assert_eq!(details["unverified_profiles"][0], "stopped-profile");
    }
}
