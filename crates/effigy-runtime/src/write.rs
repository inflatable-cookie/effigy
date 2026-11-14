mod planning;
mod report;

use std::path::Path;

use effigy_catalog::volumes::classify_for_reset;
use effigy_containers::{
    down_noop_report, down_report, exec::runtime_backend_is_running, load_container_policy,
    reset_report, validate_compose_backend_runtime, validate_container_policy,
    EffectiveComposeSource, EffectiveContainerPolicy,
};
use effigy_containers::{ContainerAction, ContainerCleanupResult, ContainerRuntimeState};
use effigy_ui::steps::{ProgressFrame, ProgressSource};

use crate::container_manager::{compose_invocation_plan, lifecycle_operation_report};
use crate::read::{discover_running_environments, filter_running_environments_for_scope};
use crate::signals::run_compose_plan_capture;
use crate::EffigyRuntimeError;
pub use planning::select_generated_service_image_refs;
use planning::{
    can_prepare_shutdown, prepare_shutdown, remove_generated_service_images, run_shutdown_compose,
    run_shutdown_compose_watched,
};

/// The caller's environment transition around one shutdown: it receives the
/// backend effect and must run it at most once, under the environment's gate
/// (contract 053 §3). The runner journals it; tests may call it directly.
///
/// The effect reports into the transition's progress: the transition owns the
/// stop's single step, and the runtime's stopping/removing lines arrive as
/// that step's sub-step text (effigy#974).
pub type ShutdownEffect<'a> = &'a dyn Fn(&ProgressSource) -> Result<(), EffigyRuntimeError>;
use report::{
    annotate_left_running_shared_services, annotate_removed_gateway_routes,
    render_container_down_global_report, render_container_report, StoppedContainerEnvironment,
};

pub fn run_container_down<F, T>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    deregister_gateway_routes: F,
    transition: T,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    T: Fn(&Path, &EffectiveContainerPolicy, ShutdownEffect<'_>) -> Result<(), EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    // A never-prepared environment with no project resources has nothing to
    // stop, and its unassigned ports must not reach Compose (effigy#286).
    // An empty Compose project is not proof that host integrations are down.
    // A declared host process still needs the ordinary whole-environment stop.
    if policy.host_processes.is_empty() {
        if let Some(reason) = effigy_containers::environment::nothing_to_stop(repo_root, &policy) {
            transition(repo_root, &policy, &|_| Ok(()))?;
            let removed = deregister_gateway_routes(&policy)?;
            let mut report = down_noop_report(&policy, &reason);
            annotate_removed_gateway_routes(&mut report, &removed);
            return Ok(render_container_report(report, output_json));
        }
    }
    let runtime_running = runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let prepared = runtime_running && prepare_shutdown(repo_root, &policy)?;
    // The stop is one line: the environment transition owns the step, and the
    // runtime's own stopping and removing lines arrive as that step's sub-step
    // text while it works, so a slow stop still names the service holding it
    // up (effigy#313, effigy#974). A runtime that narrates nothing this parser
    // recognises leaves the transition's own `stopping the backend` sub-step,
    // which still names the teardown it awaits with its elapsed wait.
    let stopped = transition(repo_root, &policy, &|progress| {
        if prepared {
            let started = std::time::Instant::now();
            let project = policy.project_name.clone();
            run_shutdown_compose_watched(repo_root, &policy, &mut |line| {
                if let Some(sub_step) = shutdown_sub_step(&project, line) {
                    progress.update(|_| ProgressFrame::new(sub_step));
                }
            })?;
            // A teardown the runtime reports as done is only done when its
            // containers are gone. Survivors warn on the operation, never a
            // second step and never a failed stop: the backend exit is the
            // authority, and the transition already published stopped.
            // (Review on effigy#974: dropping the old inventory read made a
            // `compose down` that left a container behind read as clean.)
            // An inventory that cannot be read warns too: it confirms
            // nothing, so it never reads as empty (second review).
            let inventory =
                effigy_containers::exec::list_compose_containers_for_policy_including_stopped(
                    repo_root, &policy,
                )
                .ok()
                .map(|rows| {
                    rows.into_iter()
                        .filter(|row| !row.oneoff)
                        .map(|row| row.container_name)
                        .collect::<Vec<_>>()
                });
            if let Some(note) =
                teardown_inventory_note(&policy.project_name, inventory, started.elapsed())
            {
                eprintln!("{note}");
            }
        }
        Ok(())
    });
    stopped?;
    let _manager_report = lifecycle_operation_report(
        repo_root,
        &policy,
        ContainerAction::Shutdown,
        if runtime_running {
            ContainerRuntimeState::Stopped
        } else {
            ContainerRuntimeState::Unknown
        },
        Some(if runtime_running {
            ContainerCleanupResult::Completed
        } else {
            ContainerCleanupResult::NotRequested
        }),
    )?;
    let removed_gateway_domains = deregister_gateway_routes(&policy)?;
    let mut report = down_report(&policy, runtime_running);
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

pub fn run_container_down_all<F>(
    output_json: bool,
    deregister_gateway_routes: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
{
    run_container_down_all_with_hook(
        output_json,
        deregister_gateway_routes,
        |_repo_root, _policy, effect| {
            effect(&ProgressSource::waiting_for("the test backend to stop"))
        },
    )
}

pub fn run_container_down_all_with_hook<F, T>(
    output_json: bool,
    deregister_gateway_routes: F,
    transition: T,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    T: Fn(&Path, &EffectiveContainerPolicy, ShutdownEffect<'_>) -> Result<(), EffigyRuntimeError>,
{
    let environments = discover_running_environments()?;
    run_container_down_environments(
        environments,
        output_json,
        deregister_gateway_routes,
        transition,
    )
}

pub fn run_container_down_under_path_with_hook<F, T>(
    scope_root: &Path,
    name: Option<&str>,
    output_json: bool,
    deregister_gateway_routes: F,
    transition: T,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    T: Fn(&Path, &EffectiveContainerPolicy, ShutdownEffect<'_>) -> Result<(), EffigyRuntimeError>,
{
    let environments =
        filter_running_environments_for_scope(discover_running_environments()?, scope_root, name);
    run_container_down_environments(
        environments,
        output_json,
        deregister_gateway_routes,
        transition,
    )
}

/// Stops every discovered environment in turn, each through the caller's
/// transition, which owns the environment's gate, holder check and any
/// host-side teardown before the backend effect.
fn run_container_down_environments<F, T>(
    environments: Vec<crate::read::DiscoveredRunningEnvironment>,
    output_json: bool,
    deregister_gateway_routes: F,
    transition: T,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    T: Fn(&Path, &EffectiveContainerPolicy, ShutdownEffect<'_>) -> Result<(), EffigyRuntimeError>,
{
    let mut stopped = Vec::new();

    for environment in environments {
        let repo_root = Path::new(&environment.repo_root);
        let policy = environment.policy;
        let runtime_running = runtime_backend_is_running(&policy, repo_root)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        let prepared = runtime_running && prepare_shutdown(repo_root, &policy)?;
        transition(repo_root, &policy, &|_| {
            if prepared {
                run_shutdown_compose(repo_root, &policy)?;
            }
            Ok(())
        })?;
        let _manager_report = lifecycle_operation_report(
            repo_root,
            &policy,
            ContainerAction::Shutdown,
            if runtime_running {
                ContainerRuntimeState::Stopped
            } else {
                ContainerRuntimeState::Unknown
            },
            Some(if runtime_running {
                ContainerCleanupResult::Completed
            } else {
                ContainerCleanupResult::NotRequested
            }),
        )?;
        let removed_gateway_domains = deregister_gateway_routes(&policy)?;
        stopped.push(StoppedContainerEnvironment {
            repo_root: environment.repo_root,
            container: policy.name.clone(),
            project_name: policy.project_name.clone(),
            profile: policy.profile.clone(),
            removed_gateway_domains,
            left_running_shared_services: policy
                .shared_services
                .iter()
                .map(|service| service.service_name.clone())
                .collect(),
            runtime_was_running: runtime_running,
        });
    }

    render_container_down_global_report(&stopped, output_json)
}

/// `transition` is the environment's reset transition: it stops the
/// environment through the backend effect and drops only this
/// environment's render references (never a sibling's, D3-07).
#[allow(clippy::too_many_arguments)]
pub fn run_container_reset<FDeregister, FRemoveVolumes, T>(
    repo_root: &Path,
    name: Option<&str>,
    _keep_data: bool,
    wipe_data: bool,
    output_json: bool,
    deregister_gateway_routes: FDeregister,
    remove_reset_volumes: FRemoveVolumes,
    transition: T,
) -> Result<String, EffigyRuntimeError>
where
    FDeregister: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    FRemoveVolumes: Fn(
        &Path,
        &EffectiveContainerPolicy,
        &effigy_catalog::volumes::VolumeClassification,
    ) -> Result<(), EffigyRuntimeError>,
    T: Fn(&Path, &EffectiveContainerPolicy, ShutdownEffect<'_>) -> Result<(), EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    if _keep_data && wipe_data {
        return Err(EffigyRuntimeError::task_invocation(
            "`reset` does not accept both `--keep-data` and `--wipe-data`",
        ));
    }
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let runtime_running = runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let can_reset = can_prepare_shutdown(repo_root, &policy)?;
    let policy = if can_reset {
        effigy_containers::policy::render::prepare_resolved(&policy)
            .map_err(|e| EffigyRuntimeError::task_invocation(e.to_string()))?
    } else {
        policy
    };
    let preserve_persistent_data = !wipe_data;
    let volume_actions = if policy.compose_source == EffectiveComposeSource::Generated {
        Some(classify_for_reset(
            &policy.managed_volumes,
            preserve_persistent_data,
        ))
    } else {
        None
    };
    // Image references come from the render before the reset drops it.
    let image_policy = policy.clone();
    if can_reset {
        transition(repo_root, &policy, &|_| {
            if !runtime_running {
                return Ok(());
            }
            let tail: &[&str] = if preserve_persistent_data {
                &["down", "--remove-orphans"]
            } else {
                &["down", "-v", "--remove-orphans"]
            };
            let plan = compose_invocation_plan(
                repo_root,
                &policy,
                tail.iter().copied(),
                ContainerAction::Shutdown,
                if preserve_persistent_data {
                    "docker compose down"
                } else {
                    "docker compose down -v"
                },
            )?;
            run_compose_plan_capture(&plan)?;
            if preserve_persistent_data {
                if let Some(classification) = volume_actions.as_ref() {
                    remove_reset_volumes(repo_root, &policy, classification)?;
                }
            }
            Ok(())
        })?;
    }
    let removed_gateway_domains = if can_reset {
        remove_generated_service_images(repo_root, &image_policy)?;
        deregister_gateway_routes(&policy)?
    } else {
        Vec::new()
    };
    let mut report = reset_report(
        &policy,
        runtime_running,
        preserve_persistent_data,
        wipe_data,
        volume_actions.as_ref(),
    );
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

/// The runtime's own stopping/removing lines arrive as the transition's
/// sub-step text (see `run_container_down`): they open no step of their own,
/// so a stop is one line that moves while the backend works (effigy#974).
///
/// Pure over recognised lines, so the mapping is testable without a terminal
/// or a runtime: each recognised line replaces the transition's sub-step,
/// and an unrecognised line leaves the transition's own sub-step alone.
fn shutdown_sub_step(project: &str, line: &str) -> Option<String> {
    effigy_containers::exec::parse_compose_step_event(line).map(|event| event.label(project))
}

/// What the post-teardown inventory says about the stop, as a warning on the
/// operation: names every surviving service, the teardown's elapsed time
/// and the commands that inspect it. `None` when the inventory is empty, so
/// a clean `compose down` stays a success with no second line. An inventory
/// that cannot be read warns too: it confirms nothing, so it never reads as
/// empty (second review on effigy#974). Neither case fails the stop: the
/// backend exit is the authority, and the transition already published
/// stopped (a handover fake lists its peer unconditionally, and a real
/// inventory can lag the removal).
///
/// Pure over the post-teardown inventory, so the wording is testable
/// without a terminal or a runtime.
fn teardown_inventory_note(
    project: &str,
    inventory: Option<Vec<String>>,
    elapsed: std::time::Duration,
) -> Option<String> {
    let Some(remaining) = inventory else {
        return Some(format!(
            "[warn] the stop teardown ran but its containers could not be listed ({}); confirm the stack is down with `effigy container status`.",
            effigy_ui::human::human_duration(elapsed),
        ));
    };
    if remaining.is_empty() {
        return None;
    }
    let mut services = remaining
        .iter()
        .map(|container| effigy_containers::exec::service_from_container_name(project, container))
        .collect::<Vec<_>>();
    services.sort();
    services.dedup();
    let held = services.join(", ");
    let (is_are, it_them) = if services.len() == 1 {
        ("is", "it")
    } else {
        ("are", "them")
    };
    Some(format!(
        "[warn] {held} {is_are} still present after the teardown ({}); inspect {it_them} with `effigy container status` and `effigy container logs --service {}`. The environment stays in a state its own `effigy container up` or `effigy container down` recovers from.",
        effigy_ui::human::human_duration(elapsed),
        services.first().expect("a survivor names its service"),
    ))
}

#[cfg(test)]
mod tests {
    use super::{shutdown_sub_step, teardown_inventory_note};
    use effigy_ui::steps::{ProgressFrame, ProgressSource};

    /// The runtime's stopping/removing lines move the transition's sub-step;
    /// they open no step of their own, so a stop stays one line (effigy#974).
    #[test]
    fn recognised_teardown_lines_become_the_transition_sub_step() {
        let progress = ProgressSource::new(ProgressFrame::new("stopping the backend"));
        let project = "demo-web-dev";
        for (line, expected) in [
            (" Container demo-web-dev-web-1  Stopping", "stopping web"),
            (" Container demo-web-dev-web-1  Stopped", "stopping web"),
            (" Container demo-web-dev-web-1  Removing", "removing web"),
        ] {
            let label = shutdown_sub_step(project, line).expect("a recognised line");
            assert_eq!(label, expected, "{line}");
            progress.update(|_| ProgressFrame::new(label));
        }
        assert_eq!(
            progress.frame_at(std::time::Duration::from_secs(8)),
            "removing web · 8s"
        );
    }

    /// A line the parser does not recognise changes nothing: the
    /// transition's own `stopping the backend` sub-step still names the
    /// teardown it awaits, so an operator never watches a silent stop.
    #[test]
    fn an_unrecognised_teardown_line_leaves_the_transition_sub_step() {
        assert_eq!(
            shutdown_sub_step("demo-web-dev", "some unparsed runtime line"),
            None
        );
        let progress = ProgressSource::new(ProgressFrame::new("stopping the backend"));
        assert_eq!(
            progress.frame_at(std::time::Duration::from_secs(8)),
            "stopping the backend · 8s"
        );
        let (_, lines) = effigy_ui::steps::capture_lines(|| {
            let _source = progress;
        });
        assert!(lines.is_empty(), "no step of its own: {lines:?}");
    }

    /// An empty post-teardown inventory is a success with no second line.
    #[test]
    fn an_empty_post_teardown_inventory_is_no_note() {
        assert!(teardown_inventory_note(
            "demo-web-dev",
            Some(Vec::new()),
            std::time::Duration::from_secs(3)
        )
        .is_none());
    }

    /// An inventory that cannot be read warns too: it confirms nothing, so
    /// the stop never reports cleanly on an unchecked claim.
    #[test]
    fn an_unreadable_post_teardown_inventory_warns() {
        let note = teardown_inventory_note("demo-web-dev", None, std::time::Duration::from_secs(4))
            .expect("an unreadable inventory warns");
        assert!(
            note.contains(
                "[warn] the stop teardown ran but its containers could not be listed (4s)"
            ),
            "{note}"
        );
        assert!(note.contains("`effigy container status`"), "{note}");
    }

    /// Survivors warn on the operation without failing the stop: the note
    /// names every surviving service, the teardown's elapsed time and the
    /// commands that inspect it, so a `compose down` that left a container
    /// behind never reads as clean. The stop itself stays a success: the
    /// backend exit is the authority.
    #[test]
    fn surviving_containers_warn_with_their_services() {
        let note = teardown_inventory_note(
            "demo-web-dev",
            Some(vec![
                "demo-web-dev-web-1".to_owned(),
                "demo-web-dev-db-1".to_owned(),
            ]),
            std::time::Duration::from_secs(11),
        )
        .expect("survivors warn");
        assert!(
            note.contains("[warn] db, web are still present after the teardown"),
            "{note}"
        );
        assert!(note.contains("11s"), "{note}");
        assert!(note.contains("`effigy container status`"), "{note}");
        assert!(
            note.contains("`effigy container logs --service db`"),
            "{note}"
        );

        let single = teardown_inventory_note(
            "demo-web-dev",
            Some(vec!["demo-web-dev-web-1".to_owned()]),
            std::time::Duration::from_secs(11),
        )
        .expect("a single survivor warns");
        assert!(
            single.contains("[warn] web is still present after the teardown"),
            "{single}"
        );
    }
}
