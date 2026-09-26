use std::path::{Path, PathBuf};

use effigy_catalog::volumes::{parse_listed_volume_names, remove_volume_command, DockerCommand};
use effigy_containers::{
    load_container_policy, load_for_checkout, load_scope_record, plan_retirement, remaining_after,
    remove_scope_record, retire_report, ObservedKind, ObservedResource, ScopeComposeKind,
    ScopeRecord, SCOPE_LABEL,
};
use effigy_core::worktree_scope;
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::ports::PortRegistry;
use effigy_gateway::registration::deregister_owned_routes;
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
            "Remove owned containers, volumes, routes and ports for this runtime scope.",
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
    let Ok(policy) = load_container_policy(checkout, name) else {
        return;
    };
    let Ok(Some(token)) = worktree_scope::load_or_create(checkout) else {
        return;
    };
    let Ok(host_map) = host_map_for_policy(&policy) else {
        return;
    };
    let Some(key) = host_map.host_key.clone() else {
        return;
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
            ObservedKind::Network => true,
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
    observed.extend(observe_labeled_containers(record));
    observed.extend(observe_volumes(record)?);
    observed.extend(observe_routes(record)?);
    observed.extend(observe_ports(record)?);
    observed.extend(observe_loopbacks(record)?);
    Ok(observed)
}

fn observe_labeled_containers(record: &ScopeRecord) -> Vec<ObservedResource> {
    let cwd = observation_cwd(record);
    let mut found = Vec::new();
    for project in &record.project_names {
        let command = DockerCommand {
            program: "docker".to_owned(),
            args: vec![
                "ps".to_owned(),
                "-a".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={project}"),
                "--format".to_owned(),
                "{{.Names}}".to_owned(),
            ],
            description: format!("list containers for compose project {project}"),
        };
        let Ok(output) = run_runtime_volume_capture(&cwd, &record.profile, &command) else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        found.extend(
            parse_listed_volume_names(&String::from_utf8_lossy(&output.stdout))
                .into_iter()
                .map(|name| ObservedResource {
                    kind: ObservedKind::Container,
                    name,
                    scope_label: None,
                    project_label: Some(project.clone()),
                    persist: false,
                    external: false,
                }),
        );
    }
    found
}

fn observe_volumes(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let cwd = observation_cwd(record);
    let labeled = DockerCommand {
        program: "docker".to_owned(),
        args: vec![
            "volume".to_owned(),
            "ls".to_owned(),
            "--filter".to_owned(),
            format!("label={SCOPE_LABEL}={}", record.token),
            "--format".to_owned(),
            "{{.Name}}".to_owned(),
        ],
        description: "list volumes by runtime scope label".to_owned(),
    };
    if let Ok(output) = run_runtime_volume_capture(&cwd, &record.profile, &labeled) {
        if output.status.success() {
            let names = parse_listed_volume_names(&String::from_utf8_lossy(&output.stdout));
            if !names.is_empty() {
                return Ok(names
                    .into_iter()
                    .map(|name| ObservedResource {
                        kind: ObservedKind::Volume,
                        name,
                        scope_label: Some(record.token.clone()),
                        project_label: record.project_names.first().cloned(),
                        persist: true,
                        external: false,
                    })
                    .collect());
            }
        }
    }
    let mut found = Vec::new();
    for project in &record.project_names {
        let command = DockerCommand {
            program: "docker".to_owned(),
            args: vec![
                "volume".to_owned(),
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label=com.docker.compose.project={project}"),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            description: format!("list volumes for compose project {project}"),
        };
        let Ok(output) = run_runtime_volume_capture(&cwd, &record.profile, &command) else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        found.extend(
            parse_listed_volume_names(&String::from_utf8_lossy(&output.stdout))
                .into_iter()
                .map(|name| ObservedResource {
                    kind: ObservedKind::Volume,
                    name,
                    scope_label: None,
                    project_label: Some(project.clone()),
                    persist: true,
                    external: false,
                }),
        );
    }
    Ok(found)
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
    let removed = deregister_owned_routes(&path, &record.checkout, Some(&record.token))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    for route in &removed {
        if route.tls {
            remove_gateway_tls_cert(&route.domain)?;
        }
    }
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
