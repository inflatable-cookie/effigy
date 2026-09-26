use std::path::{Path, PathBuf};

use effigy_catalog::volumes::{
    inspect_volumes_command, parse_inspect_volume_metadata_list, parse_listed_volume_names,
    remove_volume_command, DockerCommand,
};
use effigy_containers::{
    load_all_container_policies, load_container_policy, load_for_checkout, load_scope_record,
    plan_retirement, remaining_after, remove_scope_record, retire_report, ObservedKind,
    ObservedResource, ScopeComposeKind, ScopeRecord, PERSIST_LABEL, SCOPE_LABEL,
};
use effigy_core::worktree_scope;
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::ports::PortRegistry;
use effigy_gateway::registration::deregister_owned_routes_with;
use effigy_gateway::routes::RouteTableLock;

use super::data::maybe_confirm_destructive_container_action;
use super::hosts::{host_map_for_policy, shared_runtime_identity};
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
    name: Option<&str>,
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
    refresh_live_record(&checkout, name);
    load_for_checkout(&checkout).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn refresh_live_record(checkout: &Path, name: Option<&str>) {
    let policies = if name.is_some() {
        load_container_policy(checkout, name)
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        load_all_container_policies(checkout).unwrap_or_default()
    };
    let Ok(Some(token)) = worktree_scope::load_or_create(checkout) else {
        return;
    };
    for policy in policies {
        let Ok(host_map) = host_map_for_policy(&policy) else {
            continue;
        };
        let Some(key) = host_map.host_key.clone() else {
            continue;
        };
        let record = ScopeRecord::from_policy(
            &policy,
            &token,
            &key,
            &host_map,
            shared_runtime_identity(&policy, Some(&token)),
        );
        let _ = effigy_containers::upsert_scope_record(&record);
    }
}

fn retire_one_record(record: &ScopeRecord, output_json: bool) -> Result<String, RunnerError> {
    let observed = observe_scope(record)?;
    let plan = plan_retirement(record, &observed);
    if plan.skip_reason.is_some() {
        return Ok(render_container_report(
            retire_report(Some(record), Some(&plan), &[], &[], false),
            output_json,
        ));
    }
    let mut removed = Vec::new();
    let mut routes_cleared = false;
    let mut ports_cleared = false;
    let mut loopbacks_cleared = false;
    for resource in &plan.delete {
        let deleted = match resource.kind {
            ObservedKind::Route if routes_cleared => true,
            ObservedKind::Port if ports_cleared => true,
            ObservedKind::Loopback if loopbacks_cleared => true,
            ObservedKind::Route => {
                routes_cleared = true;
                delete_routes(record)?
            }
            ObservedKind::Port => {
                ports_cleared = true;
                delete_ports(record)?
            }
            ObservedKind::Loopback => {
                loopbacks_cleared = true;
                delete_loopbacks(record)?
            }
            ObservedKind::Container => delete_container(record, &resource.name)?,
            ObservedKind::Volume => delete_volume(record, &resource.name)?,
            ObservedKind::Network => delete_network(record, &resource.name)?,
        };
        if deleted {
            removed.push(resource.clone());
        }
    }
    let remaining = remaining_after(record, &observe_scope(record)?);
    let record_removed = remaining.is_empty();
    if record_removed {
        remove_scope_record(&record.token)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    let report = retire_report(
        Some(record),
        Some(&plan),
        &removed,
        &remaining,
        record_removed,
    );
    if remaining.is_empty() {
        Ok(render_container_report(report, output_json))
    } else {
        Err(RunnerError::task_invocation(report.success_text))
    }
}

fn observe_scope(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut observed = Vec::new();
    observed.extend(observe_labeled_containers(record)?);
    observed.extend(observe_networks(record)?);
    observed.extend(observe_volumes(record)?);
    observed.extend(observe_routes(record)?);
    observed.extend(observe_ports(record)?);
    observed.extend(observe_loopbacks(record)?);
    Ok(observed)
}

fn observe_labeled_containers(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    found.extend(list_named_resources(
        record,
        ObservedKind::Container,
        "ps",
        &[
            "-a".to_owned(),
            "--filter".to_owned(),
            format!("label={SCOPE_LABEL}={}", record.token),
            "--format".to_owned(),
            "{{.Names}}".to_owned(),
        ],
        "list containers by runtime scope label",
        Some(record.token.as_str()),
        record.project_names.first().cloned(),
    )?);
    for project in &record.project_names {
        found.extend(list_named_resources(
            record,
            ObservedKind::Container,
            "ps",
            &[
                "-a".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={project}"),
                "--format".to_owned(),
                "{{.Names}}".to_owned(),
            ],
            &format!("list containers for compose project {project}"),
            None,
            Some(project.clone()),
        )?);
    }
    Ok(unique_resources(found))
}

fn observe_networks(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    found.extend(list_named_resources(
        record,
        ObservedKind::Network,
        "network",
        &[
            "ls".to_owned(),
            "--filter".to_owned(),
            format!("label={SCOPE_LABEL}={}", record.token),
            "--format".to_owned(),
            "{{.Name}}".to_owned(),
        ],
        "list networks by runtime scope label",
        Some(record.token.as_str()),
        record.project_names.first().cloned(),
    )?);
    for project in &record.project_names {
        found.extend(list_named_resources(
            record,
            ObservedKind::Network,
            "network",
            &[
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={project}"),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            &format!("list networks for compose project {project}"),
            None,
            Some(project.clone()),
        )?);
    }
    found.retain(|resource| !is_builtin_network(&resource.name));
    Ok(unique_resources(found))
}

fn observe_volumes(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut names = Vec::new();
    names.extend(list_names(
        record,
        "volume",
        &[
            "ls".to_owned(),
            "--filter".to_owned(),
            format!("label={SCOPE_LABEL}={}", record.token),
            "--format".to_owned(),
            "{{.Name}}".to_owned(),
        ],
        "list volumes by runtime scope label",
    )?);
    for project in &record.project_names {
        names.extend(list_names(
            record,
            "volume",
            &[
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={project}"),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            &format!("list volumes for compose project {project}"),
        )?);
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let output = run_runtime_volume_capture(
        &observation_cwd(record),
        &record.profile,
        &inspect_volumes_command(&names),
    )?;
    Ok(
        parse_inspect_volume_metadata_list(&String::from_utf8_lossy(&output.stdout))
            .into_iter()
            .map(|metadata| {
                let persist = metadata
                    .labels
                    .get(PERSIST_LABEL)
                    .map(|value| value.eq_ignore_ascii_case("true"))
                    .unwrap_or_else(|| {
                        record
                            .retain_volumes
                            .iter()
                            .any(|name| name == &metadata.name)
                    });
                let external = metadata
                    .labels
                    .get("com.docker.compose.external")
                    .map(|value| value.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                ObservedResource {
                    kind: ObservedKind::Volume,
                    name: metadata.name,
                    scope_label: metadata.labels.get(SCOPE_LABEL).cloned(),
                    project_label: metadata
                        .labels
                        .get("com.effigy.project")
                        .cloned()
                        .or_else(|| metadata.labels.get("com.docker.compose.project").cloned()),
                    persist,
                    external,
                }
            })
            .collect(),
    )
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
    Ok(list_names(record, program, args, description)?
        .into_iter()
        .map(|name| ObservedResource {
            kind,
            name,
            scope_label: scope_label.map(str::to_owned),
            project_label: project_label.clone(),
            persist: false,
            external: false,
        })
        .collect())
}

fn list_names(
    record: &ScopeRecord,
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
    let output = run_runtime_volume_capture(&observation_cwd(record), &record.profile, &command)?;
    Ok(parse_listed_volume_names(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn unique_resources(mut found: Vec<ObservedResource>) -> Vec<ObservedResource> {
    found.sort_by(|left, right| left.kind.cmp(&right.kind).then(left.name.cmp(&right.name)));
    found.dedup_by(|left, right| left.kind == right.kind && left.name == right.name);
    found
}

fn is_builtin_network(name: &str) -> bool {
    matches!(name, "bridge" | "host" | "none")
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
    Ok(record
        .loopback_identities
        .iter()
        .filter(|identity| registry.get(identity).is_some())
        .map(|identity| ObservedResource {
            kind: ObservedKind::Loopback,
            name: identity.clone(),
            scope_label: Some(record.token.clone()),
            project_label: record.project_names.first().cloned(),
            persist: false,
            external: false,
        })
        .collect())
}

fn delete_container(record: &ScopeRecord, name: &str) -> Result<bool, RunnerError> {
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["rm".to_owned(), "-f".to_owned(), name.to_owned()],
        description: format!("remove owned container {name}"),
    };
    match run_runtime_volume_capture(&observation_cwd(record), &record.profile, &command) {
        Ok(output) => Ok(output.status.success()),
        Err(_) => Ok(false),
    }
}

fn delete_network(record: &ScopeRecord, name: &str) -> Result<bool, RunnerError> {
    if is_builtin_network(name) {
        return Ok(true);
    }
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["network".to_owned(), "rm".to_owned(), name.to_owned()],
        description: format!("remove owned network {name}"),
    };
    match run_runtime_volume_capture(&observation_cwd(record), &record.profile, &command) {
        Ok(output) => Ok(output.status.success()),
        Err(_) => Ok(false),
    }
}

fn delete_volume(record: &ScopeRecord, name: &str) -> Result<bool, RunnerError> {
    match run_runtime_volume_capture(
        &observation_cwd(record),
        &record.profile,
        &remove_volume_command(name),
    ) {
        Ok(output) => Ok(output.status.success()),
        Err(_) => Ok(false),
    }
}

fn delete_routes(record: &ScopeRecord) -> Result<bool, RunnerError> {
    let path = super::gateway_registration::gateway_route_table_path()?;
    deregister_owned_routes_with(&path, &record.checkout, Some(&record.token), |route| {
        if !route.tls {
            return Ok(());
        }
        remove_gateway_tls_cert(&route.domain).map_err(|error| {
            effigy_gateway::GatewayError::TlsError {
                domain: route.domain.clone(),
                reason: error.to_string(),
            }
        })
    })
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
    for identity in &record.loopback_identities {
        registry.deallocate(identity);
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
