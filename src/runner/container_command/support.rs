use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_catalog::volumes::{
    remove_volume_command, DockerCommand, VolumeClassification, VolumeIdentity,
};
use effigy_containers::backend::endpoint_backend;
use effigy_containers::{
    exec::{
        list_running_compose_containers_for_policy, ContainerExecError, RunningComposeContainer,
    },
    health::wait_for_ready,
    EffectiveContainerPolicy,
};
use effigy_containers::{
    BackendId, ContainerAction, ContainerBackendDetection, ContainerInterruptPolicy,
    ContainerManager, ContainerManagerRequest,
};
use effigy_core::repo_markers::has_task_manifest;
use effigy_core::shell::shell_quote;
use serde_json::json;

use super::gateway_registration::{
    resolve_gateway_tcp_alias_routes_for_container, RegisteredGatewayRoute,
};
use super::RunnerError;
use crate::runner::command_context::{active_invocation_cwd, resolve_active_command_context};

pub(super) enum ContainerRepoScope {
    RepoRoot(PathBuf),
    InvocationCwd(PathBuf),
}

pub(super) fn resolve_repo_root_or_invocation_cwd_scope(
    repo_override: Option<PathBuf>,
) -> Result<ContainerRepoScope, RunnerError> {
    match resolve_active_command_context(repo_override.clone()) {
        Ok(context) if repo_root_has_effigy_manifest(&context.resolved.resolved_root) => Ok(
            ContainerRepoScope::RepoRoot(context.resolved.resolved_root.clone()),
        ),
        Ok(_) if repo_override.is_none() => {
            Ok(ContainerRepoScope::InvocationCwd(active_invocation_cwd()?))
        }
        Ok(context) => Err(RunnerError::task_invocation(format!(
            "`--repo {}` does not point to an Effigy repo",
            context.resolved.resolved_root.display()
        ))),
        Err(RunnerError::Resolve(_)) if repo_override.is_none() => {
            Ok(ContainerRepoScope::InvocationCwd(active_invocation_cwd()?))
        }
        Err(error) => Err(error),
    }
}

pub(super) fn repo_root_has_effigy_manifest(repo_root: &Path) -> bool {
    has_task_manifest(repo_root)
}

/// Wait until the environment is ready, naming each service as it waits.
///
/// Every service's own health check decides first (effigy#313), because that
/// is what tells a person which service is wrong. The environment's declared
/// `[containers.<name>.health]` check is still the final gate when it declares
/// one; an environment whose services the runtime cannot see keeps exactly
/// the old behaviour.
pub(super) fn wait_for_container_ready(
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<&'static str>, RunnerError> {
    match super::readiness::await_services_ready(&policy.repo_root, policy, stop_requested)? {
        super::readiness::ReadinessEnd::Interrupted => return Ok(Some("interrupted")),
        super::readiness::ReadinessEnd::Ready | super::readiness::ReadinessEnd::NothingToAwait => {}
    }
    wait_for_ready(
        &policy.name,
        policy.health_check.as_deref(),
        policy.health_timeout_secs,
        stop_requested,
    )
    .map_err(RunnerError::task_invocation)
}

pub(in crate::runner) fn validate_running_container_runtime_match(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let rows = match list_running_compose_containers_for_policy(repo_root, policy) {
        Ok(rows) => rows,
        Err(error) if exec_error_means_runtime_not_running(&error) => return Ok(()),
        Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
    };
    if let Some(message) = running_container_runtime_mismatch(repo_root, policy, &rows) {
        return Err(RunnerError::task_invocation(message));
    }
    Ok(())
}

fn exec_error_means_runtime_not_running(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => {
            runtime_probe_means_runtime_not_running("", stderr)
        }
        ContainerExecError::Launch { .. } => false,
    }
}

pub(super) fn runtime_probe_means_runtime_not_running(stdout: &str, stderr: &str) -> bool {
    stderr.contains("level=fatal msg=\"colima is not running\"")
        || (stderr.contains("level=fatal msg=\"colima [profile=")
            && stderr.contains("] is not running\""))
        || stderr.contains("Cannot connect to the Docker daemon")
        || stderr.contains("daemon is not running")
        || stdout.contains("Cannot connect to the Docker daemon")
        || stdout.contains("daemon is not running")
}

pub(super) fn running_container_runtime_mismatch(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Option<String> {
    let repo_rows = rows
        .iter()
        .filter(|row| {
            row.working_dir.as_deref().is_some_and(|working_dir| {
                effigy_runtime::read::working_dir_belongs_to_repo(working_dir, repo_root)
            })
        })
        .collect::<Vec<_>>();
    if repo_rows.is_empty() {
        return None;
    }

    let expected_project_running = repo_rows.iter().any(|row| {
        row.project_name.as_deref() == Some(policy.project_name.as_str())
            && row.service.as_deref() == Some(policy.primary_service.as_str())
    });
    if expected_project_running {
        return None;
    }

    let mut stale_projects = repo_rows
        .iter()
        .filter(|row| row.service.as_deref() == Some(policy.primary_service.as_str()))
        .filter_map(|row| row.project_name.as_deref())
        .filter(|project_name| *project_name != policy.project_name)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    stale_projects.sort();
    stale_projects.dedup();
    if stale_projects.is_empty() {
        return None;
    }

    let stale = stale_projects
        .iter()
        .map(|project| format!("`{project}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "container `{}` expects Compose project `{}`, but this repo still has running `{}` service container(s) under {}. `project_name` changed while the old runtime was still up; stop the stale project first, then start the current one again.",
        policy.name, policy.project_name, policy.primary_service, stale
    ))
}

pub(super) fn annotate_registered_gateway_routes(
    report: &mut effigy_containers::ContainerCommandReport,
    routes: &[RegisteredGatewayRoute],
) {
    if routes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            json!(routes
                .iter()
                .map(|route| json!({
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

pub(super) fn install_primary_service_tcp_alias_hosts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<Vec<String>, RunnerError> {
    let pairs = tcp_alias_host_pairs(policy, routes);
    if pairs.is_empty() {
        return Ok(Vec::new());
    }

    let script = render_tcp_alias_hosts_script(&pairs);
    let tail = vec![
        "exec",
        "-T",
        "--user",
        "root",
        policy.primary_service.as_str(),
        "sh",
        "-lc",
        script.as_str(),
    ];
    let plan = effigy_runtime::container_manager::compose_invocation_plan(
        repo_root,
        policy,
        tail,
        ContainerAction::Exec,
        "docker compose exec tcp alias hosts",
    )
    .map_err(RunnerError::from)?;
    let output = crate::runner::exec_command::run_verified_compose_exec_plan(
        policy, &plan, binding, true, None, None,
    )?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(
            "verified TCP alias installation failed",
        ));
    }

    Ok(pairs
        .into_iter()
        .map(|(domain, host)| format!("{domain} -> {host}"))
        .collect())
}

pub(in crate::runner) fn reconcile_primary_service_tcp_alias_hosts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<Vec<String>, RunnerError> {
    let routes = resolve_gateway_tcp_alias_routes_for_container(repo_root, policy)?;
    install_primary_service_tcp_alias_hosts(repo_root, policy, &routes, binding)
}

fn tcp_alias_host_pairs(
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Vec<(String, String)> {
    let shared_hosts = policy
        .shared_services
        .iter()
        .map(|shared| (shared.service_name.as_str(), shared.host.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut pairs = std::collections::BTreeSet::new();
    for route in routes {
        let Some(service) = route.service.as_deref() else {
            continue;
        };
        if route.tcp_port.is_none() {
            continue;
        }
        let host = shared_hosts.get(service).copied().unwrap_or(service);
        pairs.insert((route.domain.clone(), host.to_owned()));
    }
    pairs.into_iter().collect()
}

fn render_tcp_alias_hosts_script(pairs: &[(String, String)]) -> String {
    // This script runs inside the service container, so its `mktemp` writes to
    // the guest's own temp directory; it deliberately leaves TMPDIR alone.
    let mut script = String::from(
        r#"set -eu
patch_alias() {
  alias="$1"
  service="$2"
  ip="$(getent hosts "$service" | awk 'NR == 1 { print $1 }')"
  if [ -z "$ip" ]; then
    printf '[effigy] could not resolve service `%s` for alias `%s`\n' "$service" "$alias" >&2
    return 0
  fi
  tmp="$(mktemp)"
  awk -v alias="$alias" '{
    keep=1
    for (i = 2; i <= NF; i++) {
      if ($i == alias) {
        keep=0
      }
    }
    if (keep) {
      print
    }
  }' /etc/hosts > "$tmp"
  cat "$tmp" > /etc/hosts
  rm -f "$tmp"
  printf '%s %s\n' "$ip" "$alias" >> /etc/hosts
}
"#,
    );
    for (domain, service) in pairs {
        script.push_str("patch_alias ");
        script.push_str(&shell_quote(domain));
        script.push(' ');
        script.push_str(&shell_quote(service));
        script.push('\n');
    }
    script
}

/// Refuse a manifest-pinned host port another process already holds, naming
/// the holder. A port this environment's own containers publish is not a
/// conflict: Compose reuses its own binding.
pub(super) fn ensure_pinned_host_ports_available(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.declared_ports.is_empty() {
        return Ok(());
    }
    let owned =
        effigy_containers::exec::list_running_compose_containers_for_policy(repo_root, policy)
            .map(|rows| {
                rows.iter()
                    .flat_map(|row| row.ports.iter())
                    .filter_map(|published| published.split_once("->").map(|(host, _)| host))
                    .filter_map(|host| host.rsplit(':').next())
                    .filter_map(|port| port.trim().parse::<u16>().ok())
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
    match effigy_containers::policy::ports::pinned_host_port_conflict(
        &policy.name,
        &policy.declared_ports,
        &owned,
    ) {
        Some(message) => Err(RunnerError::task_invocation(message)),
        None => Ok(()),
    }
}

/// Shared services come up through the same one apply as the environment's
/// own stack: `compose up -d --remove-orphans`, with failures propagated.
///
/// A shared service belongs to no checkout, so its ensure first takes the one
/// machine-wide gate for that Compose project: two checkouts never interleave
/// a replacement (effigy#377). When the runtime reports a container from a
/// different render than the one about to be applied, that container is
/// stopped and removed before Compose creates its replacement, so a published
/// host port is never held twice (`port is already allocated`). Its named data
/// volume is untouched. An unchanged render is left exactly where it is.
pub(in crate::runner) fn ensure_shared_services_running(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let mut notes = Vec::new();
    for service in &policy.shared_services {
        // Held until this service's ensure is complete; a gate that cannot be
        // taken is named and the ensure proceeds, because the service still
        // has to run.
        let _gate = match crate::runner::task_engine::shared_service_gate(&service.project_name) {
            Ok(gate) => Some(gate),
            Err(error) => {
                eprintln!(
                    "[warn] shared {} ran without its machine gate: {error}",
                    service.service_name
                );
                None
            }
        };
        let replaced = replace_stale_shared_render(policy, service)?;
        let workdir = service.compose_file.parent().ok_or_else(|| {
            RunnerError::task_invocation("shared service compose file has no parent directory")
        })?;
        run_shared_compose_capture(
            workdir,
            policy,
            &shared_compose_args(service, ["up", "-d", "--remove-orphans"]),
            &format!("docker compose up (shared {})", service.service_name),
        )?;
        // The shared stack is started, so it gets the restart policy here too
        // (effigy#383).
        effigy_containers::policy::restart::apply_after_start(
            &policy.repo_root,
            policy,
            &service.project_name,
            std::slice::from_ref(&service.compose_file),
        );
        notes.push(match replaced {
            true => format!("replaced shared {} (render changed)", service.service_name),
            false => format!(
                "{} [{}] -> {}:{}",
                service.service_name, service.catalog, service.host, service.host_port
            ),
        });
    }
    Ok(notes)
}

/// Stop and remove every container of a shared project the runtime says came
/// from a different render, so Compose creates the new one against a free
/// port. An unreadable runtime is named and left to Compose rather than
/// stopping a service this run cannot see.
fn replace_stale_shared_render(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
) -> Result<bool, RunnerError> {
    let containers = match effigy_containers::exec::shared_project_containers(
        &policy.repo_root,
        policy,
        &service.project_name,
    ) {
        Ok(containers) => containers,
        Err(error) => {
            eprintln!(
                "[warn] cannot read the runtime's shared {} containers: {error}",
                service.service_name
            );
            return Ok(false);
        }
    };
    let stale = containers
        .iter()
        .filter(|container| container.render.as_deref() != Some(service.render_digest.as_str()))
        .collect::<Vec<_>>();
    if stale.is_empty() {
        return Ok(false);
    }
    for container in stale {
        stop_and_remove_container(
            &policy.repo_root,
            &policy.profile,
            &container.container_name,
        )
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot replace shared {}: {error}",
                service.service_name
            ))
        })?;
    }
    Ok(true)
}

/// Stop a container, then remove it (effigy#376). A forced remove of a running
/// container is a kill the runtime's restart manager undoes for the
/// `restart: unless-stopped` policy `up` asked the runtime for; an explicit stop is
/// recorded as a user stop, so the removal holds. A stop that finds nothing
/// running is not a failure.
pub(super) fn stop_and_remove_container(
    cwd: &Path,
    profile: &str,
    name: &str,
) -> Result<(), RunnerError> {
    let stop = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["stop".to_owned(), name.to_owned()],
        description: format!("stop owned container {name}"),
    };
    let _ = run_runtime_volume_capture(cwd, profile, &stop);
    let remove = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["rm".to_owned(), "-f".to_owned(), name.to_owned()],
        description: format!("remove owned container {name}"),
    };
    run_runtime_volume_capture(cwd, profile, &remove).map(|_| ())
}

/// Remove only volumes whose labels prove this environment owns them, on its
/// exact backend. A same-named lookalike is reported and left alone (R3).
pub(super) fn remove_reset_volumes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    classification: &VolumeClassification,
) -> Result<(), RunnerError> {
    let endpoint = effigy_containers::backend::runtime_endpoint(repo_root, policy)?;
    let identities = effigy_runtime::data::volume_identities(
        repo_root,
        &endpoint,
        &policy.project_name,
        &classification.remove,
        &|cwd: &Path, endpoint: &str, command: &DockerCommand| {
            run_runtime_volume_capture(cwd, endpoint, command)
                .map_err(super::runtime_error_from_runner)
        },
    )
    .map_err(RunnerError::from)?;
    for name in &classification.remove {
        match identities.get(name) {
            Some(VolumeIdentity::Owned) => {
                run_runtime_volume_capture(repo_root, &endpoint, &remove_volume_command(name))?;
            }
            Some(VolumeIdentity::Lookalike(reason)) => {
                eprintln!(
                    "[warn] reset left {reason}; it was not created for `{}`",
                    policy.name
                );
            }
            Some(VolumeIdentity::Absent) | None => {}
        }
    }
    Ok(())
}

pub(super) fn annotate_shared_service_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "ensured",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        match note.starts_with("replaced shared ") {
            true => report.success_text.push_str(&format!("[shared] {note}")),
            false => report
                .success_text
                .push_str(&format!("[shared] ensured {note}")),
        }
    }
}

/// Surface `data_from` handover lines in the detached up report: one text
/// line per handover plus a machine-readable `data_handover` list.
pub(super) fn annotate_data_handover_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("data_handover".to_owned(), json!(notes));
    }
    for note in notes {
        report.success_text.push('\n');
        report.success_text.push_str(note);
    }
}

/// Bundle or catalog sources this Prepare refreshed, in the JSON envelope's
/// `notes` (contract 020). The one line per source is already on stderr.
pub(super) fn annotate_source_update_notes(report: &mut effigy_containers::ContainerCommandReport) {
    let notes = effigy_manifest::bundle_update_notes();
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("notes".to_owned(), json!(notes));
    }
}

pub(super) fn annotate_tcp_alias_host_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "tcp_alias_hosts".to_owned(),
            json!({
                "action": "installed",
                "aliases": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[gateway] installed container TCP alias {note}"));
    }
}

pub(super) fn annotate_warning_lines(
    report: &mut effigy_containers::ContainerCommandReport,
    warnings: &[String],
) {
    if warnings.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), json!(warnings));
    }
    for warning in warnings {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[warn] {warning}"));
    }
}

#[cfg(test)]
pub(super) fn annotate_left_running_shared_services(
    report: &mut effigy_containers::ContainerCommandReport,
    policy: &EffectiveContainerPolicy,
) {
    if policy.shared_services.is_empty() {
        return;
    }
    let services = policy
        .shared_services
        .iter()
        .map(|service| {
            format!(
                "{} [{}] -> {}:{}",
                service.service_name, service.catalog, service.host, service.host_port
            )
        })
        .collect::<Vec<_>>();
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "left-running",
                "services": services,
            }),
        );
    }
    for service in services {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[shared] left running {service}"));
    }
}

fn shared_compose_args<'a>(
    service: &effigy_containers::SharedServiceBinding,
    tail: impl IntoIterator<Item = &'a str>,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("compose"),
        OsString::from("-f"),
        service.compose_file.as_os_str().to_os_string(),
        OsString::from("-p"),
        OsString::from(service.project_name.as_str()),
    ];
    args.extend(tail.into_iter().map(OsString::from));
    args
}

/// Shared services start on the dependent environment's exact backend, never
/// a hard-coded engine. They run from the shared render's own machine-wide
/// directory, so the invocation names no checkout (effigy#377).
fn run_shared_compose_capture(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let backend =
        effigy_containers::backend::exact_backend(&policy.repo_root, &policy.name, policy.driver)
            .id;
    let profile = policy.profile.as_str();
    let request = ContainerManagerRequest::new(&policy.repo_root)
        .backend_override(backend.clone())
        .interrupt_policy(ContainerInterruptPolicy::Forward);
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let mut plan = ContainerManager::defaults()
        .compose_invocation_plan(
            &request,
            &detection,
            profile,
            args,
            ContainerAction::Activate,
            label,
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    // A VM-hosted backend cannot inherit the host environment, so the shared
    // transform names the Compose interpolation file its VM can read.
    let resolved_args = effigy_containers::policy::render::compose_invocation_args(
        plan.program.as_ref(),
        &plan.args,
        &[],
    )
    .map_err(|error| RunnerError::TaskCommandLaunch {
        command: format!(
            "{label} ({} {})",
            plan.program.to_string_lossy(),
            format_args(&plan.args)
        ),
        error,
    })?;
    plan.args = resolved_args;
    let mut command = std::process::Command::new(&plan.program);
    command.current_dir(cwd).args(&plan.args);
    command
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{label} ({} {})",
                plan.program.to_string_lossy(),
                format_args(&plan.args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{label} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

pub(super) fn run_runtime_volume_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    if command.program == "__effigy_volume_usage" {
        return run_runtime_volume_usage_capture(repo_root, profile, command);
    }
    if command.program == "__effigy_volume_usage_batch" {
        return run_runtime_volume_usage_batch_capture(repo_root, profile, command);
    }
    if command.program == "__effigy_volume_cache_kind_batch" {
        return run_runtime_volume_cache_kind_batch_capture(repo_root, profile, command);
    }
    let plan = runtime_volume_plan(repo_root, profile, command)?;
    std::process::Command::new(&plan.program)
        .current_dir(repo_root)
        .args(&plan.args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                command.description,
                plan.program.to_string_lossy(),
                format_args(&plan.args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

/// Resolve one exact backend command without executing it, for kernel custody.
pub(super) fn runtime_volume_plan(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<effigy_containers::ContainerRuntimeInvocationPlan, RunnerError> {
    let docker_args = runtime_args(&command.args);
    let request =
        ContainerManagerRequest::new(repo_root).interrupt_policy(ContainerInterruptPolicy::Forward);
    // The endpoint carries the exact backend; see `endpoint_backend`.
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(endpoint_backend(profile));
    ContainerManager::defaults()
        .runtime_invocation_plan(
            &request,
            &detection,
            effigy_containers::RuntimeInvocation {
                profile,
                docker_program: command.program.as_str().into(),
                docker_args: &docker_args,
                action: ContainerAction::Shutdown,
                label: command.description.clone(),
            },
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn run_runtime_volume_usage_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    let Some(mount_point) = command.args.first() else {
        return Err(RunnerError::task_invocation(
            "runtime volume usage command requires one mount-point argument",
        ));
    };
    let backend_id = endpoint_backend(profile);
    let (program, args) = if backend_id == BackendId::docker_compose() {
        (
            OsString::from("du"),
            vec![OsString::from("-sk"), OsString::from(mount_point)],
        )
    } else {
        (
            effigy_containers::compose::resolve_host_cli_program("colima"),
            vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from(profile),
                OsString::from("--"),
                OsString::from("sudo"),
                OsString::from("du"),
                OsString::from("-sk"),
                OsString::from(mount_point),
            ],
        )
    };

    std::process::Command::new(&program)
        .current_dir(repo_root)
        // Run tokens stay out of privileged helper commands.
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                command.description,
                program.to_string_lossy(),
                format_args(&args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn run_runtime_volume_usage_batch_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    if command.args.is_empty() {
        return Err(RunnerError::task_invocation(
            "runtime volume usage batch command requires one or more mount-point arguments",
        ));
    }
    let backend_id = endpoint_backend(profile);
    let (program, args) = if backend_id == BackendId::docker_compose() {
        let mut args = vec![OsString::from("-sk")];
        args.extend(command.args.iter().map(OsString::from));
        (OsString::from("du"), args)
    } else {
        let mut args = vec![
            OsString::from("ssh"),
            OsString::from("--profile"),
            OsString::from(profile),
            OsString::from("--"),
            OsString::from("sudo"),
            OsString::from("du"),
            OsString::from("-sk"),
        ];
        args.extend(command.args.iter().map(OsString::from));
        (
            effigy_containers::compose::resolve_host_cli_program("colima"),
            args,
        )
    };

    std::process::Command::new(&program)
        .current_dir(repo_root)
        // Run tokens stay out of privileged helper commands.
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                command.description,
                program.to_string_lossy(),
                format_args(&args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn run_runtime_volume_cache_kind_batch_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    if command.args.is_empty() {
        return Err(RunnerError::task_invocation(
            "runtime volume cache-kind batch command requires one or more mount-point arguments",
        ));
    }
    let script = render_volume_cache_kind_script(&command.args);
    let backend_id = endpoint_backend(profile);
    let (program, args) = if backend_id == BackendId::docker_compose() {
        (
            OsString::from("sh"),
            vec![OsString::from("-lc"), OsString::from(script)],
        )
    } else {
        (
            effigy_containers::compose::resolve_host_cli_program("colima"),
            vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from(profile),
                OsString::from("--"),
                OsString::from("sudo"),
                OsString::from("sh"),
                OsString::from("-lc"),
                OsString::from(script),
            ],
        )
    };

    std::process::Command::new(&program)
        .current_dir(repo_root)
        // Run tokens stay out of privileged helper commands.
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                command.description,
                program.to_string_lossy(),
                format_args(&args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn render_volume_cache_kind_script(mount_points: &[String]) -> String {
    let mut script = String::from(
        r#"set -eu
classify_cache_volume() {
  path="$1"
  kind=""
  if [ -d "$path/debug/deps" ] || [ -d "$path/release/deps" ] || [ -f "$path/.rustc_info.json" ]; then
    kind="rust-target"
  fi
  if [ -n "$kind" ]; then
    printf '%s\t%s\n' "$path" "$kind"
  fi
}
"#,
    );
    for mount_point in mount_points {
        script.push_str("classify_cache_volume ");
        script.push_str(&shell_quote(mount_point));
        script.push('\n');
    }
    script
}

fn runtime_args(args: &[String]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        render_tcp_alias_hosts_script, running_container_runtime_mismatch, tcp_alias_host_pairs,
    };
    use effigy_containers::{
        exec::RunningComposeContainer, EffectiveComposeSource, EffectiveContainerPolicy,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn shared_compose_receives_its_immutable_generation_root() {
        use crate::contract_test_support::EnvGuard;
        use std::os::unix::fs::PermissionsExt;
        let fixture =
            effigy_test_support::Fixture::workspace("effigy-parent-fixtures-shared-render-")
                .unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let generation = root.join("objects").join("a".repeat(64));
        let directory = generation.join("shared");
        std::fs::create_dir_all(&directory).unwrap();
        let compose = directory.join("compose.yml");
        std::fs::write(&compose, "services: {}\n").unwrap();
        std::fs::write(directory.join("config.txt"), "shared asset").unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        // The invocation names the generation's own Compose file and carries no
        // Effigy interpolation value: the file reaches its assets relatively.
        let script = "#!/bin/sh\ncase \"$*\" in *EFFIGY_RENDER_ROOT*) exit 76;; esac\n[ -z \"$EFFIGY_RENDER_ROOT\" ] || exit 77\n[ -r \"$EFFIGY_TEST_SHARED_ASSET\" ] || exit 78\nprintf 'shared asset available\\n'\n";
        for name in ["docker", "colima"] {
            let executable = bin.join(name);
            std::fs::write(&executable, script).unwrap();
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let _env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("HOME", Some(root.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
            ("EFFIGY_RENDER_ROOT", None),
            (
                "EFFIGY_TEST_SHARED_ASSET",
                Some(directory.join("config.txt").display().to_string()),
            ),
        ]);
        let args = vec![
            "compose".into(),
            "-f".into(),
            compose.into_os_string(),
            "up".into(),
            "-d".into(),
        ];
        let mut policy = test_policy();
        policy.repo_root = root.clone();
        let output =
            super::run_shared_compose_capture(&root, &policy, &args, "shared render proof")
                .unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "shared asset available"
        );
    }

    #[test]
    fn reset_removes_only_labelled_volumes_on_the_exact_backend() {
        use crate::contract_test_support::EnvGuard;
        use std::os::unix::fs::PermissionsExt;
        let _lock = crate::contract_test_support::lock_test();
        let fixture = effigy_test_support::Fixture::workspace("effigy-reset-adoption-").unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let log = root.join("calls");
        let owned = r#"{"Name":"demo-web-renamed-data","Labels":{"com.effigy.managed":"true","com.effigy.project":"demo-web-renamed","com.effigy.volume-name":"demo-web-renamed-data"}}"#;
        // Same name pattern, created outside Effigy: a lookalike.
        let lookalike = r#"{"Name":"demo-web-renamed-cache","Labels":{"com.docker.compose.project":"demo-web-renamed"}}"#;
        let docker = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$1 $2\" in\n'volume ls') printf 'demo-web-renamed-data\\tlocal\\t\\ndemo-web-renamed-cache\\tlocal\\t\\n' ;;\n'volume inspect') printf '[{owned},{lookalike}]' ;;\n'volume rm') ;;\n*) exit 70 ;;\nesac\n",
            log = log.display(),
        );
        for (name, script) in [
            ("docker", docker.as_str()),
            ("colima", "#!/bin/sh\nexit 99\n"),
        ] {
            let executable = bin.join(name);
            std::fs::write(&executable, script).unwrap();
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let _env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("HOME", Some(root.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
        ]);
        let mut policy = test_policy();
        policy.repo_root = root.clone();
        let classification = effigy_catalog::volumes::VolumeClassification {
            remove: vec![
                "demo-web-renamed-data".to_owned(),
                "demo-web-renamed-cache".to_owned(),
                "demo-web-renamed-missing".to_owned(),
            ],
            keep: vec![],
        };
        super::remove_reset_volumes(&root, &policy, &classification).unwrap();
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.contains("volume rm demo-web-renamed-data"), "{calls}");
        assert!(
            !calls.contains("volume rm demo-web-renamed-cache"),
            "{calls}"
        );
        assert!(
            !calls.contains("volume rm demo-web-renamed-missing"),
            "{calls}"
        );
        assert!(
            calls.contains("volume inspect demo-web-renamed-data demo-web-renamed-cache"),
            "{calls}"
        );
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            render: None,
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services: vec![],
            project_name: "demo-web-renamed".to_owned(),
            primary_service: "app".to_owned(),
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
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    fn test_policy_with_shared_services(
        shared_services: Vec<effigy_containers::SharedServiceBinding>,
    ) -> EffectiveContainerPolicy {
        let mut policy = test_policy();
        policy.shared_services = shared_services;
        policy
    }

    #[test]
    fn runtime_mismatch_detects_stale_project_name_for_same_repo_service() {
        let mismatch = running_container_runtime_mismatch(
            Path::new("/tmp/demo"),
            &test_policy(),
            &[RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "Up 2 minutes".to_owned(),
                ports: vec![],
                project_name: Some("demo-web-old".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("app".to_owned()),
                oneoff: false,
            }],
        )
        .expect("mismatch");

        assert!(mismatch.contains("expects Compose project `demo-web-renamed`"));
        assert!(mismatch.contains("under `demo-web-old`"));
    }

    #[test]
    fn runtime_mismatch_ignores_matching_project_name() {
        let mismatch = running_container_runtime_mismatch(
            Path::new("/tmp/demo"),
            &test_policy(),
            &[RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "Up 2 minutes".to_owned(),
                ports: vec![],
                project_name: Some("demo-web-renamed".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("app".to_owned()),
                oneoff: false,
            }],
        );

        assert!(mismatch.is_none());
    }
    #[test]
    fn tcp_alias_host_pairs_use_gateway_tcp_routes_only() {
        let policy = test_policy();
        let routes = vec![
            super::RegisteredGatewayRoute {
                domain: "api.demo.test".to_owned(),
                target: Some("127.0.0.1:19900".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                service: Some("workspace".to_owned()),
                external_target: false,
            },
            super::RegisteredGatewayRoute {
                domain: "db.demo.test".to_owned(),
                target: None,
                dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
                tcp_port: Some(5432),
                tcp_target: Some("127.0.0.1:19932".to_owned()),
                tls: false,
                service: Some("postgres".to_owned()),
                external_target: false,
            },
        ];

        assert_eq!(
            tcp_alias_host_pairs(&policy, &routes),
            vec![("db.demo.test".to_owned(), "postgres".to_owned())]
        );
    }

    #[test]
    fn tcp_alias_host_pairs_rewrite_shared_service_targets_to_shared_host() {
        let policy =
            test_policy_with_shared_services(vec![effigy_containers::SharedServiceBinding {
                service_name: "db".to_owned(),
                catalog: "mariadb".to_owned(),
                domain_label: "mysql".to_owned(),
                project_name: "effigy-shared-mariadb-deadbeef".to_owned(),
                compose_file: PathBuf::from("/tmp/shared-db/docker-compose.yml"),
                host: "host.docker.internal".to_owned(),
                host_port: 23306,
                container_port: 3306,
                host_env_vars: Vec::new(),
                port_env_vars: Vec::new(),
                render_digest: "shared-render".to_owned(),
            }]);
        let routes = vec![super::RegisteredGatewayRoute {
            domain: "db.demo.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 7)),
            tcp_port: Some(3306),
            tcp_target: Some("127.0.0.1:23306".to_owned()),
            tls: false,
            service: Some("db".to_owned()),
            external_target: false,
        }];

        assert_eq!(
            tcp_alias_host_pairs(&policy, &routes),
            vec![("db.demo.test".to_owned(), "host.docker.internal".to_owned())]
        );
    }

    #[test]
    fn tcp_alias_hosts_script_rewrites_existing_alias_entries() {
        let script =
            render_tcp_alias_hosts_script(&[("db.demo.test".to_owned(), "postgres".to_owned())]);

        assert!(script.contains("patch_alias 'db.demo.test' 'postgres'"));
        assert!(script.contains("return 0"));
        assert!(script.contains("awk -v alias=\"$alias\""));
        assert!(script.contains("printf '%s %s\\n' \"$ip\" \"$alias\" >> /etc/hosts"));
    }

    struct SharedFixture {
        _fixture: effigy_test_support::Fixture,
        root: PathBuf,
    }

    impl SharedFixture {
        /// A private checkout whose runtime is a script recording every call,
        /// listing one container of `project` rendered from `container_render`.
        fn new(name: &str, project: &str, container: &str, container_render: &str) -> Self {
            let fixture = effigy_test_support::Fixture::workspace(name).unwrap();
            let root = fixture.path().canonicalize().unwrap();
            let bin = root.join("bin");
            std::fs::create_dir(&bin).unwrap();
            let log = root.join("calls");
            let script = format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  *'--format'*)\n    printf '%s\\t%s\\t%s\\t%s\\n' '{container}' 'Up 2 minutes' '{container_render}' '{project}'\n    ;;\nesac\n",
                log = log.display(),
                container = container,
                container_render = container_render,
                project = project,
            );
            for name in ["docker", "colima"] {
                let executable = bin.join(name);
                std::fs::write(&executable, &script).unwrap();
                std::fs::set_permissions(
                    executable,
                    std::os::unix::fs::PermissionsExt::from_mode(0o700),
                )
                .unwrap();
            }
            std::fs::write(root.join("effigy.toml"), "[tasks]\nbuild = \"true\"\n").unwrap();
            let shared = root.join("shared");
            std::fs::create_dir_all(&shared).unwrap();
            std::fs::write(shared.join("compose.yml"), "services: {}\n").unwrap();
            Self {
                _fixture: fixture,
                root,
            }
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.root.join("calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn policy(&self, project: &str, render_digest: &str) -> EffectiveContainerPolicy {
            let mut policy = test_policy();
            policy.repo_root = self.root.clone();
            policy.shared_services = vec![effigy_containers::SharedServiceBinding {
                service_name: "legacy_mysql".to_owned(),
                catalog: "mariadb".to_owned(),
                domain_label: "mysql".to_owned(),
                project_name: project.to_owned(),
                compose_file: self.root.join("shared/compose.yml"),
                host: "host.docker.internal".to_owned(),
                host_port: 23306,
                container_port: 3306,
                host_env_vars: Vec::new(),
                port_env_vars: Vec::new(),
                render_digest: render_digest.to_owned(),
            }];
            policy
        }
    }

    /// A changed shared render is replaced in the one order that frees its
    /// published host port before the new container is created (effigy#377).
    #[test]
    fn a_changed_shared_render_stops_then_removes_before_it_starts_the_new_one() {
        use crate::contract_test_support::EnvGuard;
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-replace-",
            project,
            &container,
            "previous-render",
        );
        let _env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    fixture.root.join("bin").display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("HOME", Some(fixture.root.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
        ]);
        let policy = fixture.policy(project, "this-render");

        let notes = super::ensure_shared_services_running(&policy).expect("ensure succeeds");

        assert_eq!(
            notes,
            vec!["replaced shared legacy_mysql (render changed)".to_owned()]
        );
        let calls = fixture.calls();
        let position = |needle: &str| {
            calls
                .iter()
                .position(|call| call.split(' ').any(|arg| arg == needle))
                .unwrap_or_else(|| panic!("`{needle}` was never run: {}", calls.join("; ")))
        };
        let listed = calls
            .iter()
            .position(|call| call.starts_with("ps "))
            .expect("the runtime is asked which render is running");
        assert!(calls[listed].contains("--all"), "{}", calls[listed]);
        assert!(
            position("stop") < position("rm") && position("rm") < position("up"),
            "stop, remove, then up: {calls:?}"
        );
        assert!(
            calls[position("stop")].ends_with(&container),
            "the stale container is stopped by name: {calls:?}"
        );
        assert!(
            calls[position("rm")].contains(&format!("rm -f {container}")),
            "the stale container is removed by name: {calls:?}"
        );
        assert!(
            !calls.iter().any(|call| call.contains("volume rm")),
            "the shared data volume is kept: {calls:?}"
        );
    }

    /// An unchanged shared render is never touched, so a second `up` leaves a
    /// healthy shared data service and its connections alone (effigy#377).
    #[test]
    fn an_unchanged_shared_render_is_left_alone() {
        use crate::contract_test_support::EnvGuard;
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-unchanged-",
            project,
            &container,
            "this-render",
        );
        let _env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    fixture.root.join("bin").display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("HOME", Some(fixture.root.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
        ]);
        let policy = fixture.policy(project, "this-render");

        let notes = super::ensure_shared_services_running(&policy).expect("ensure succeeds");

        assert_eq!(
            notes,
            vec!["legacy_mysql [mariadb] -> host.docker.internal:23306".to_owned()]
        );
        let calls = fixture.calls();
        assert!(
            !calls
                .iter()
                .any(|call| call.starts_with("stop ") || call.starts_with("rm ")),
            "an unchanged shared service is not stopped or removed: {calls:?}"
        );
        assert!(
            calls.iter().any(|call| call.contains(" up ")),
            "the unchanged service still gets its one apply: {calls:?}"
        );
    }
}
