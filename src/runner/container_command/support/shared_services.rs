//! Bringing the shared data services an environment depends on up to date.
//!
//! A shared service belongs to no checkout: its ensure takes the one
//! machine-wide gate for that Compose project, proves the runtime's containers
//! against the render about to be applied, and only then publishes and starts
//! it (effigy#377, ef5780da).

use std::ffi::OsString;

use effigy_containers::EffectiveContainerPolicy;
use effigy_ui::steps::{ProgressFrame, ProgressSource};

use crate::runner::container_command::RunnerError;

use super::shared_compose::{
    run_shared_compose_capture, shared_compose_args, SHARED_COMPOSE_TIMEOUT,
};

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
pub(super) fn ensure_shared_services_with_renders(
    policy: &EffectiveContainerPolicy,
    renders: &[effigy_containers::SharedRender],
) -> Result<Vec<String>, RunnerError> {
    ensure_shared_services_with_renders_observed(
        policy,
        renders,
        &ProgressSource::waiting_for("shared services"),
    )
}

pub(super) fn ensure_shared_services_with_renders_observed(
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
        // Held across inventory, publication, replacement and apply.
        progress.update(|_| {
            ProgressFrame::new(format!(
                "waiting for the machine gate of shared project `{}`",
                service.project_name
            ))
            .count(Some((done, total)))
            .label("Waiting for shared service lock")
            .waiting_for(shared_service_subject(&service.service_name))
        });
        let _gate = crate::runner::task_engine::shared_service_gate(&service.project_name)
            .map_err(|error| shared_service_refusal(policy, service, "machine gate", error))?;
        // One inventory read serves the refusal and the replacement below.
        progress.update(|_| {
            ProgressFrame::new(format!(
                "reading the runtime's containers for shared `{}`",
                service.service_name
            ))
            .count(Some((done, total)))
            .label("Checking shared containers")
        });
        // Ok(empty) proves absence; Ok(rows) is verified inventory; Err is
        // unknown and refuses before any publication or runtime mutation.
        let containers = effigy_containers::exec::shared_project_containers(
            &policy.repo_root,
            policy,
            &service.project_name,
        )
        .map_err(|error| shared_service_refusal(policy, service, "inventory", error.into()))?;
        refuse_vanished_shared_render(policy, service, &containers)
            .map_err(|error| shared_service_refusal(policy, service, "render check", error))?;
        if let Some(render) = renders
            .iter()
            .find(|render| Some(render.directory.as_path()) == service.compose_file.parent())
        {
            progress.update(|_| {
                ProgressFrame::new(format!(
                    "publishing the render of shared `{}`",
                    service.service_name
                ))
                .count(Some((done, total)))
                .label("Saving shared stack")
            });
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
        progress.update(|_| {
            ProgressFrame::new(format!(
                "checking the render of shared `{}` against its container",
                service.service_name
            ))
            .count(Some((done, total)))
            .label("Checking shared stack")
        });
        let mut replaced = false;
        for container in containers
            .iter()
            .filter(|container| container.render.as_deref() != Some(service.render_digest.as_str()))
        {
            for (phase, args) in [
                (
                    "replacement stop",
                    vec!["stop", container.container_name.as_str()],
                ),
                (
                    "replacement remove",
                    vec!["rm", "-f", container.container_name.as_str()],
                ),
            ] {
                effigy_containers::exec::run_runtime_command_capture_for_policy_with_timeout(
                    &policy.repo_root,
                    policy,
                    &args.into_iter().map(OsString::from).collect::<Vec<_>>(),
                    phase,
                    SHARED_COMPOSE_TIMEOUT,
                )
                .map_err(|error| shared_service_refusal(policy, service, phase, error.into()))?;
            }
            replaced = true;
        }
        let workdir = service.compose_file.parent().ok_or_else(|| {
            RunnerError::task_invocation("shared service compose file has no parent directory")
        })?;
        progress.update(|_| {
            ProgressFrame::new(format!(
                "waiting for docker compose up (shared {}) to return",
                service.service_name
            ))
            .count(Some((done, total)))
            .label("Starting shared service")
        });
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
        let mut restart_stage =
            |stage: crate::runner::container_command::steps::RestartPolicyStage| {
                crate::runner::container_command::steps::restart_policy_stage(
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
pub(super) fn shared_service_subject(service_name: &str) -> String {
    format!("shared `{service_name}`")
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

/// Refuse a shared service that would start against a missing render (ef5780da).
///
/// A container whose own render directory is gone has bind sources that no
/// longer exist, so starting or replacing it fails after a stop or remove. That
/// is refused before any publish, stop, remove or Compose call, whichever render
/// the container was created from. A missing render with no container is the
/// normal creation path. Inventory must already be verified before this check.
fn refuse_vanished_shared_render(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
    containers: &[effigy_containers::exec::SharedProjectContainer],
) -> Result<(), RunnerError> {
    let current_present = service.compose_file.is_file();
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
