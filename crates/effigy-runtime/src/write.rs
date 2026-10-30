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
pub type ShutdownEffect<'a> = &'a dyn Fn() -> Result<(), EffigyRuntimeError>;
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
            transition(repo_root, &policy, &|| Ok(()))?;
            let removed = deregister_gateway_routes(&policy)?;
            let mut report = down_noop_report(&policy, &reason);
            annotate_removed_gateway_routes(&mut report, &removed);
            return Ok(render_container_report(report, output_json));
        }
    }
    let runtime_running = runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let prepared = runtime_running && prepare_shutdown(repo_root, &policy)?;
    // One step per service, created and moved by the runtime's own stopping and
    // removing lines while it works, so a slow stop names the service holding
    // it up (effigy#313). A runtime that narrates nothing still gets one honest
    // step, and a step it never finished is closed from what the inventory
    // still holds.
    let compose_steps = std::cell::RefCell::new(effigy_containers::exec::ComposeSteps::new(
        policy.project_name.clone(),
    ));
    // A runtime that narrates nothing this parser recognises still gets one
    // live step, so an operator is never looking at a silent teardown.
    let whole_stack = prepared.then(|| {
        effigy_containers::exec::runtime_wait(format!("stopping the `{}` stack", policy.name))
    });
    let stopped = transition(repo_root, &policy, &|| {
        if prepared {
            run_shutdown_compose_watched(repo_root, &policy, &mut |line| {
                if let Some(event) = effigy_containers::exec::parse_compose_step_event(line) {
                    compose_steps.borrow_mut().observe(&event);
                }
            })?;
        }
        Ok(())
    });
    if let Some(whole_stack) = whole_stack {
        finish_whole_stack(
            &whole_stack,
            whole_stack_ending(compose_steps.borrow().touched(), &stopped),
        );
    }

    // A step the runtime never closed is closed from the post-teardown
    // inventory: gone means removed; still listed means still present.
    if prepared {
        let remaining =
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
        compose_steps
            .borrow()
            .finish_unfinished_teardown(remaining.as_deref());
    }
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
        |_repo_root, _policy, effect| effect(),
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
        transition(repo_root, &policy, &|| {
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
        transition(repo_root, &policy, &|| {
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

/// How the whole-stack step ends when the runtime's own step lines did not
/// carry the teardown (effigy#313). Pure, so the choice is testable without a
/// terminal or a runtime.
#[derive(Debug, PartialEq, Eq)]
enum WholeStackEnding {
    Stopped,
    Failed(String),
    Cleared,
}

/// `touched` is how many subjects the runtime's recognised lines moved.
fn whole_stack_ending(
    touched: usize,
    stopped: &Result<(), EffigyRuntimeError>,
) -> WholeStackEnding {
    match (touched, stopped) {
        // The runtime printed nothing this parser recognises. The teardown's
        // own result is the only evidence, so one step says what happened.
        (0, Ok(())) => WholeStackEnding::Stopped,
        (0, Err(error)) => WholeStackEnding::Failed(format!("the stack did not stop: {error}")),
        // The per-service steps carry the story; the fallback had none.
        _ => WholeStackEnding::Cleared,
    }
}

fn finish_whole_stack(whole_stack: &effigy_containers::exec::TimedStep, ending: WholeStackEnding) {
    match ending {
        WholeStackEnding::Stopped => whole_stack.succeed("the runtime reported the stack stopped"),
        WholeStackEnding::Failed(cause) => whole_stack.fail(cause),
        WholeStackEnding::Cleared => whole_stack.finish_clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::{finish_whole_stack, whole_stack_ending, WholeStackEnding};
    use crate::EffigyRuntimeError;
    use effigy_containers::exec::{runtime_wait, ComposeStepEvent, ComposeSteps};

    #[test]
    fn a_silent_runtime_teardown_step_names_the_phase_its_report_and_elapsed_time() {
        let step = runtime_wait("stopping the `web` stack");
        let frame = step.label();
        assert!(
            frame.starts_with("stopping the `web` stack; waiting for the runtime's next report; "),
            "{frame}"
        );
        assert!(frame.ends_with("s elapsed"), "{frame}");
    }

    #[test]
    fn a_stop_the_runtime_never_narrated_ends_the_stack_step_with_a_check() {
        let (_, lines) = effigy_ui::steps::capture_lines(|| {
            let ending = whole_stack_ending(0, &Ok(()));
            assert_eq!(ending, WholeStackEnding::Stopped);
            finish_whole_stack(&runtime_wait("stopping the `web` stack"), ending);
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("✓ the runtime reported the stack stopped"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_failed_stop_the_runtime_never_narrated_ends_the_stack_step_with_a_cross_and_its_cause() {
        let stopped = Err(EffigyRuntimeError::task_invocation(
            "docker compose down exited 1",
        ));
        let (_, lines) = effigy_ui::steps::capture_lines(|| {
            let ending = whole_stack_ending(0, &stopped);
            assert_eq!(
                ending,
                WholeStackEnding::Failed(
                    "the stack did not stop: docker compose down exited 1".to_owned()
                )
            );
            finish_whole_stack(&runtime_wait("stopping the `web` stack"), ending);
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("✗ the stack did not stop: docker compose down exited 1"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_recognised_service_move_clears_the_stack_step_and_inventory_ends_the_service_step() {
        let (_, lines) = effigy_ui::steps::capture_lines(|| {
            let mut steps = ComposeSteps::new("demo-web-dev");
            steps.observe(&ComposeStepEvent::Stopping {
                subject: "demo-web-dev-web-1".to_owned(),
            });
            steps.observe(&ComposeStepEvent::Removing {
                subject: "demo-web-dev-web-1".to_owned(),
            });
            let ending = whole_stack_ending(steps.touched(), &Ok(()));
            assert_eq!(ending, WholeStackEnding::Cleared);
            finish_whole_stack(&runtime_wait("stopping the `web` stack"), ending);
            steps.finish_unfinished_teardown(Some(&["demo-web-dev-web-1".to_owned()]));
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("✗ web is still present after the teardown"),
            "{lines:?}"
        );

        let (_, lines) = effigy_ui::steps::capture_lines(|| {
            let mut steps = ComposeSteps::new("demo-web-dev");
            steps.observe(&ComposeStepEvent::Removing {
                subject: "demo-web-dev-web-1".to_owned(),
            });
            let ending = whole_stack_ending(steps.touched(), &Ok(()));
            finish_whole_stack(&runtime_wait("stopping the `web` stack"), ending);
            steps.finish_unfinished_teardown(Some(&[]));
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("✓ stopped and removed web"),
            "{lines:?}"
        );
    }
}
