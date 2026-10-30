use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_catalog::volumes::{
    remove_volume_command, DockerCommand, VolumeClassification, VolumeIdentity,
};
use effigy_containers::backend::endpoint_backend;
use effigy_containers::{
    exec::{
        container_last_log_line_for_policy, list_running_compose_containers_for_policy,
        ContainerExecError, RunningComposeContainer,
    },
    health::{wait_for_ready, HealthWaitError},
    EffectiveContainerPolicy,
};
use effigy_containers::{
    BackendId, ContainerAction, ContainerBackendDetection, ContainerInterruptPolicy,
    ContainerManager, ContainerManagerRequest,
};
use effigy_core::repo_markers::has_task_manifest;
use effigy_core::shell::shell_quote;
use effigy_ui::steps::ProgressSource;
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
/// the old behaviour. `deadline` is the bring-up's one observation budget; a
/// caller without one starts the environment's own health timeout now.
pub(super) fn wait_for_container_ready(
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
    deadline: Option<std::time::Instant>,
) -> Result<Option<&'static str>, RunnerError> {
    match super::readiness::await_services_ready(
        &policy.repo_root,
        policy,
        stop_requested,
        deadline,
    )? {
        super::readiness::ReadinessEnd::Interrupted => return Ok(Some("interrupted")),
        super::readiness::ReadinessEnd::Ready | super::readiness::ReadinessEnd::NothingToAwait => {}
    }
    wait_for_ready(
        &policy.name,
        policy.health_check.as_deref(),
        policy.health_timeout_secs,
        stop_requested,
    )
    .map_err(|error| declared_health_refusal(policy, error))
}

/// The container a declared environment health check belongs to: the Compose
/// project's one primary service container.
fn declared_health_container(policy: &EffectiveContainerPolicy) -> String {
    format!("{}-{}-1", policy.project_name, policy.primary_service)
}

/// The last line the container itself printed, read only when a declared check
/// did not come ready: the evidence its health check cannot give (effigy#455).
fn declared_health_tail(policy: &EffectiveContainerPolicy) -> Option<String> {
    let container = declared_health_container(policy);
    let line =
        container_last_log_line_for_policy(&policy.repo_root, policy, &container, 20).ok()?;
    (!line.is_empty()).then_some(line)
}

/// A declared health check that did not turn ready, as the wait it was in: the
/// container, the check, the answer it gave and the container's own last
/// output (effigy#460, Q-017). A definite answer is terminal, so the refusal
/// names the status it observed instead of a spent budget (effigy#455).
fn declared_health_refusal(
    policy: &EffectiveContainerPolicy,
    error: HealthWaitError,
) -> RunnerError {
    declared_health_refusal_with_tail(policy, error, declared_health_tail(policy))
}

/// The same refusal with the container's last output already read, so the
/// message and its machine-readable waits are testable without a runtime.
fn declared_health_refusal_with_tail(
    policy: &EffectiveContainerPolicy,
    error: HealthWaitError,
    tail: Option<String>,
) -> RunnerError {
    let mut message = error.to_string();
    if let Some(tail) = &tail {
        message.push_str(&format!("; last container output: {tail}"));
    }
    let mut wait = json!({
        "kind": "health_check",
        "environment": policy.name,
        "service": policy.primary_service,
        "container": declared_health_container(policy),
        "check": error.check(),
        "timeout_secs": error.timeout_secs(),
        "probe": "declared health check",
        "answer": error.outcome(),
    });
    if let Some(status) = error.observed_status() {
        wait["status"] = json!(status);
    }
    if let Some(tail) = &tail {
        wait["last_output"] = json!(tail);
    }
    RunnerError::container_wait_refused(
        &policy.name,
        message,
        json!({
            "failure_kind": "container_readiness",
            "readiness": "unverified",
            "waiting_on": [wait],
        }),
    )
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
    deadline: Option<std::time::Instant>,
) -> Result<Vec<String>, RunnerError> {
    let routes = resolve_gateway_tcp_alias_routes_for_container(repo_root, policy, deadline)?;
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
    deadline: Option<std::time::Instant>,
) -> Result<(), RunnerError> {
    if policy.declared_ports.is_empty() {
        return Ok(());
    }
    let owned = effigy_containers::exec::list_running_compose_containers_for_policy_with_deadline(
        repo_root, policy, deadline,
    )
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
    ensure_shared_services_observed(policy, &ProgressSource::waiting_for("shared services"))
}

/// The ensure with the wait it is in named on `progress`: for each shared
/// service, the machine gate, the runtime's container read, the render
/// publication and the `compose up` it runs (effigy#460, Q-017).
pub(in crate::runner) fn ensure_shared_services_observed(
    policy: &EffectiveContainerPolicy,
    progress: &ProgressSource,
) -> Result<Vec<String>, RunnerError> {
    let renders = policy
        .render
        .as_ref()
        .map_or(&[][..], |render| render.shared.as_slice());
    ensure_shared_services_with_renders_observed(policy, renders, progress)
}

/// The ensure with the shared renders it may publish. They are the ones the
/// policy's one render produced; publication happens after the vanished-render
/// refusal so a missing directory is never silently restored (ef5780da).
#[cfg(test)]
fn ensure_shared_services_with_renders(
    policy: &EffectiveContainerPolicy,
    renders: &[effigy_containers::SharedRender],
) -> Result<Vec<String>, RunnerError> {
    ensure_shared_services_with_renders_observed(
        policy,
        renders,
        &ProgressSource::waiting_for("shared services"),
    )
}

fn ensure_shared_services_with_renders_observed(
    policy: &EffectiveContainerPolicy,
    renders: &[effigy_containers::SharedRender],
    progress: &ProgressSource,
) -> Result<Vec<String>, RunnerError> {
    let mut notes = Vec::new();
    let total = policy.shared_services.len() as u64;
    for (done, service) in policy.shared_services.iter().enumerate() {
        let done = done as u64;
        // Held until this service's ensure is complete; a gate that cannot be
        // taken is named and the ensure proceeds, because the service still
        // has to run.
        progress.update(
            format!(
                "waiting for the machine gate of shared project `{}`",
                service.project_name
            ),
            Some((done, total)),
            None,
        );
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
        // One inventory read serves the refusal and the replacement below.
        progress.update(
            format!(
                "reading the runtime's containers for shared `{}`",
                service.service_name
            ),
            Some((done, total)),
            None,
        );
        let containers = read_shared_containers(policy, service);
        refuse_vanished_shared_render(policy, service, containers.as_deref())
            .map_err(|error| shared_service_refusal(policy, service, "render check", error))?;
        if let Some(render) = renders
            .iter()
            .find(|render| Some(render.directory.as_path()) == service.compose_file.parent())
        {
            progress.update(
                format!("publishing the render of shared `{}`", service.service_name),
                Some((done, total)),
                None,
            );
            effigy_containers::policy::render::publish_shared_render(render).map_err(|error| {
                shared_service_refusal(
                    policy,
                    service,
                    "render publication",
                    RunnerError::task_invocation(format!(
                        "cannot publish shared {} render: {error}",
                        service.service_name
                    )),
                )
            })?;
        }
        progress.update(
            format!(
                "checking the render of shared `{}` against its container",
                service.service_name
            ),
            Some((done, total)),
            None,
        );
        let replaced = replace_stale_shared_render(policy, service, containers.as_deref())
            .map_err(|error| {
                shared_service_refusal(policy, service, "render replacement", error)
            })?;
        let workdir = service.compose_file.parent().ok_or_else(|| {
            RunnerError::task_invocation("shared service compose file has no parent directory")
        })?;
        progress.update(
            format!(
                "waiting for docker compose up (shared {}) to return",
                service.service_name
            ),
            Some((done, total)),
            None,
        );
        run_shared_compose_capture(
            workdir,
            policy,
            &shared_compose_args(service, ["up", "-d", "--remove-orphans"]),
            &format!("docker compose up (shared {})", service.service_name),
        )
        .map_err(|error| shared_service_refusal(policy, service, "docker compose up", error))?;
        // The shared stack is started, so it gets the restart policy here too
        // (effigy#383).
        effigy_containers::policy::restart::apply_after_start(
            &policy.repo_root,
            policy,
            &service.project_name,
            std::slice::from_ref(&service.compose_file),
            None,
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

/// A shared service's failed step, as the wait it was in: the service, its
/// project and the step that did not finish. A refusal that already carries
/// its own machine-readable document keeps it (effigy#460, Q-017).
fn shared_service_refusal(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
    step: &str,
    error: RunnerError,
) -> RunnerError {
    if error.json_error_details().is_some() {
        return error;
    }
    RunnerError::container_wait_refused(
        &policy.name,
        error.to_string(),
        serde_json::json!({
            "failure_kind": "shared_service",
            "waiting_on": [{
                "kind": "shared_service",
                "service": service.service_name,
                "project": service.project_name,
                "step": step,
            }],
        }),
    )
}

/// The shared project's containers as the runtime reports them. A failed read is
/// named and yields `None`.
fn read_shared_containers(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
) -> Option<Vec<effigy_containers::exec::SharedProjectContainer>> {
    match effigy_containers::exec::shared_project_containers(
        &policy.repo_root,
        policy,
        &service.project_name,
    ) {
        Ok(containers) => Some(containers),
        Err(error) => {
            eprintln!(
                "[warn] cannot read the runtime's shared {} containers: {error}",
                service.service_name
            );
            None
        }
    }
}

/// Refuse a shared service that would start against a missing render (ef5780da).
///
/// A container whose own render directory is gone has bind sources that no
/// longer exist, so starting or replacing it fails after a stop or remove. That
/// is refused before any publish, stop, remove or Compose call, whichever render
/// the container was created from. A missing render with no container is the
/// normal creation path. A missing render with an unreadable inventory is
/// refused, because no container can be ruled out. A present render keeps the
/// previous behaviour when the inventory is unreadable.
fn refuse_vanished_shared_render(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
    containers: Option<&[effigy_containers::exec::SharedProjectContainer]>,
) -> Result<(), RunnerError> {
    let current_present = service.compose_file.is_file();
    let Some(containers) = containers else {
        if current_present {
            return Ok(());
        }
        let render = service
            .compose_file
            .parent()
            .unwrap_or(&service.compose_file)
            .display();
        return Err(RunnerError::task_invocation(format!(
            "shared {service} (project `{project}`) has no render at `{render}`, and the runtime's containers cannot be read, so no container can be ruled out. Nothing was stopped, removed, published or started. Make the runtime readable, then re-run `effigy container up {env}`.",
            service = service.service_name,
            project = service.project_name,
            env = policy.name,
        )));
    };
    // `<project>/<digest>/<file>`: the project directory names every render.
    let project_dir = service.compose_file.ancestors().nth(2);
    let vanished = containers
        .iter()
        .filter_map(|container| {
            let label = container.render.as_deref()?;
            if label == service.render_digest {
                let directory = service.compose_file.parent()?;
                return (!current_present)
                    .then(|| (container.container_name.as_str(), directory.to_path_buf()));
            }
            let directory = project_dir?.join(label);
            (!directory.is_dir()).then_some((container.container_name.as_str(), directory))
        })
        .collect::<Vec<_>>();
    if vanished.is_empty() {
        return Ok(());
    }
    let names = vanished
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    let renders = vanished
        .iter()
        .map(|(_, directory)| directory.display().to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");
    Err(RunnerError::task_invocation(format!(
        "shared {service} (project `{project}`) has container {names} created from render `{renders}`, which is missing, so Compose cannot start it. Nothing was stopped, removed or re-rendered: the container and its named data volume are unchanged. Recover by removing exactly the container named here with the runtime's `rm -f` (its named volume stays), then re-run `effigy container up {env}`, which renders the shared service and creates it on the preserved volume.",
        service = service.service_name,
        project = service.project_name,
        env = policy.name,
    )))
}

/// Stop and remove every container of a shared project the runtime says came
/// from a different render, so Compose creates the new one against a free
/// port. An unreadable runtime (`None`) is left to Compose rather than stopping
/// a service this run cannot see.
fn replace_stale_shared_render(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
    containers: Option<&[effigy_containers::exec::SharedProjectContainer]>,
) -> Result<bool, RunnerError> {
    let Some(containers) = containers else {
        return Ok(false);
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
                    "{label} failed ({})\nstdout:\n{}\nstderr:\n{}",
                    effigy_ui::human::exit_code(output.status.code()),
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
                    "{} failed ({})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    effigy_ui::human::exit_code(output.status.code()),
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
                    "{} failed ({})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    effigy_ui::human::exit_code(output.status.code()),
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
                    "{} failed ({})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    effigy_ui::human::exit_code(output.status.code()),
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
                    "{} failed ({})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    effigy_ui::human::exit_code(output.status.code()),
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
    fn a_definite_health_refusal_names_the_status_the_container_and_its_tail() {
        let mut policy = test_policy();
        let check = "http://127.0.0.1:3000/health".to_owned();
        policy.health_check = Some(check.clone());
        policy.health_timeout_secs = 60;
        let error = super::declared_health_refusal_with_tail(
            &policy,
            effigy_containers::health::HealthWaitError::StatusMismatch {
                container: policy.name.clone(),
                check: check.clone(),
                status: 500,
                timeout_secs: 60,
            },
            Some("app: GET /health answered 500".to_owned()),
        );
        let message = error.to_string();
        assert!(
            message.contains("answered HTTP 500") && message.contains("status_mismatch"),
            "{message}"
        );
        assert!(
            message.contains("last container output: app: GET /health answered 500"),
            "{message}"
        );
        let details: serde_json::Value = serde_json::from_str(
            error
                .json_error_details()
                .expect("a declared check refusal carries its wait"),
        )
        .expect("the wait details are JSON");
        assert_eq!(details["failure_kind"], "container_readiness");
        assert_eq!(details["waiting_on"][0]["kind"], "health_check");
        assert_eq!(details["waiting_on"][0]["answer"], "status_mismatch");
        assert_eq!(details["waiting_on"][0]["status"], 500);
        assert_eq!(details["waiting_on"][0]["check"], check);
        assert_eq!(
            details["waiting_on"][0]["container"],
            "demo-web-renamed-app-1"
        );
        assert_eq!(
            details["waiting_on"][0]["last_output"],
            "app: GET /health answered 500"
        );
    }

    #[test]
    fn a_spent_health_budget_keeps_the_old_refusal_without_a_status() {
        let policy = test_policy();
        let error = super::declared_health_refusal_with_tail(
            &policy,
            effigy_containers::health::HealthWaitError::NotReady {
                container: policy.name.clone(),
                check: "tcp://127.0.0.1:9".to_owned(),
                timeout_secs: 60,
            },
            None,
        );
        assert!(
            error
                .to_string()
                .contains("did not become ready within 60s"),
            "{error}"
        );
        let details: serde_json::Value =
            serde_json::from_str(error.json_error_details().expect("wait details"))
                .expect("JSON details");
        assert_eq!(details["waiting_on"][0]["answer"], "not_ready");
        assert!(details["waiting_on"][0].get("status").is_none());
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

    /// A private checkout whose runtime is a script recording every call. The
    /// store holds the renders `<project>/<digest>` the bindings point at, and a
    /// named data volume that the fake `volume rm` deletes.
    struct SharedFixture {
        _fixture: effigy_test_support::Fixture,
        root: PathBuf,
        volume: PathBuf,
    }

    impl SharedFixture {
        /// The runtime lists one container of `project` rendered from
        /// `container_render`.
        fn new(name: &str, project: &str, container: &str, container_render: &str) -> Self {
            let listing = format!(
                "printf '%s\\t%s\\t%s\\t%s\\n' '{container}' 'Up 2 minutes' '{container_render}' '{project}'"
            );
            Self::build(name, project, &listing)
        }

        /// The same runtime with no container of `project` on it.
        fn unlisted(name: &str, project: &str) -> Self {
            Self::build(name, project, "exit 0")
        }

        /// A runtime whose inventory read fails.
        fn unreadable(name: &str, project: &str) -> Self {
            Self::build(name, project, "echo 'daemon is down' >&2; exit 1")
        }

        fn build(name: &str, project: &str, listing: &str) -> Self {
            let fixture = effigy_test_support::Fixture::workspace(name).unwrap();
            let root = fixture.path().canonicalize().unwrap();
            let bin = root.join("bin");
            std::fs::create_dir(&bin).unwrap();
            let log = root.join("calls");
            let volumes = root.join("volumes");
            let volume = volumes.join(format!("{project}-data"));
            std::fs::create_dir_all(&volume).unwrap();
            std::fs::write(volume.join("rows"), "kept\n").unwrap();
            let script = format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  *'volume rm'*)\n    rm -rf {volumes}\n    ;;\n  *'--format'*)\n    {listing}\n    ;;\nesac\n",
                log = log.display(),
                volumes = volumes.display(),
                listing = listing,
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
            for digest in ["this-render", "previous-render"] {
                let directory = root.join("store").join(digest);
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(directory.join("compose.yml"), "services: {}\n").unwrap();
            }
            Self {
                _fixture: fixture,
                root,
                volume,
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
                compose_file: self.render_dir(render_digest).join("compose.yml"),
                host: "host.docker.internal".to_owned(),
                host_port: 23306,
                container_port: 3306,
                host_env_vars: Vec::new(),
                port_env_vars: Vec::new(),
                render_digest: render_digest.to_owned(),
            }];
            policy
        }

        fn render_dir(&self, digest: &str) -> PathBuf {
            self.root.join("store").join(digest)
        }

        /// The render `up` hands the ensure: the directory and its compose file,
        /// as `publish_shared_render` would write them.
        fn render(&self, digest: &str) -> effigy_containers::SharedRender {
            effigy_containers::SharedRender {
                directory: self.render_dir(digest),
                files: std::collections::BTreeMap::from([(
                    PathBuf::from("compose.yml"),
                    b"services: {}\n".to_vec(),
                )]),
            }
        }

        /// Removes a rendered directory, as an operator's cleanup would.
        fn vanish_render(&self, digest: &str) {
            std::fs::remove_dir_all(self.render_dir(digest)).unwrap();
        }

        /// Whether the named data volume still holds its rows.
        fn volume_kept(&self) -> bool {
            self.volume.join("rows").is_file()
        }
    }

    /// The fake runtime's environment for one test, pointed at its checkout.
    fn runtime_env(fixture: &SharedFixture) -> crate::contract_test_support::EnvGuard {
        runtime_env_for(&fixture.root)
    }

    fn runtime_env_for(root: &Path) -> crate::contract_test_support::EnvGuard {
        use crate::contract_test_support::EnvGuard;
        EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    root.join("bin").display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("HOME", Some(root.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
        ])
    }

    /// A shared service whose render is unchanged is never touched, so a second
    /// `up` leaves a healthy shared data service and its connections alone
    /// (effigy#377).
    #[test]
    fn an_unchanged_shared_render_is_left_alone() {
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-unchanged-",
            project,
            &container,
            "this-render",
        );
        let _env = runtime_env(&fixture);
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

    /// A changed shared render is replaced in the one order that frees its
    /// published host port before the new container is created (effigy#377).
    #[test]
    fn a_changed_shared_render_stops_then_removes_before_it_starts_the_new_one() {
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-replace-",
            project,
            &container,
            "previous-render",
        );
        let _env = runtime_env(&fixture);
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
        assert!(
            position("stop") < position("rm") && position("rm") < position("up"),
            "stop, remove, then up: {calls:?}"
        );
        assert!(
            !calls.iter().any(|call| call.contains("volume rm")),
            "the shared data volume is kept: {calls:?}"
        );
        assert!(fixture.volume_kept());
    }

    /// A container created from the current render, whose render directory has
    /// vanished, is refused before anything is published, stopped, removed or
    /// started. Its identity and named volume are unchanged (ef5780da).
    #[test]
    fn a_container_from_a_vanished_render_is_refused_before_anything_changes() {
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-vanished-",
            project,
            &container,
            "this-render",
        );
        let _env = runtime_env(&fixture);
        fixture.vanish_render("this-render");
        let policy = fixture.policy(project, "this-render");

        let error =
            super::ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
                .expect_err("a vanished render with its container is refused");

        let message = error.to_string();
        assert!(message.contains("legacy_mysql"), "{message}");
        assert!(message.contains(project), "{message}");
        assert!(message.contains("is missing"), "{message}");
        assert!(message.contains(&container), "{message}");
        assert!(message.contains("rm -f"), "{message}");
        assert!(message.contains("effigy container up"), "{message}");
        assert!(
            !fixture.render_dir("this-render").exists(),
            "the vanished render is not silently restored"
        );
        let calls = fixture.calls();
        assert!(
            calls.iter().all(|call| call.starts_with("ps ")),
            "only the runtime listing ran: {calls:?}"
        );
        assert!(fixture.volume_kept(), "the named volume is preserved");
    }

    /// A container from an older render whose own render directory has vanished
    /// is refused even when the current render is present: starting or replacing
    /// it would stop a container whose bind sources are gone (ef5780da).
    #[test]
    fn a_container_from_a_vanished_older_render_is_refused() {
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-older-",
            project,
            &container,
            "previous-render",
        );
        let _env = runtime_env(&fixture);
        fixture.vanish_render("previous-render");
        let policy = fixture.policy(project, "this-render");

        let error = super::ensure_shared_services_running(&policy)
            .expect_err("an older vanished render with its container is refused");

        let message = error.to_string();
        assert!(message.contains("is missing"), "{message}");
        assert!(message.contains(&container), "{message}");
        assert!(message.contains("rm -f"), "{message}");
        assert!(
            !fixture.render_dir("previous-render").exists(),
            "the older render is not silently restored"
        );
        let calls = fixture.calls();
        assert!(
            calls.iter().all(|call| call.starts_with("ps ")),
            "nothing is stopped, removed or started: {calls:?}"
        );
        assert!(fixture.volume_kept(), "the named volume is preserved");
    }

    /// A stale container whose render is present is replaced after the current
    /// render is published, with the named volume kept (effigy#377).
    #[test]
    fn a_container_from_another_render_is_replaced_after_its_render_is_published() {
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-replaced-",
            project,
            &container,
            "previous-render",
        );
        let _env = runtime_env(&fixture);
        fixture.vanish_render("this-render");
        let policy = fixture.policy(project, "this-render");

        let notes =
            super::ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
                .expect("a changed render replaces its container");

        assert_eq!(
            notes,
            vec!["replaced shared legacy_mysql (render changed)".to_owned()]
        );
        assert!(fixture
            .render_dir("this-render")
            .join("compose.yml")
            .is_file());
        let calls = fixture.calls();
        let position = |needle: &str| {
            calls
                .iter()
                .position(|call| call.split(' ').any(|arg| arg == needle))
                .unwrap_or_else(|| panic!("`{needle}` was never run: {}", calls.join("; ")))
        };
        assert!(
            position("stop") < position("rm") && position("rm") < position("up"),
            "stop, remove, then up: {calls:?}"
        );
        assert!(
            !calls.iter().any(|call| call.contains("volume rm")),
            "the shared data volume is kept: {calls:?}"
        );
        assert!(fixture.volume_kept());
    }

    /// A vanished render with an unreadable inventory is refused: no container
    /// can be ruled out, so nothing is published or started.
    #[test]
    fn a_vanished_render_with_an_unreadable_inventory_is_refused() {
        let project = "effigy-shared-mariadb-probe";
        let fixture = SharedFixture::unreadable("effigy-shared-unreadable-", project);
        let _env = runtime_env(&fixture);
        fixture.vanish_render("this-render");
        let policy = fixture.policy(project, "this-render");

        let error =
            super::ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
                .expect_err("an unreadable inventory is refused");

        assert!(error.to_string().contains("cannot be read"), "{error}");
        let calls = fixture.calls();
        assert!(
            calls.iter().all(|call| call.starts_with("ps ")),
            "nothing is published, stopped or started: {calls:?}"
        );
        assert!(!fixture.render_dir("this-render").exists());
    }

    /// A present render with an unreadable inventory keeps the previous
    /// behaviour: it is named and the service still starts.
    #[test]
    fn a_healthy_render_with_an_unreadable_inventory_still_starts() {
        let project = "effigy-shared-mariadb-probe";
        let fixture = SharedFixture::unreadable("effigy-shared-healthy-", project);
        let _env = runtime_env(&fixture);
        let policy = fixture.policy(project, "this-render");

        let notes = super::ensure_shared_services_running(&policy)
            .expect("a present render starts without the inventory");

        assert_eq!(
            notes,
            vec!["legacy_mysql [mariadb] -> host.docker.internal:23306".to_owned()]
        );
        assert!(fixture.calls().iter().any(|call| call.contains(" up ")));
    }

    /// First creation: no shared container and no render yet. The ensure
    /// publishes the render, then Compose creates the service (effigy#377).
    #[test]
    fn a_first_creation_publishes_its_render_then_starts_the_service() {
        let project = "effigy-shared-mariadb-probe";
        let fixture = SharedFixture::unlisted("effigy-shared-fresh-", project);
        let _env = runtime_env(&fixture);
        fixture.vanish_render("this-render");
        let policy = fixture.policy(project, "this-render");

        let notes =
            super::ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
                .expect("first creation succeeds");

        assert_eq!(
            notes,
            vec!["legacy_mysql [mariadb] -> host.docker.internal:23306".to_owned()]
        );
        assert!(fixture
            .render_dir("this-render")
            .join("compose.yml")
            .is_file());
        let calls = fixture.calls();
        assert!(
            calls.iter().any(|call| call.contains(" up ")),
            "the service is created: {calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|call| call.starts_with("stop ") || call.starts_with("rm ")),
            "nothing existing is stopped or removed: {calls:?}"
        );
        assert!(fixture.volume_kept());
    }

    /// A checkout whose manifest declares a shared mariadb service and whose
    /// runtime is a script. Unlike [`SharedFixture`], the policy, the render
    /// digest and the publication are the product's own: `up`'s preparation
    /// (`load_container_policy_for_creation`) and its shared-service ensure run
    /// here, so a proof cannot pass on a hand-built binding.
    struct UpFixture {
        _fixture: effigy_test_support::Fixture,
        root: PathBuf,
        home: PathBuf,
        listing: PathBuf,
        volume: PathBuf,
    }

    impl UpFixture {
        fn new(name: &str) -> Self {
            let fixture = effigy_test_support::Fixture::workspace(name).unwrap();
            let root = fixture.path().canonicalize().unwrap();
            let bin = root.join("bin");
            std::fs::create_dir(&bin).unwrap();
            let log = root.join("calls");
            let volumes = root.join("volumes");
            let volume = volumes.join("effigy-shared-mariadb-data");
            std::fs::create_dir_all(&volume).unwrap();
            std::fs::write(volume.join("rows"), "kept\n").unwrap();
            // The runtime's shared-project inventory: rows the test writes.
            let listing = root.join("listing");
            std::fs::write(&listing, "").unwrap();
            let script = format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  *'volume rm'*)\n    rm -rf {volumes}\n    ;;\n  *'--format'*)\n    if [ -s {listing} ]; then cat {listing}; fi\n    ;;\nesac\n",
                log = log.display(),
                volumes = volumes.display(),
                listing = listing.display(),
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
            std::fs::write(
                root.join("effigy.toml"),
                r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
            )
            .unwrap();
            let home = root.join("effigy-home");
            std::fs::create_dir(&home).unwrap();
            Self {
                _fixture: fixture,
                root,
                home,
                listing,
                volume,
            }
        }

        /// `up`'s preparation: resolve the creation policy from this checkout's
        /// manifest, render it and prepare its mounts. The shared render is
        /// rendered here and published by the ensure, never by this stage.
        fn prepare(&self) -> EffectiveContainerPolicy {
            effigy_containers::with_test_effigy_home(&self.home, || {
                effigy_containers::load_container_policy_for_creation(&self.root, None)
                    .expect("creation policy")
                    .0
            })
        }

        /// `up`'s shared-service stage, on the policy its preparation produced.
        fn ensure(
            &self,
            policy: &EffectiveContainerPolicy,
        ) -> Result<Vec<String>, super::RunnerError> {
            effigy_containers::with_test_effigy_home(&self.home, || {
                super::ensure_shared_services_running(policy)
            })
        }

        /// The one shared binding the manifest declares.
        fn only_shared<'a>(
            &self,
            policy: &'a EffectiveContainerPolicy,
        ) -> &'a effigy_containers::SharedServiceBinding {
            assert_eq!(policy.shared_services.len(), 1, "one shared service");
            &policy.shared_services[0]
        }

        fn container_name(&self, service: &effigy_containers::SharedServiceBinding) -> String {
            format!("{}-{}-1", service.project_name, service.service_name)
        }

        /// The runtime reports one container of the shared project, created
        /// from `render`'s digest.
        fn list_container(&self, container: &str, render: &str, project: &str) {
            std::fs::write(
                &self.listing,
                format!("{container}\tUp 2 minutes\t{render}\t{project}\n"),
            )
            .unwrap();
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.root.join("calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        /// Whether the runtime's named data volume still holds its rows.
        fn volume_kept(&self) -> bool {
            self.volume.join("rows").is_file()
        }
    }

    /// The papercut's refusal, proved through `up`'s own preparation and
    /// publication path. Preparation renders the shared service without
    /// publishing it, the ensure publishes it and Compose creates it, a healthy
    /// existing service is left alone, and a render directory an operator
    /// deleted is neither restored by preparation nor started: the runtime's
    /// container is refused before Compose runs, with its identity and named
    /// data volume unchanged (ef5780da).
    #[test]
    fn up_refuses_a_vanished_shared_render_instead_of_republishing_it() {
        let fixture = UpFixture::new("effigy-shared-up-");
        let _env = runtime_env_for(&fixture.root);

        // First creation: the manifest resolves one shared service whose render
        // exists only once the ensure publishes it.
        let first = fixture.prepare();
        assert_eq!(
            first
                .render
                .as_ref()
                .expect("the creation policy renders")
                .shared
                .len(),
            1,
            "the policy's one render carries the shared service"
        );
        let shared = fixture.only_shared(&first);
        let render_dir = shared
            .compose_file
            .parent()
            .expect("shared render directory")
            .to_path_buf();
        assert_eq!(shared.service_name, "db");
        assert!(
            !render_dir.exists(),
            "preparation renders the shared service without publishing it: {}",
            render_dir.display()
        );
        fixture.ensure(&first).expect("first creation succeeds");
        assert!(
            shared.compose_file.is_file(),
            "the ensure publishes the shared render: {}",
            shared.compose_file.display()
        );
        let created = fixture.calls();
        assert!(
            created.iter().any(|call| {
                call.contains("compose")
                    && call.contains(&shared.compose_file.display().to_string())
                    && call.contains("up -d")
            }),
            "Compose starts the render the ensure published: {created:?}"
        );

        // A healthy existing shared service: the runtime holds the container
        // this render created, so the next `up` leaves it and its data alone.
        let container = fixture.container_name(shared);
        fixture.list_container(&container, &shared.render_digest, &shared.project_name);
        let healthy = fixture.prepare();
        let healthy_shared = fixture.only_shared(&healthy);
        assert_eq!(
            healthy_shared.compose_file, shared.compose_file,
            "one machine-wide render path for the shared project"
        );
        let notes = fixture
            .ensure(&healthy)
            .expect("a healthy shared service starts");
        assert_eq!(
            notes,
            vec![format!(
                "db [mariadb] -> {}:{}",
                healthy_shared.host, healthy_shared.host_port
            )]
        );
        let calls = fixture.calls();
        assert!(
            !calls.iter().any(|call| call.starts_with("stop ")
                || call.starts_with("rm ")
                || call.contains("volume rm")),
            "the healthy container and its volume are untouched: {calls:?}"
        );

        // The operator deletes the rendered project directory; the runtime still
        // holds the container that render created.
        std::fs::remove_dir_all(&render_dir).expect("delete the rendered project");
        let stale = fixture.prepare();
        let stale_shared = fixture.only_shared(&stale);
        assert_eq!(
            stale_shared.compose_file, shared.compose_file,
            "the same render is due again"
        );
        assert!(
            !render_dir.exists(),
            "preparation never republishes a vanished shared render: {}",
            render_dir.display()
        );

        let before_refusal = fixture.calls().len();
        let error = fixture
            .ensure(&stale)
            .expect_err("a vanished shared render with its container is refused");
        let message = error.to_string();
        for expected in [
            "db",
            stale_shared.project_name.as_str(),
            container.as_str(),
            render_dir.display().to_string().as_str(),
            "is missing",
            "named data volume",
            "rm -f",
            "effigy container up web",
        ] {
            assert!(message.contains(expected), "{expected}: {message}");
        }
        // The refusal names the shared wait it was in, machine-readably.
        let details: serde_json::Value = serde_json::from_str(
            error
                .json_error_details()
                .expect("a shared-service refusal carries its wait"),
        )
        .expect("the wait details are JSON");
        assert_eq!(details["failure_kind"], "shared_service");
        assert_eq!(details["waiting_on"][0]["kind"], "shared_service");
        assert_eq!(details["waiting_on"][0]["service"], "db");
        assert_eq!(details["waiting_on"][0]["step"], "render check");
        assert!(
            !render_dir.exists(),
            "the refusal does not re-render the shared service: {}",
            render_dir.display()
        );
        let refusal_calls = &fixture.calls()[before_refusal..];
        assert!(
            refusal_calls.iter().all(|call| call.starts_with("ps ")),
            "one inventory read, then no Compose start, stop or remove: {refusal_calls:?}"
        );
        assert!(
            fixture.volume_kept(),
            "the shared service's named data volume is preserved"
        );
    }
}
