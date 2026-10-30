use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

use effigy_catalog::volumes::{
    remove_volume_command, DockerCommand, VolumeClassification, VolumeIdentity,
};
use effigy_containers::backend::endpoint_backend;
use effigy_containers::{
    exec::{
        container_last_log_line_for_policy, list_running_compose_containers_for_policy,
        list_running_compose_containers_for_policy_with_deadline, ContainerExecError,
        RunningComposeContainer,
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
    let readiness = super::readiness::await_services_ready(
        &policy.repo_root,
        policy,
        stop_requested,
        deadline,
    )?;
    match readiness.end {
        super::readiness::ReadinessEnd::Interrupted => return Ok(Some("interrupted")),
        super::readiness::ReadinessEnd::Ready | super::readiness::ReadinessEnd::NothingToAwait => {}
    }
    let Some(check) = policy.health_check.as_deref() else {
        return Ok(None);
    };
    // The declared check probes one service's endpoint, and the container that
    // serves it is the identity the runtime reported for that service: the
    // readiness probe already observed it, so it is carried from there. When it
    // was never observed, one live step covers the runtime read that names it,
    // so a stalled read is never an idle command (effigy#460, Q-017).
    let container = readiness
        .observed
        .iter()
        .find(|service| service.service == policy.primary_service)
        .map(|service| service.container.clone())
        .or_else(|| primary_service_container_observed(policy, deadline));
    let subject = match &container {
        Some(container) => format!("container `{container}`"),
        None => format!(
            "environment `{}` (primary service `{}`)",
            policy.name, policy.primary_service
        ),
    };
    wait_for_ready(
        &subject,
        Some(check),
        policy.health_timeout_secs,
        stop_requested,
    )
    .map_err(|error| declared_health_refusal(policy, error, container))
}

/// The container the primary service actually runs as: the runtime's own row
/// for that Compose service. Compose may declare an explicit `container_name`
/// and a runtime may name it differently, so Effigy never assumes the
/// `<project>-<service>-1` default for an identity it can read (effigy#460).
///
/// `deadline` is the bring-up's one observation budget, so this read never
/// grants the runtime a fresh one. The read runs under its own live step: the
/// readiness wait has already finished, and a runtime that stops answering here
/// must still say what it is waiting for (Q-017).
fn primary_service_container_observed(
    policy: &EffectiveContainerPolicy,
    deadline: Option<std::time::Instant>,
) -> Option<String> {
    let progress = primary_container_progress(policy);
    let step = super::steps::TimedStep::live(progress);
    let container = read_primary_service_container(policy, deadline);
    match &container {
        Some(container) => step.succeed(format!(
            "primary service `{}` runs as `{container}`",
            policy.primary_service
        )),
        // The declared wait names the environment and service instead; this
        // read's own failure is not the reason the check failed.
        None => step.finish_clear(),
    }
    container
}

/// The exact wait the identity read is in: the runtime's row for the primary
/// service, with elapsed time (effigy#460, Q-017).
fn primary_container_progress(policy: &EffectiveContainerPolicy) -> ProgressSource {
    let progress = ProgressSource::waiting_for(format!(
        "the runtime to name the primary container of `{}`",
        policy.project_name
    ));
    progress.update(
        format!(
            "reading the runtime's row for primary service `{}` of `{}`",
            policy.primary_service, policy.project_name
        ),
        None,
        None,
    );
    progress
}

/// One runtime inventory read, selecting the primary service's own row.
fn read_primary_service_container(
    policy: &EffectiveContainerPolicy,
    deadline: Option<std::time::Instant>,
) -> Option<String> {
    let rows = list_running_compose_containers_for_policy_with_deadline(
        &policy.repo_root,
        policy,
        deadline,
    )
    .ok()?;
    rows.into_iter()
        .find(|row| row.service.as_deref() == Some(policy.primary_service.as_str()))
        .map(|row| row.container_name)
}

/// How many trailing log lines the failed container is read for.
const DECLARED_HEALTH_LOG_TAIL: u32 = 20;

/// The last line the container itself printed, read only when a declared check
/// did not come ready: the evidence its health check cannot give (effigy#455).
///
/// The read is a bounded runtime call of its own (up to 30s), and the declared
/// check's step has already ended, so it runs under a live step that names the
/// container it reads and reports when the runtime does not answer
/// (effigy#460, Q-017).
fn declared_health_tail(policy: &EffectiveContainerPolicy, container: &str) -> Option<String> {
    let step = super::steps::TimedStep::live(declared_health_tail_progress(container));
    match container_last_log_line_for_policy(
        &policy.repo_root,
        policy,
        container,
        DECLARED_HEALTH_LOG_TAIL,
    ) {
        Ok(line) if !line.is_empty() => {
            step.succeed(format!("read the last line `{container}` printed"));
            Some(line)
        }
        // The container printed nothing this read kept; the refusal still
        // carries the status it observed.
        Ok(_) => {
            step.finish_clear();
            None
        }
        Err(error) => {
            step.fail(format!(
                "the runtime did not answer the log read for `{container}`: {}",
                first_line(&error.to_string())
            ));
            None
        }
    }
}

/// The exact wait the log tail is in: the runtime's own log read for the
/// container that failed, with elapsed time (effigy#460, Q-017).
fn declared_health_tail_progress(container: &str) -> ProgressSource {
    let progress = ProgressSource::waiting_for(format!("the runtime's log tail of `{container}`"));
    progress.update(
        format!("reading the last line `{container}` printed"),
        None,
        None,
    );
    progress
}

/// The first line of a runtime error, bounded so one frame stays one line.
fn first_line(text: &str) -> String {
    const LIMIT: usize = 160;
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= LIMIT {
        return line.to_owned();
    }
    let cut: String = line.chars().take(LIMIT).collect();
    format!("{cut}…")
}

/// A declared health check that did not turn ready, as the wait it was in: the
/// container, the check, the answer it gave and the container's own last
/// output (effigy#460, Q-017). A definite answer is terminal, so the refusal
/// names the status it observed instead of a spent budget (effigy#455).
fn declared_health_refusal(
    policy: &EffectiveContainerPolicy,
    error: HealthWaitError,
    container: Option<String>,
) -> RunnerError {
    let tail = container
        .as_deref()
        .and_then(|container| declared_health_tail(policy, container));
    declared_health_refusal_with_evidence(policy, error, container, tail)
}

/// The same refusal with the container and its last output already read, so the
/// message and its machine-readable waits are testable without a runtime.
fn declared_health_refusal_with_evidence(
    policy: &EffectiveContainerPolicy,
    error: HealthWaitError,
    container: Option<String>,
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
        "container": container,
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
        // Each service is its own wait: entering it replaces the previous
        // service's awaited subject and drops the event it ended on, so a
        // stalled stage here never names the service that already finished
        // (effigy#460, Q-017).
        begin_shared_service_wait(progress, &service.service_name);
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
        // Compose has returned for this shared stack, so the frame follows the
        // restart-policy apply stage by stage: the Compose declaration read,
        // the runtime inventory read, then the update. A stalled read names
        // that read, never the finished Compose call (effigy#460, Q-017).
        let restart_subject = shared_service_subject(&service.service_name);
        let mut restart_stage = |stage: super::steps::RestartPolicyStage| {
            super::steps::restart_policy_stage(
                progress,
                &restart_subject,
                stage,
                Some((done, total)),
            );
        };
        // The shared stack is started, so it gets the restart policy here too
        // (effigy#383). Its containers are proven against the render just
        // published, not against a checkout.
        effigy_containers::policy::restart::apply_after_start_shared_observed(
            &policy.repo_root,
            policy,
            &service.project_name,
            std::slice::from_ref(&service.compose_file),
            &service.render_digest,
            None,
            &mut restart_stage,
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

/// The awaited subject for one shared service's wait.
fn shared_service_subject(service_name: &str) -> String {
    format!("shared `{service_name}`")
}

/// Enter one shared service's wait: it names that service and drops the
/// previous service's event, which described work that already finished
/// (effigy#460, Q-017).
fn begin_shared_service_wait(progress: &ProgressSource, service_name: &str) {
    progress.waiting_on(shared_service_subject(service_name));
    progress.clear_event();
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
            "shared {service} (project `{project}`) has no render at `{render}`, and the runtime's containers cannot be read, so no container can be ruled out. Nothing was stopped, removed, published or started. Make the runtime readable, then re-run `effigy container {env} up`.",
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
        "shared {service} (project `{project}`) has container {names} created from render `{renders}`, which is missing, so Compose cannot start it. Nothing was stopped, removed or re-rendered: the container and its named data volume are unchanged. Recover by removing exactly the container named here with the runtime's `rm -f` (its named volume stays), then re-run `effigy container {env} up`, which renders the shared service and creates it on the preserved volume.",
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

/// Bound for one shared-service Compose control call (an ensure's `compose
/// up`). A hung runtime must not hold the caller forever; the capture still
/// retains the call's complete output.
const SHARED_COMPOSE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Bound for one shared data exec (reset, import or logical dump). A whole
/// database may travel through the call, so the bound is generous but real;
/// complete output is retained because the caller writes it to the dump
/// artifact. On expiry the adapter terminates the owned process group.
const SHARED_DATA_EXEC_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// One shared-service Compose invocation on the dependent environment's exact
/// backend, never a hard-coded engine. It runs from the shared render's own
/// machine-wide directory, so the invocation names no checkout (effigy#377).
fn shared_compose_invocation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    action: ContainerAction,
    label: &str,
) -> Result<effigy_containers::ContainerComposeInvocationPlan, RunnerError> {
    let backend =
        effigy_containers::backend::exact_backend(repo_root, &policy.name, policy.driver).id;
    let profile = policy.profile.as_str();
    let request = ContainerManagerRequest::new(repo_root)
        .backend_override(backend.clone())
        .interrupt_policy(ContainerInterruptPolicy::Forward);
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let mut plan = ContainerManager::defaults()
        .compose_invocation_plan(&request, &detection, profile, args, action, label)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    // A VM-hosted backend cannot inherit the host environment, so the shared
    // transform names the Compose interpolation file its VM can read.
    plan.args = effigy_containers::policy::render::compose_invocation_args(
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
    Ok(plan)
}

/// Run one shared-service Compose subcommand through the bounded capture
/// adapter, so a shared compose call cannot leak a child, an inherited pipe or
/// an unbounded wait into a data command (effigy#477).
fn run_shared_compose_bounded(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    action: ContainerAction,
    label: &str,
    stdin_file: Option<&Path>,
    timeout: Duration,
) -> Result<Output, RunnerError> {
    let plan = shared_compose_invocation_plan(&policy.repo_root, policy, args, action, label)?;
    // One absolute deadline supervises the spawn; expiry terminates the owned
    // process group, and complete output stays retained for the caller's
    // artifact (no per-stream cap in the adapter's deadline path).
    let deadline = Instant::now() + timeout;
    crate::runner::exec_command::run_command_capture_until(
        cwd,
        plan.program.as_os_str(),
        &plan.args,
        stdin_file,
        Some(deadline),
    )
    .map_err(|error| match error {
        RunnerError::TaskInvocation(message) => RunnerError::task_invocation(format!(
            "{label} did not complete within {}s: {message}",
            timeout.as_secs()
        )),
        other => other,
    })
}

fn run_shared_compose_capture(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let output = run_shared_compose_bounded(
        cwd,
        policy,
        args,
        ContainerAction::Activate,
        label,
        None,
        SHARED_COMPOSE_TIMEOUT,
    )?;
    if output.status.success() {
        return Ok(output);
    }
    Err(RunnerError::task_invocation(format!(
        "{label} failed ({})\nstdout:\n{}\nstderr:\n{}",
        effigy_ui::human::exit_code(output.status.code()),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
}

/// Exec a command in one shared service's own Compose project and render.
///
/// The invocation names the shared render file and project, so Compose resolves
/// the declared service inside that project; a checkout-local project never
/// receives the command. Output and the optional input file travel through the
/// same bounded capture as the shared-service ensure.
///
/// Values in `env` are forwarded to the container by name only. Each exec
/// writes its own private file under the Effigy home (0600 inside a 0700
/// directory, created exclusively and never rewritten) that the client sources
/// before it runs: the host environment for a host backend, and the VM-readable
/// home through `colima ssh` for a VM-hosted backend (the same transport that
/// carries managed task secrets across that boundary). The file is removed when
/// the call returns, so concurrent data commands never share a credential path.
/// No value reaches the command line, the Compose invocation or a report.
///
/// One absolute deadline bounds the call; expiry terminates the owned process
/// group, and the adapter retains the call's complete output.
pub(in crate::runner) fn run_shared_service_exec_capture<'a>(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
    tail: impl IntoIterator<Item = &'a str>,
    env: &std::collections::BTreeMap<String, String>,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    let cwd = service.compose_file.parent().ok_or_else(|| {
        RunnerError::task_invocation("shared service compose file has no parent directory")
    })?;
    let args = shared_compose_args(service, tail);
    let label = format!("docker compose exec (shared {})", service.service_name);
    if env.is_empty() {
        return run_shared_compose_bounded(
            cwd,
            policy,
            &args,
            ContainerAction::Exec,
            &label,
            stdin_file,
            SHARED_DATA_EXEC_TIMEOUT,
        );
    }
    let plan = shared_compose_invocation_plan(
        &policy.repo_root,
        policy,
        &args,
        ContainerAction::Exec,
        &label,
    )?;
    super::shared_credentials::run_shared_credential_exec(
        cwd,
        policy,
        &plan,
        service,
        env,
        stdin_file,
        SHARED_DATA_EXEC_TIMEOUT,
    )
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
    use effigy_ui::steps::ProgressSource;
    use std::ffi::OsString;
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

    /// A hung shared Compose call is bounded and takes its whole process group
    /// with it, so no child outlives the caller (effigy#477).
    #[test]
    fn a_hung_shared_compose_call_hits_its_deadline_and_takes_its_process_group() {
        use crate::contract_test_support::EnvGuard;
        use std::os::unix::fs::PermissionsExt;
        let fixture =
            effigy_test_support::Fixture::workspace("effigy-shared-compose-deadline-").unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let pid_file = root.join("child.pid");
        let script = format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > {pid_file}\nsleep 60\nprintf 'late\n'\n",
            pid_file = pid_file.display()
        );
        for name in ["docker", "colima"] {
            let executable = bin.join(name);
            std::fs::write(&executable, &script).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
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
        let args = vec![
            OsString::from("compose"),
            OsString::from("-f"),
            root.join("compose.yml").into_os_string(),
            OsString::from("up"),
            OsString::from("-d"),
        ];
        let started = std::time::Instant::now();
        let error = super::run_shared_compose_bounded(
            &root,
            &policy,
            &args,
            super::ContainerAction::Activate,
            "shared deadline proof",
            None,
            std::time::Duration::from_millis(300),
        )
        .expect_err("a hung call must hit its deadline");
        assert!(
            error.to_string().contains("did not complete within"),
            "{error}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "the deadline must bound the call"
        );
        let child = std::fs::read_to_string(&pid_file)
            .expect("the fake runtime records its pid")
            .trim()
            .parse::<i32>()
            .expect("pid");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            // SAFETY: signal 0 only probes whether the recorded pid exists.
            if unsafe { libc::kill(child, 0) } == -1 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the timed-out child survived its process-group termination"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
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
        // A Compose `container_name` is not the `<project>-<service>-1`
        // default, so the refusal reports the identity the runtime gave.
        let error = super::declared_health_refusal_with_evidence(
            &policy,
            effigy_containers::health::HealthWaitError::StatusMismatch {
                subject: "container `ui-custom`".to_owned(),
                check: check.clone(),
                status: 500,
                timeout_secs: 60,
            },
            Some("ui-custom".to_owned()),
            Some("app: GET /health answered 500".to_owned()),
        );
        let message = error.to_string();
        assert!(
            message.contains("the health check of container `ui-custom`"),
            "the refusal names the container the runtime reported: {message}"
        );
        assert!(
            !message.contains("container `web`"),
            "the environment is never named as the container: {message}"
        );
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
        assert_eq!(details["waiting_on"][0]["container"], "ui-custom");
        assert_eq!(
            details["waiting_on"][0]["last_output"],
            "app: GET /health answered 500"
        );
    }

    #[test]
    fn a_spent_health_budget_names_the_environment_honestly_without_a_container() {
        let policy = test_policy();
        let error = super::declared_health_refusal_with_evidence(
            &policy,
            effigy_containers::health::HealthWaitError::NotReady {
                subject: format!(
                    "environment `{}` (primary service `{}`)",
                    policy.name, policy.primary_service
                ),
                check: "tcp://127.0.0.1:9".to_owned(),
                timeout_secs: 60,
            },
            None,
            None,
        );
        let message = error.to_string();
        assert!(
            message.contains("did not become ready within 60s"),
            "{message}"
        );
        // With no container identity, the diagnostic describes the environment
        // and its service instead of calling the environment a container.
        assert!(
            message.contains("environment `web` (primary service `app`)"),
            "{message}"
        );
        assert!(!message.contains("container `web`"), "{message}");
        let details: serde_json::Value =
            serde_json::from_str(error.json_error_details().expect("wait details"))
                .expect("JSON details");
        assert_eq!(details["waiting_on"][0]["answer"], "not_ready");
        assert!(details["waiting_on"][0].get("status").is_none());
        // No container could be read, so the wait claims none instead of
        // guessing one.
        assert!(details["waiting_on"][0]["container"].is_null());
    }

    /// A checkout whose runtime answers one project row, an empty inspect, and
    /// a log tail for the named container. `rows` decides whether the runtime
    /// names the primary container at all.
    fn runtime_fixture(
        name: &str,
        rows: bool,
        logs: bool,
        container: &str,
        project: &str,
    ) -> (effigy_test_support::Fixture, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let fixture = effigy_test_support::Fixture::workspace(name).expect("fixture workspace");
        let root = fixture.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let listing = if rows {
            format!(
                "printf '{container}\\tUp 2 minutes\\t\\t{project}\\t{root}\\tapp\\t\\n'",
                root = root.display(),
            )
        } else {
            "exit 0".to_owned()
        };
        // The primary service reports Running with no health check, so the
        // per-service readiness wait finishes and the declared check is the one
        // under test.
        let inspect = format!(
            "[{{\"Name\":\"/{container}\",\"State\":{{\"Status\":\"running\"}},\"Config\":{{\"Labels\":{{\"com.docker.compose.service\":\"app\"}}}}}}]"
        );
        let log = root.join("calls");
        let logs_answer = if logs {
            "printf 'app: GET /health answered 500\\n'".to_owned()
        } else {
            "echo 'runtime logs unavailable' >&2; exit 1".to_owned()
        };
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  logs*) {logs_answer} ;;\n  inspect*) printf '%s' '{inspect}' ;;\n  *) {listing} ;;\nesac\n",
            log = log.display(),
        );
        for name in ["docker", "colima"] {
            let executable = bin.join(name);
            std::fs::write(&executable, &script).unwrap();
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        std::fs::write(root.join("effigy.toml"), "[tasks]\nbuild = \"true\"\n").unwrap();
        (fixture, root)
    }

    /// One in-process HTTP answer on a loopback port, so the declared check's
    /// probe sees a real status line.
    fn answer_http_once(status_line: &str) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let answer = status_line.to_owned();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 256];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(
                    format!("{answer}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                );
            }
        });
        port
    }

    /// The runtime calls this fixture's fake runtime recorded.
    fn runtime_calls(root: &Path) -> Vec<String> {
        std::fs::read_to_string(root.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn a_definite_health_refusal_names_the_container_the_runtime_reports() {
        let (_fixture, root) = runtime_fixture(
            "effigy-declared-health-container-",
            true,
            true,
            "ui-custom",
            "demo-web-renamed",
        );
        let _env = runtime_env_for(&root);
        let port = answer_http_once("HTTP/1.1 500 Internal Server Error");
        let check = format!("http://127.0.0.1:{port}/health");
        let mut policy = test_policy();
        policy.repo_root = root.clone();
        policy.health_check = Some(check.clone());
        policy.health_timeout_secs = 60;
        let error = super::wait_for_container_ready(&policy, None, None)
            .expect_err("a definite 500 is a terminal refusal");
        // The readiness probe already reported the primary service's container,
        // so the declared wait carries that identity instead of asking the
        // runtime a second time: no bare `ps` (the readiness read is `ps
        // --all`) was issued for it.
        let calls = runtime_calls(&root);
        assert!(
            !calls
                .iter()
                .any(|call| call.starts_with("ps ") && !call.contains("--all")),
            "the identity is carried from the readiness observation: {calls:?}"
        );
        let message = error.to_string();
        assert!(message.contains("answered HTTP 500"), "{message}");
        assert!(
            message.contains("the health check of container `ui-custom`"),
            "the human message names the runtime's container: {message}"
        );
        assert!(
            !message.contains("container `web`"),
            "the environment is never named as the container: {message}"
        );
        assert!(
            message.contains("last container output: app: GET /health answered 500"),
            "{message}"
        );
        let details: serde_json::Value =
            serde_json::from_str(error.json_error_details().expect("wait details"))
                .expect("JSON details");
        assert_eq!(details["failure_kind"], "container_readiness");
        assert_eq!(details["waiting_on"][0]["container"], "ui-custom");
        assert_eq!(details["waiting_on"][0]["answer"], "status_mismatch");
        assert_eq!(details["waiting_on"][0]["status"], 500);
        assert_eq!(
            details["waiting_on"][0]["last_output"],
            "app: GET /health answered 500"
        );
        // Without a readiness observation the same identity still comes from
        // the runtime's row, read under its own live step.
        assert_eq!(
            super::primary_service_container_observed(&policy, None).as_deref(),
            Some("ui-custom"),
            "the runtime's own row names the container, not the Compose default"
        );
    }

    #[test]
    fn the_primary_container_read_names_the_runtime_row_it_waits_on() {
        use std::time::Duration;

        // The readiness wait has finished by the time this read starts, so the
        // frame must name the read itself: a runtime that stalls here is never
        // an idle command (effigy#460, Q-017).
        let policy = test_policy();
        let progress = super::primary_container_progress(&policy);
        assert_eq!(
            progress.frame_at(Duration::from_secs(12)),
            "reading the runtime's row for primary service `app` of `demo-web-renamed`; waiting for the runtime to name the primary container of `demo-web-renamed`; 12s elapsed"
        );
    }

    #[test]
    fn the_declared_health_log_read_names_the_container_and_its_elapsed_wait() {
        use std::time::Duration;

        // The declared check's own step has ended by the time the log tail is
        // read, so the tail names the container it reads: a runtime that stalls
        // here is never an idle command (effigy#460, Q-017).
        let progress = super::declared_health_tail_progress("ui-custom");
        assert_eq!(
            progress.frame_at(Duration::from_secs(12)),
            "reading the last line `ui-custom` printed; waiting for the runtime's log tail of `ui-custom`; 12s elapsed"
        );
    }

    #[test]
    fn an_unavailable_log_read_leaves_the_refusal_without_a_tail() {
        let (_fixture, root) = runtime_fixture(
            "effigy-declared-health-no-logs-",
            true,
            false,
            "ui-custom",
            "demo-web-renamed",
        );
        let _env = runtime_env_for(&root);
        let mut policy = test_policy();
        policy.repo_root = root.clone();
        policy.health_check = Some("http://127.0.0.1:3000/health".to_owned());
        policy.health_timeout_secs = 60;
        assert_eq!(
            super::declared_health_tail(&policy, "ui-custom"),
            None,
            "a log read the runtime does not answer yields no tail"
        );
        let error = super::declared_health_refusal(
            &policy,
            effigy_containers::health::HealthWaitError::StatusMismatch {
                subject: "container `ui-custom`".to_owned(),
                check: "http://127.0.0.1:3000/health".to_owned(),
                status: 500,
                timeout_secs: 60,
            },
            Some("ui-custom".to_owned()),
        );
        let message = error.to_string();
        assert!(message.contains("answered HTTP 500"), "{message}");
        assert!(!message.contains("last container output"), "{message}");
        let details: serde_json::Value =
            serde_json::from_str(error.json_error_details().expect("wait details"))
                .expect("JSON details");
        assert_eq!(details["waiting_on"][0]["container"], "ui-custom");
        assert_eq!(details["waiting_on"][0]["answer"], "status_mismatch");
        assert!(
            details["waiting_on"][0].get("last_output").is_none(),
            "the wait claims no tail it did not read: {details}"
        );
    }

    #[test]
    fn a_health_refusal_without_a_container_names_the_environment_and_service() {
        let (_fixture, root) = runtime_fixture(
            "effigy-declared-health-no-container-",
            false,
            true,
            "ui-custom",
            "demo-web-renamed",
        );
        let _env = runtime_env_for(&root);
        let port = answer_http_once("HTTP/1.1 500 Internal Server Error");
        let check = format!("http://127.0.0.1:{port}/health");
        let mut policy = test_policy();
        policy.repo_root = root.clone();
        policy.health_check = Some(check.clone());
        policy.health_timeout_secs = 60;
        assert_eq!(
            super::primary_service_container_observed(&policy, None),
            None
        );
        let error = super::wait_for_container_ready(&policy, None, None)
            .expect_err("a definite 500 is a terminal refusal");
        let message = error.to_string();
        assert!(
            message.contains("the health check of environment `web` (primary service `app`)"),
            "with no container identity the diagnostic describes the environment: {message}"
        );
        assert!(!message.contains("container `web`"), "{message}");
        assert!(!message.contains("last container output"), "{message}");
        let details: serde_json::Value =
            serde_json::from_str(error.json_error_details().expect("wait details"))
                .expect("JSON details");
        assert!(
            details["waiting_on"][0]["container"].is_null(),
            "no container identity is claimed: {details}"
        );
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
        compose: Vec<u8>,
        inspections: PathBuf,
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
            Self::with_compose(name, project, "services: {}\n", listing)
        }

        /// The same checkout and runtime, with a Compose render that declares
        /// `services`.
        fn with_compose(name: &str, project: &str, compose: &str, listing: &str) -> Self {
            let fixture = effigy_test_support::Fixture::workspace(name).unwrap();
            let root = fixture.path().canonicalize().unwrap();
            let bin = root.join("bin");
            std::fs::create_dir(&bin).unwrap();
            let log = root.join("calls");
            let volumes = root.join("volumes");
            let volume = volumes.join(format!("{project}-data"));
            std::fs::create_dir_all(&volume).unwrap();
            std::fs::write(volume.join("rows"), "kept\n").unwrap();
            // `inspect` answers the test's sequence in `inspections`: the nth
            // call reads `n.json`, and calls past the sequence read `last.json`.
            let inspections = root.join("inspections");
            std::fs::create_dir_all(&inspections).unwrap();
            let script = format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  *'volume rm'*)\n    rm -rf {volumes}\n    ;;\n  *'volume inspect'*)\n    ;;\n  inspect\\ *|*\\ inspect\\ *)\n    n=$(cat {inspections}/count 2>/dev/null || echo 0)\n    n=$((n + 1))\n    echo \"$n\" > {inspections}/count\n    if [ -f {inspections}/$n.json ]; then cat {inspections}/$n.json; else cat {inspections}/last.json 2>/dev/null; fi\n    ;;\n  *'--format'*)\n    {listing}\n    ;;\nesac\n",
                log = log.display(),
                volumes = volumes.display(),
                inspections = inspections.display(),
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
                std::fs::write(directory.join("compose.yml"), compose).unwrap();
            }
            Self {
                _fixture: fixture,
                root,
                volume,
                compose: compose.as_bytes().to_vec(),
                inspections,
            }
        }

        /// The answers `inspect` gives in call order; the last one repeats.
        fn answer_inspections(&self, answers: &[String]) {
            for (index, answer) in answers.iter().enumerate() {
                std::fs::write(self.inspections.join(format!("{}.json", index + 1)), answer)
                    .unwrap();
            }
            if let Some(last) = answers.last() {
                std::fs::write(self.inspections.join("last.json"), last).unwrap();
            }
            std::fs::remove_file(self.inspections.join("count")).ok();
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
                    self.compose.clone(),
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

    /// Each shared service is its own wait: entering it replaces the previous
    /// service's subject and drops the event it ended on, so a stalled stage
    /// never names the service that already finished (effigy#460, Q-017).
    #[test]
    fn each_shared_service_names_its_own_wait() {
        use std::time::Duration;

        let progress = ProgressSource::waiting_for("shared services");

        // Service `db` reaches its restart-policy apply, which is where its
        // wait ends.
        super::begin_shared_service_wait(&progress, "db");
        progress.update(
            "waiting for the machine gate of shared project `demo-db`",
            Some((0, 2)),
            None,
        );
        super::super::steps::restart_policy_stage(
            &progress,
            &super::shared_service_subject("db"),
            super::super::steps::RestartPolicyStage::Apply,
            Some((0, 2)),
        );
        assert!(
            progress
                .frame_at(Duration::from_secs(5))
                .contains("waiting for `restart: unless-stopped` on shared `db`"),
            "{}",
            progress.frame_at(Duration::from_secs(5))
        );
        // The completed service left an event behind for its own wait.
        progress.event("shared `db` is running");

        // Service `cache` is a different wait: its gate frame names `cache` and
        // carries neither `db`'s subject nor its event.
        super::begin_shared_service_wait(&progress, "cache");
        progress.update(
            "waiting for the machine gate of shared project `demo-cache`",
            Some((1, 2)),
            None,
        );
        let frame = progress.frame_at(Duration::from_secs(9));
        assert_eq!(
            frame,
            "waiting for the machine gate of shared project `demo-cache` (1/2); waiting for shared `cache`; 9s elapsed"
        );
        assert!(!frame.contains("shared `db`"), "{frame}");
        assert!(
            !frame.contains("last:"),
            "the previous service's event is cleared: {frame}"
        );
    }

    /// A shared service's step follows the restart-policy apply stage by stage:
    /// the Compose declaration read, the runtime inventory read, then the
    /// update. A stalled read names that read, never the Compose call that
    /// already returned (effigy#460, Q-017).
    #[test]
    fn a_shared_service_step_follows_the_restart_policy_stages() {
        use std::time::Duration;

        let project = "effigy-shared-restart-frame";
        let container = format!("{project}-legacy_mysql-1");
        // The runtime answers both the shared-project inventory this ensure
        // reads and the project inventory the restart-policy apply reads, so
        // the apply reaches its update.
        let listing = format!(
            "case \"$*\" in *com.effigy.shared-render*) printf '%s\\t%s\\t%s\\t%s\\n' '{container}' 'Up 2 minutes' 'this-render' '{project}' ;; *) printf '%s\\t%s\\t\\t%s\\t%s\\tlegacy_mysql\\t\\n' '{container}' 'Up 2 minutes' '{project}' '{project}' ;; esac"
        );
        let fixture = SharedFixture::with_compose(
            "effigy-shared-restart-frame-",
            project,
            "services:\n  legacy_mysql:\n    image: mariadb:latest\n",
            &listing,
        );
        // The apply proves the listed container before it updates it.
        fixture.answer_inspections(&[shared_inspect_record(
            'a',
            &container,
            project,
            "legacy_mysql",
            Some("this-render"),
            true,
        )]);
        let _env = runtime_env(&fixture);
        let policy = fixture.policy(project, "this-render");
        let progress = ProgressSource::waiting_for("shared services");
        let renders = vec![fixture.render("this-render")];
        super::ensure_shared_services_with_renders_observed(&policy, &renders, &progress)
            .expect("a healthy shared service starts");
        // The step ends on the restart-policy update, not the Compose call it
        // already returned from, and keeps the per-service count.
        assert_eq!(
            progress.frame_at(Duration::from_secs(9)),
            "asking the runtime to keep the containers of shared `legacy_mysql` on `restart: unless-stopped` (0/1); waiting for `restart: unless-stopped` on shared `legacy_mysql`; 9s elapsed"
        );
        assert!(
            fixture.calls().iter().any(|call| call.ends_with(&format!(
                "update --restart unless-stopped {}",
                "a".repeat(64)
            ))),
            "the update was sent to the verified full ID: {:?}",
            fixture.calls()
        );
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

    /// One `inspect` record for the shared restart-policy proof: a container
    /// `name` of `project` and `service`, rendered from `render` when one is
    /// given, running or not.
    fn shared_inspect_record(
        id: char,
        name: &str,
        project: &str,
        service: &str,
        render: Option<&str>,
        running: bool,
    ) -> String {
        let mut labels = serde_json::json!({
            "com.docker.compose.project": project,
            "com.docker.compose.service": service,
            "com.docker.compose.oneoff": "False",
        });
        if let Some(render) = render {
            labels["com.effigy.shared-render"] = serde_json::json!(render);
        }
        let status = if running { "running" } else { "exited" };
        serde_json::json!([{
            "Id": id.to_string().repeat(64),
            "Name": format!("/{name}"),
            "State": {"Status": status, "Running": running},
            "Config": {"Labels": labels},
            "NetworkSettings": {"Ports": {}},
        }])
        .to_string()
    }

    /// Runs the shared ensure against a runtime whose `inspect` answers
    /// `answers` in order, with one listed `legacy_mysql` container of `project`
    /// named `name`. Returns the runtime calls, and the frame the restart stage
    /// showed after its update was sent.
    fn shared_restart_run(
        project: &str,
        name: &str,
        answers: &[String],
    ) -> (Vec<String>, String) {
        shared_restart_run_listing(project, name, answers, 1)
    }

    /// As [`shared_restart_run`], with `rows` listed containers of the same
    /// service; more than one makes the service ambiguous.
    fn shared_restart_run_listing(
        project: &str,
        name: &str,
        answers: &[String],
        rows: usize,
    ) -> (Vec<String>, String) {
        use std::time::Duration;

        let restart_row = format!(
            "printf '%s\\t%s\\t\\t%s\\t%s\\tlegacy_mysql\\t\\n' '{name}' 'Up 2 minutes' '{project}' '{project}'; "
        );
        let restart_rows = restart_row.repeat(rows);
        let listing = format!(
            "case \"$*\" in *com.effigy.shared-render*) printf '%s\\t%s\\t%s\\t%s\\n' '{name}' 'Up 2 minutes' 'this-render' '{project}' ;; *) {restart_rows} ;; esac"
        );
        let fixture = SharedFixture::with_compose(
            "effigy-shared-restart-proof-",
            project,
            "services:\n  legacy_mysql:\n    image: mariadb:latest\n",
            &listing,
        );
        fixture.answer_inspections(answers);
        let _env = runtime_env(&fixture);
        let policy = fixture.policy(project, "this-render");
        let progress = ProgressSource::waiting_for("shared services");
        let renders = vec![fixture.render("this-render")];
        super::ensure_shared_services_with_renders_observed(&policy, &renders, &progress)
            .expect("a healthy shared service starts; the restart policy is best-effort");
        let frame = progress.frame_at(Duration::from_secs(9));
        (fixture.calls(), frame)
    }

    #[test]
    fn a_shared_service_with_two_listed_containers_is_ambiguous_and_receives_no_update() {
        let project = "effigy-shared-restart-ambiguous";
        let name = format!("{project}-legacy_mysql-1");
        let verified = shared_inspect_record('a', &name, project, "legacy_mysql", Some("this-render"), true);
        let (calls, _) = shared_restart_run_listing(project, &name, &[verified.clone(), verified], 2);

        assert!(restart_updates(&calls).is_empty(), "{calls:?}");
    }

    /// The restart-policy updates a run sent, in order.
    fn restart_updates(calls: &[String]) -> Vec<String> {
        calls
            .iter()
            .filter(|call| call.contains("update --restart"))
            .cloned()
            .collect()
    }

    #[test]
    fn a_shared_restart_update_names_only_the_verified_full_id() {
        let project = "effigy-shared-restart-verified";
        let name = format!("{project}-legacy_mysql-1");
        let verified = shared_inspect_record('a', &name, project, "legacy_mysql", Some("this-render"), true);
        let (calls, _) = shared_restart_run(project, &name, &[verified.clone(), verified]);

        let updates = restart_updates(&calls);
        assert_eq!(updates.len(), 1, "one verified update: {calls:?}");
        assert!(
            updates[0].ends_with(&format!("update --restart unless-stopped {}", "a".repeat(64))),
            "the update names the verified full ID, not the listed name: {updates:?}"
        );
        assert!(!updates[0].contains(&name), "{updates:?}");
    }

    #[test]
    fn a_same_name_container_of_a_foreign_project_receives_no_shared_update() {
        let project = "effigy-shared-restart-foreign";
        let name = format!("{project}-legacy_mysql-1");
        let foreign = shared_inspect_record('a', &name, "effigy-other-project", "legacy_mysql", Some("this-render"), true);
        let (calls, _) = shared_restart_run(project, &name, &[foreign.clone(), foreign]);

        assert!(restart_updates(&calls).is_empty(), "{calls:?}");
    }

    #[test]
    fn a_container_rendered_from_a_stale_render_receives_no_shared_update() {
        let project = "effigy-shared-restart-stale";
        let name = format!("{project}-legacy_mysql-1");
        let stale = shared_inspect_record('a', &name, project, "legacy_mysql", Some("previous-render"), true);
        let (calls, _) = shared_restart_run(project, &name, &[stale.clone(), stale]);

        assert!(restart_updates(&calls).is_empty(), "{calls:?}");
    }

    #[test]
    fn a_shared_container_whose_full_id_changes_between_inspections_receives_no_update() {
        let project = "effigy-shared-restart-changed";
        let name = format!("{project}-legacy_mysql-1");
        let first = shared_inspect_record('a', &name, project, "legacy_mysql", Some("this-render"), true);
        let replaced = shared_inspect_record('b', &name, project, "legacy_mysql", Some("this-render"), true);
        let (calls, _) = shared_restart_run(project, &name, &[first, replaced]);

        assert!(restart_updates(&calls).is_empty(), "{calls:?}");
    }

    #[test]
    fn a_shared_container_with_no_inspect_identity_receives_no_update() {
        let project = "effigy-shared-restart-missing";
        let name = format!("{project}-legacy_mysql-1");
        let (calls, _) = shared_restart_run(project, &name, &["[]".to_owned()]);

        assert!(restart_updates(&calls).is_empty(), "{calls:?}");
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
            "effigy container web up",
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
