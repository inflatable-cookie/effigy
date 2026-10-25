use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};

/// Bound for one exec readiness attempt: ownership re-inspection plus the probe
/// exec itself. The readiness window only decides whether to start another
/// attempt; it never cuts off an attempt that is already running.
const EXEC_READY_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

/// Retry window for callers that need a ready binding but supply no deadline.
const EXEC_READY_RETRY_WINDOW: Duration = Duration::from_secs(2);

/// The window a saturated host gets for exec readiness before Effigy considers
/// a restart. One bounded attempt can spend the whole short window on a
/// round trip the host never scheduled; a restart is not the repair for that,
/// because the service was never given a chance to answer (effigy#291).
const EXEC_READY_SATURATED_RETRY_WINDOW: Duration = Duration::from_secs(15);

mod gateway;
mod prep;
mod running;
mod validation;

use effigy_containers::compose::compose_args;
use effigy_containers::exec::{
    ensure_colima_running, recover_exited_owned_compose_services, run_compose_capture,
};
use effigy_containers::host_load::HostLoad;
use effigy_containers::{load_container_exec_working_dir, EffectiveContainerPolicy};
use effigy_runtime_plan::{
    RuntimeActivationPlan, RuntimeActivationRequest, RuntimeActivationRoute, RuntimeLeasePolicy,
};
#[cfg(test)]
use effigy_runtime_plan::{RuntimeActivationReport, RuntimeCleanupResult};

pub(in crate::runner) use gateway::container_policy_uses_gateway_surface;
use gateway::ensure_runtime_gateway_readiness_stage;
#[cfg(test)]
use gateway::ensure_runtime_gateway_readiness_stage_using;
use prep::{
    ensure_runtime_exec_readiness_stage, prepare_runtime_mounts_stage,
    reconcile_runtime_aliases_stage, run_runtime_compose_up_stage,
};
#[cfg(test)]
use prep::{ensure_runtime_exec_readiness_stage_using, reconcile_runtime_aliases_stage_using};
use running::{check_runtime_running_state_stage, ensure_runtime_running_stage};
use validation::validate_policy_runtime;
#[cfg(test)]
use validation::validate_runtime_activation_stage;

use crate::runner::environment::Held;
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};
use effigy_containers::environment::Retention;
use effigy_kernel::id::RunId;

// Inventory and strict inspection require separate runtime transport hops. They
// must not consume the short workload probe's execution window. Explicit caller
// deadlines still bound both phases without being extended.
pub(in crate::runner) const OWNED_EXEC_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub(in crate::runner) struct ContainerTaskActivation {
    /// The immutable snapshot used by every activation stage and its exec caller.
    pub(in crate::runner) policy: EffectiveContainerPolicy,
    pub(in crate::runner) system_was_running: bool,
    /// This activation started a held environment: it stops when the last
    /// run using it ends.
    pub(in crate::runner) started_held: bool,
    /// The command's hold on its environment, released when the activation
    /// drops at the end of the command, or earlier by a command whose own
    /// closeout stops the environment.
    pub(in crate::runner) hold: Option<Held>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ActivationRequest<'a> {
    pub(in crate::runner) container_name: Option<&'a str>,
    pub(in crate::runner) repo_override: Option<PathBuf>,
    pub(in crate::runner) route: RuntimeActivationRoute,
    pub(in crate::runner) session_context: RuntimeSessionContext,
}

pub(in crate::runner) fn ensure_container_runtime_prepared(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    route: RuntimeActivationRoute,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    start: StartPolicy<'_>,
) -> Result<bool, RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    if route == RuntimeActivationRoute::Exec {
        return ensure_exec_container_running_with(policy, container_name, || {
            check_runtime_running_state_stage(repo_root, policy)
        });
    }
    let system_was_running = check_runtime_running_state_stage(repo_root, policy)?;
    ensure_runtime_running_stage(
        system_was_running,
        container_name.map(str::to_owned),
        repo_override,
        |name, repo_override| {
            crate::runner::container_command::run_container_up_held(
                repo_override,
                name,
                start.retention,
                start.holders,
            )
        },
    )?;

    prepare_container_exec_runtime(
        repo_root,
        policy,
        container_name.or(Some(policy.name.as_str())),
        start,
    )?;
    Ok(system_was_running)
}

/// How an activation starts or reconciles a stack: the retention a stack it
/// starts gets, and the runs the activation attaches.
#[derive(Debug, Clone, Copy)]
pub(in crate::runner) struct StartPolicy<'a> {
    pub(in crate::runner) retention: Retention,
    pub(in crate::runner) holders: &'a [RunId],
}

fn ensure_exec_container_running_with(
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    is_running: impl FnOnce() -> Result<bool, RunnerError>,
) -> Result<bool, RunnerError> {
    if is_running()? {
        return Ok(true);
    }
    Err(RunnerError::container_surface_not_running(
        container_name.unwrap_or(policy.name.as_str()),
    ))
}

pub(in crate::runner) fn activate_container_runtime_for_task(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let prepared = effigy_containers::policy::render::prepare_resolved(policy)?;
    let policy = &prepared;
    let plan = runtime_activation_plan_from_request(repo_root, policy, request);
    activate_container_runtime_plan_for_task_using(
        &plan,
        policy,
        ensure_container_runtime_prepared,
        crate::runner::workspace_command::ensure_workspace_permissions_ready,
        ensure_runtime_gateway_readiness_stage,
    )
}

/// Activate the named container of the manifest at `repo_root` for a routed
/// task or suite execution.
///
/// This is the one activation path shared by the standard task pipeline and
/// container-routed builtin test suites: the policy loads from the manifest
/// that owns the declared runtime target.
pub(in crate::runner) fn activate_routed_container_runtime(
    repo_root: &Path,
    container_name: &str,
) -> Result<ContainerTaskActivation, RunnerError> {
    activate_routed_container_runtime_with(
        repo_root,
        container_name,
        |repo_root, container_name| {
            effigy_containers::load_container_policy(repo_root, Some(container_name))
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        },
        |repo_root, policy, request, _plan| {
            activate_container_runtime_for_task(repo_root, policy, request)
        },
    )
}

/// Injectable form of [`activate_routed_container_runtime`] for tests.
pub(in crate::runner) fn activate_routed_container_runtime_with(
    repo_root: &Path,
    container_name: &str,
    load_policy: impl FnOnce(&Path, &str) -> Result<EffectiveContainerPolicy, RunnerError>,
    activate: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        ActivationRequest<'_>,
        &RuntimeActivationPlan,
    ) -> Result<ContainerTaskActivation, RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let policy = load_policy(repo_root, container_name)?;
    let session_context = crate::runner::runtime_session_context::current_runtime_session_context();
    let plan = build_runtime_activation_plan(
        repo_root,
        policy.name.as_str(),
        Some(container_name),
        Some(repo_root.to_path_buf()),
        RuntimeActivationRoute::Task,
        session_context,
    );
    activate(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: plan.request.container_name.as_deref(),
            repo_override: plan.request.repo_override.clone(),
            route: plan.route,
            session_context,
        },
        &plan,
    )
}

pub(in crate::runner) fn build_runtime_activation_plan(
    repo_root: &Path,
    policy_name: &str,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    route: RuntimeActivationRoute,
    session_context: RuntimeSessionContext,
) -> RuntimeActivationPlan {
    let mut plan_request =
        RuntimeActivationRequest::new(repo_root.to_path_buf(), policy_name.to_owned())
            .repo_override(repo_override.unwrap_or_else(|| repo_root.to_path_buf()))
            .route(route)
            .lease_policy(runtime_lease_policy(session_context));
    if let Some(container_name) = container_name {
        plan_request = plan_request.container_name(container_name.to_owned());
    }
    plan_request.plan()
}

fn runtime_activation_plan_from_request(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
) -> RuntimeActivationPlan {
    build_runtime_activation_plan(
        repo_root,
        &policy.name,
        request.container_name,
        request.repo_override,
        request.route,
        request.session_context,
    )
}

fn runtime_lease_policy(session_context: RuntimeSessionContext) -> RuntimeLeasePolicy {
    match session_context.lease_refresh_policy {
        LeaseRefreshPolicy::RefreshOnActivation => RuntimeLeasePolicy::RefreshOnActivation,
        LeaseRefreshPolicy::SkipRefresh => RuntimeLeasePolicy::Skip,
    }
}

fn activate_container_runtime_plan_for_task_using(
    plan: &RuntimeActivationPlan,
    policy: &EffectiveContainerPolicy,
    ensure_runtime_prepared: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        RuntimeActivationRoute,
        Option<&str>,
        Option<PathBuf>,
        StartPolicy<'_>,
    ) -> Result<bool, RunnerError>,
    ensure_workspace_permissions: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        Option<&str>,
        Option<PathBuf>,
    ) -> Result<(), RunnerError>,
    ensure_gateway_ready: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let repo_root = plan.request.repo_root.as_path();
    // The command's session exists before any bring-up, so the bring-up's
    // activation attaches it and no idle stop can intervene (§5.9).
    let mut hold = Held::begin(repo_root, policy, activation_label(plan.route))?;
    let holders = hold
        .as_ref()
        .and_then(Held::id)
        .into_iter()
        .collect::<Vec<_>>();
    // An activation inside a managed workspace keeps the stack it starts.
    let retention = if plan.lease.policy == RuntimeLeasePolicy::RefreshOnActivation {
        crate::runner::environment::task_retention()
    } else {
        Retention::Persistent
    };
    let system_was_running = ensure_runtime_prepared(
        repo_root,
        policy,
        plan.route,
        plan.request.container_name.as_deref(),
        plan.request.repo_override.clone(),
        StartPolicy {
            retention,
            holders: &holders,
        },
    )?;
    ensure_workspace_permissions(
        repo_root,
        policy,
        plan.request.container_name.as_deref(),
        plan.request.repo_override.clone(),
    )?;
    if plan.aliases.register_gateway_routes {
        ensure_gateway_ready(repo_root, policy)?;
    }
    // A stack that was already running needed no bring-up: attach to it. A
    // stack the reconciler never recorded (a plain exec target started
    // elsewhere) runs unheld until a bring-up records it.
    if let Some(held) = hold.as_mut() {
        if !held.holds() {
            held.attach_active()?;
        }
    }
    Ok(ContainerTaskActivation {
        policy: policy.clone(),
        system_was_running,
        started_held: !system_was_running
            && retention == Retention::Held
            && plan.route != RuntimeActivationRoute::Exec,
        hold,
    })
}

fn activation_label(route: RuntimeActivationRoute) -> &'static str {
    match route {
        RuntimeActivationRoute::Exec => "exec",
        _ => "task",
    }
}

#[cfg(test)]
fn runtime_activation_report_for_result(
    plan: RuntimeActivationPlan,
    activation: ContainerTaskActivation,
) -> RuntimeActivationReport {
    plan.report(
        activation.system_was_running,
        RuntimeCleanupResult::NotRequired,
    )
}

pub(in crate::runner) fn prepare_container_exec_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    start: StartPolicy<'_>,
) -> Result<(), RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    let _colima_started = ensure_colima_running(policy, repo_root)?;
    let working_dir = load_container_exec_working_dir(repo_root, container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut bring_up = None;
    run_runtime_prep_steps(
        || prepare_runtime_mounts_stage(repo_root, policy),
        || {
            // Task preparation applies through the same one apply path as
            // `container up`: Compose recreates only changed services.
            bring_up = Some(crate::runner::environment::reconcile(
                repo_root,
                policy,
                start.retention,
                || {
                    run_runtime_compose_up_stage(
                        repo_root,
                        policy,
                        |repo_root, policy, args, label| {
                            Ok(run_compose_capture(repo_root, policy, args, label).map(|_| ())?)
                        },
                    )
                    .map_err(|error| {
                        effigy_containers::environment::BackendError::Uncertain(error.to_string())
                    })?;
                    recover_exited_owned_compose_services(repo_root, policy)
                        .map(|_| ())
                        .map_err(|error| {
                            effigy_containers::environment::BackendError::Uncertain(
                                error.to_string(),
                            )
                        })
                },
            )?);
            Ok(())
        },
        || {
            let binding = ensure_runtime_exec_readiness_stage(repo_root, policy, &working_dir)?;
            if effigy_core::worktree_scope::load_or_create(repo_root)
                .map_err(|error| {
                    RunnerError::task_invocation(format!("cannot identify checkout scope: {error}"))
                })?
                .is_some()
            {
                verify_live_checkout_mount(repo_root, policy, &working_dir, &binding)?;
                refresh_changed_cargo_inputs(repo_root, policy, &working_dir, &binding)?;
            }
            Ok(binding)
        },
        |binding| reconcile_runtime_aliases_stage(repo_root, policy, &binding),
    )?;
    match bring_up {
        Some(bring_up) => crate::runner::environment::activate(&bring_up, start.holders),
        None => Ok(()),
    }
}

fn run_verified_prep_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    args: &[std::ffi::OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args.to_vec(),
        effigy_containers::ContainerAction::Exec,
        label,
    )
    .map_err(RunnerError::from)?;
    crate::runner::exec_command::run_verified_compose_exec_plan(
        policy, &plan, binding, true, None, None,
    )
}

fn refresh_changed_cargo_inputs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    refresh_changed_cargo_inputs_using(repo_root, |changed| {
        for batch in changed.chunks(128) {
            let workdir = working_dir.to_string_lossy();
            let mut args = vec![
                "exec".to_owned(),
                "-T".to_owned(),
                "-w".to_owned(),
                workdir.into_owned(),
                policy.primary_service.clone(),
                "touch".to_owned(),
                "-c".to_owned(),
                "--".to_owned(),
            ];
            args.extend(batch.iter().map(|path| path.to_string_lossy().into_owned()));
            run_verified_prep_capture(
                repo_root,
                policy,
                binding,
                &compose_args(policy, args.iter().map(String::as_str)),
                "refresh changed Cargo inputs",
            )
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot refresh changed Cargo inputs in selected checkout: {error}"
                ))
            })?;
        }
        Ok(())
    })
}

fn refresh_changed_cargo_inputs_using(
    repo_root: &Path,
    touch_in_container: impl FnOnce(&[PathBuf]) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "-z", "HEAD", "--"])
        .current_dir(repo_root)
        .output()
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot list changed Cargo inputs: {error}"))
        })?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "cannot list changed Cargo inputs in {}: {}",
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let changed = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| std::path::PathBuf::from(String::from_utf8_lossy(bytes).as_ref()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "rs")
                || matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("Cargo.toml" | "Cargo.lock")
                )
        })
        .filter(|path| repo_root.join(path).is_file())
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(());
    }
    touch_in_container(&changed)
}

/// A running Compose service can be healthy while its bind mount still points
/// at an earlier checkout. Prove the selected checkout is visible before a
/// task lease is refreshed or any user command runs.
fn verify_live_checkout_mount(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    verify_live_checkout_mount_using(
        repo_root,
        working_dir,
        |probe_path| {
            let workdir = working_dir.to_string_lossy();
            let args = compose_args(
                policy,
                [
                    "exec",
                    "-T",
                    "-w",
                    workdir.as_ref(),
                    policy.primary_service.as_str(),
                    "cat",
                    probe_path,
                ],
            );
            let output = run_verified_prep_capture(
                repo_root,
                policy,
                binding,
                &args,
                "checkout mount read probe",
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            Ok(output.stdout)
        },
        |probe_path| {
            let workdir = working_dir.to_string_lossy();
            let args = compose_args(
                policy,
                [
                    "exec",
                    "-T",
                    "-w",
                    workdir.as_ref(),
                    policy.primary_service.as_str(),
                    "touch",
                    "--",
                    probe_path,
                ],
            );
            run_verified_prep_capture(
                repo_root,
                policy,
                binding,
                &args,
                "checkout mount write probe",
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            Ok(())
        },
    )
}

fn verify_live_checkout_mount_using(
    repo_root: &Path,
    working_dir: &Path,
    read_in_container: impl FnOnce(&str) -> Result<Vec<u8>, RunnerError>,
    write_in_container: impl FnOnce(&str) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    use std::io::Write;
    // The probe must live in the checkout, at the same relative location as
    // the container working directory. A fresh name also defeats stale copies.
    let name = format!(
        ".effigy-checkout-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            .as_nanos()
    );
    let host_path = repo_root.join(&name);
    let write_name = format!("{name}-write");
    let host_write_path = repo_root.join(&write_name);
    let payload = format!("{}:{name}", repo_root.display());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&host_path)
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot create checkout mount probe: {error}"))
        })?;
    let result = (|| {
        file.write_all(payload.as_bytes()).map_err(|error| {
            RunnerError::task_invocation(format!("cannot write checkout mount probe: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            RunnerError::task_invocation(format!("cannot sync checkout mount probe: {error}"))
        })?;
        let container_path = working_dir.join(&name);
        let actual = read_in_container(&container_path.to_string_lossy())?;
        if actual != payload.as_bytes() {
            return Err(RunnerError::task_invocation(format!(
                "container checkout mount at {} does not match selected checkout {}; refusing task execution",
                working_dir.display(), repo_root.display()
            )));
        }
        write_in_container(&working_dir.join(&write_name).to_string_lossy())?;
        if !host_write_path.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "container checkout mount at {} did not write to selected checkout {}; refusing task execution",
                working_dir.display(), repo_root.display()
            )));
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&host_path);
    let _ = std::fs::remove_file(&host_write_path);
    result.map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot verify container checkout mount for {}: {error}",
            repo_root.display()
        ))
    })
}

pub(in crate::runner) fn ensure_primary_service_exec_ready_for_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    let binding = verified_service_binding(
        repo_root,
        policy,
        &policy.primary_service,
        Instant::now() + OWNED_EXEC_VERIFICATION_TIMEOUT,
    )?;
    ensure_primary_service_exec_ready_with_recovery_and_load(
        policy,
        working_dir,
        |window| {
            probe_verified_service_exec_ready(
                repo_root,
                policy,
                working_dir,
                &binding,
                Instant::now() + window,
                None,
            )
        },
        // A restart is the environment's own repair transition: it refuses
        // while any run holds the environment (R4).
        || {
            crate::runner::environment::repair(repo_root, policy, || {
                restart_primary_service(repo_root, policy, &binding)
            })
        },
        None,
        HostLoad::observe(),
    )
}

/// Partial detached activation cannot grant restart authority to an unavailable
/// exec probe: existing services and exec sessions must remain in place.
pub(in crate::runner) fn verify_primary_service_exec_ready_for_reconciliation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    verify_service_exec_ready(
        repo_root,
        policy,
        &policy.primary_service,
        working_dir,
        None,
    )
}

/// Load-free form of
/// [`ensure_primary_service_exec_ready_with_recovery_and_load`], kept for the
/// deterministic proofs that assert the exact probe windows.
#[cfg(test)]
fn ensure_primary_service_exec_ready_with_recovery_using<T>(
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    probe: impl FnMut(Duration) -> Result<Option<T>, RunnerError>,
    restart: impl FnMut() -> Result<(), RunnerError>,
    live_exec_note: Option<String>,
) -> Result<T, RunnerError> {
    ensure_primary_service_exec_ready_with_recovery_and_load(
        policy,
        working_dir,
        probe,
        restart,
        live_exec_note,
        None,
    )
}

/// [`ensure_primary_service_exec_ready_with_recovery_using`] with the host
/// load observed before the first readiness probe. A saturated host opens the
/// first window to [`EXEC_READY_SATURATED_RETRY_WINDOW`] once, so the service
/// gets a bounded second attempt instead of a restart; the post-restart
/// windows, the live-run refusal and every failure path are unchanged.
#[allow(clippy::too_many_arguments)]
fn ensure_primary_service_exec_ready_with_recovery_and_load<T>(
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    mut probe: impl FnMut(Duration) -> Result<Option<T>, RunnerError>,
    mut restart: impl FnMut() -> Result<(), RunnerError>,
    live_exec_note: Option<String>,
    load: Option<HostLoad>,
) -> Result<T, RunnerError> {
    if let Some(binding) = probe(readiness_retry_window(Duration::from_secs(2), load))? {
        return Ok(binding);
    }
    if let Some(note) = live_exec_note {
        // A live managed run is exec'ing into this environment; restarting
        // the primary service would SIGKILL its remote processes. Keep
        // probing with the post-restart window instead of ever mutating a
        // running service under the run.
        if let Some(binding) = probe(Duration::from_secs(15))? {
            return Ok(binding);
        }
        return Err(RunnerError::task_invocation(format!(
            "container `{}` is not exec-ready and its recovery restart was refused: {note}. The running service was not restarted; stop the managed run first (`effigy <task> stop`) or wait for the service to become exec-ready",
            policy.name
        )));
    }
    if restart().is_ok() {
        if let Some(binding) = probe(Duration::from_secs(15))? {
            return Ok(binding);
        }
    }
    Err(RunnerError::container_runtime_exec_not_ready_with_load(
        policy,
        working_dir,
        load.and_then(|load| load.overload_phrase()),
    ))
}

/// The window the first exec-readiness probe gets. A saturated host may not
/// answer inside the short window at all, so it keeps the bounded
/// [`EXEC_READY_SATURATED_RETRY_WINDOW`] instead; a host below one runnable
/// task per CPU keeps `base`. Deterministic proofs pass their own observation.
fn readiness_retry_window(base: Duration, load: Option<HostLoad>) -> Duration {
    match load {
        Some(load) if load.saturated() => base.max(EXEC_READY_SATURATED_RETRY_WINDOW),
        _ => base,
    }
}

fn run_runtime_prep_steps<T>(
    prepare_mounts: impl FnOnce() -> Result<(), RunnerError>,
    compose_up: impl FnOnce() -> Result<(), RunnerError>,
    ensure_exec_ready: impl FnOnce() -> Result<T, RunnerError>,
    reconcile_aliases: impl FnOnce(T) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    // The routing decision only checks whether the primary service is running.
    // Sibling services may still be missing after a failed compose bring-up.
    prepare_mounts()?;
    compose_up()?;
    let binding = ensure_exec_ready()?;
    reconcile_aliases(binding)?;
    Ok(())
}

fn verified_service_binding(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    deadline: Instant,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    let rows = effigy_containers::exec::list_running_compose_containers_for_policy_with_deadline(
        repo_root,
        policy,
        Some(deadline),
    )?;
    // A selected-service operation must not probe or mutate its siblings.
    let selected = rows
        .into_iter()
        .filter(|row| row.service.as_deref() == Some(service))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "container service `{service}` is not running; start it with `effigy container {} up`",
            policy.name
        )));
    }
    let mut bindings = effigy_containers::exec::verify_owned_running_compose_identities(
        repo_root, policy, &selected, deadline,
    )?;
    if bindings.len() != 1 {
        return Err(RunnerError::task_invocation(
            "binding unavailable: expected one owned running service",
        ));
    }
    Ok(bindings.remove(0))
}

/// Verify the owned binding for `service` and wait until it accepts exec.
///
/// A caller `deadline` is absolute and bounds every step. Without one, binding
/// verification and each probe attempt get their own bounds and readiness
/// retries for [`EXEC_READY_RETRY_WINDOW`].
pub(in crate::runner) fn verify_service_exec_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    working_dir: &Path,
    deadline: Option<Instant>,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    let binding = verified_service_binding(
        repo_root,
        policy,
        service,
        deadline.unwrap_or_else(|| Instant::now() + OWNED_EXEC_VERIFICATION_TIMEOUT),
    )?;
    let retry_until = deadline.unwrap_or_else(|| {
        Instant::now() + readiness_retry_window(EXEC_READY_RETRY_WINDOW, HostLoad::observe())
    });
    probe_verified_service_exec_ready(
        repo_root,
        policy,
        working_dir,
        &binding,
        retry_until,
        deadline,
    )?
    .ok_or_else(|| RunnerError::container_runtime_exec_not_ready(policy, working_dir))
}

/// Whether a new readiness attempt may start at `now`. The retry window, and
/// the caller's absolute deadline when it owns one, gate every attempt; an
/// attempt already running keeps its own [`EXEC_READY_ATTEMPT_TIMEOUT`].
fn readiness_attempt_open(
    now: Instant,
    retry_until: Instant,
    hard_deadline: Option<Instant>,
) -> bool {
    now < retry_until && hard_deadline.is_none_or(|deadline| now < deadline)
}

/// Probe exec readiness until `retry_until`. An attempt already running is
/// allowed to finish within [`EXEC_READY_ATTEMPT_TIMEOUT`]; only
/// `hard_deadline`, when the caller owns one, cuts an attempt short.
fn probe_verified_service_exec_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    retry_until: Instant,
    hard_deadline: Option<Instant>,
) -> Result<Option<effigy_containers::exec::VerifiedComposeContainer>, RunnerError> {
    let args = compose_args(
        policy,
        [
            "exec",
            "-T",
            "-w",
            &working_dir.to_string_lossy(),
            binding
                .service()
                .ok_or_else(|| RunnerError::task_invocation("verified service missing"))?,
            "true",
        ],
    );
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args,
        effigy_containers::ContainerAction::Exec,
        "container exec readiness probe",
    )
    .map_err(RunnerError::from)?;
    loop {
        let now = Instant::now();
        // The retry window gates starting a new attempt: a short attempt that
        // finishes just before `retry_until` may not sleep past it and spawn
        // again. An attempt already running is still bounded by its own
        // attempt deadline and, when the caller owns one, the absolute
        // deadline.
        if !readiness_attempt_open(now, retry_until, hard_deadline) {
            return Ok(None);
        }
        let attempt_deadline = hard_deadline.map_or(now + EXEC_READY_ATTEMPT_TIMEOUT, |deadline| {
            deadline.min(now + EXEC_READY_ATTEMPT_TIMEOUT)
        });
        // Ownership failure is terminal: never restart or adopt a successor.
        let output = match crate::runner::exec_command::run_verified_compose_exec_plan(
            policy,
            &plan,
            binding,
            true,
            None,
            Some(attempt_deadline),
        ) {
            Ok(output) => output,
            Err(_) if Instant::now() >= attempt_deadline.min(retry_until) => return Ok(None),
            Err(error) => return Err(error),
        };
        if output.status.success() {
            return Ok(Some(binding.clone()));
        }
        if Instant::now() >= retry_until {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn restart_primary_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    binding.reverify(repo_root, policy, Instant::now() + Duration::from_secs(15))?;
    restart_primary_service_using(repo_root, policy, |repo_root, policy, args, label| {
        binding.reverify(repo_root, policy, Instant::now() + Duration::from_secs(15))?;
        let service = args
            .last()
            .and_then(|arg| arg.to_str())
            .ok_or_else(|| RunnerError::task_invocation("restart service missing"))?;
        let selected = if service == policy.primary_service {
            binding.clone()
        } else {
            verified_service_binding(
                repo_root,
                policy,
                service,
                Instant::now() + Duration::from_secs(15),
            )?
        };
        let native = [
            std::ffi::OsString::from("restart"),
            std::ffi::OsString::from(selected.id()),
        ];
        let plan = effigy_runtime::container_manager::runtime_invocation_plan(
            repo_root,
            policy,
            "docker",
            &native,
            effigy_containers::ContainerAction::Exec,
            label,
        )
        .map_err(RunnerError::from)?;
        selected.reverify(repo_root, policy, Instant::now() + Duration::from_secs(15))?;
        let output = crate::runner::exec_command::run_command_capture_until(
            repo_root,
            &plan.program,
            &plan.args,
            None,
            Some(Instant::now() + Duration::from_secs(15)),
        )?;
        if !output.status.success() {
            return Err(RunnerError::task_invocation("owned service restart failed"));
        }
        Ok(output)
    })
}

fn restart_primary_service_using(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    run_compose: impl Fn(
        &Path,
        &EffectiveContainerPolicy,
        &[std::ffi::OsString],
        &str,
    ) -> Result<std::process::Output, RunnerError>,
) -> Result<(), RunnerError> {
    let restart_args = compose_args(policy, ["restart", policy.primary_service.as_str()]);
    run_compose(
        repo_root,
        policy,
        &restart_args,
        "docker compose restart primary service",
    )?;

    let dependent_services = load_services_depending_on_primary(repo_root, policy)?;
    if dependent_services.is_empty() {
        return Ok(());
    }

    for service in dependent_services {
        let restart_args = compose_args(policy, ["restart", service.as_str()]);
        run_compose(
            repo_root,
            policy,
            &restart_args,
            "docker compose restart dependent service",
        )?;
    }
    Ok(())
}

fn load_services_depending_on_primary(
    _repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let mut services = Vec::new();
    for compose_file in &policy.compose_files {
        let raw = match policy.read_compose(compose_file) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let yaml: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(service_map) = yaml.get("services").and_then(|value| value.as_mapping()) else {
            continue;
        };
        for (service_name, service_value) in service_map {
            let Some(service_name) = service_name.as_str() else {
                continue;
            };
            if service_name == policy.primary_service {
                continue;
            }
            let depends_on = service_value.get("depends_on");
            if service_depends_on_primary(depends_on, &policy.primary_service)
                && !services.iter().any(|existing| existing == service_name)
            {
                services.push(service_name.to_owned());
            }
        }
    }
    Ok(services)
}

fn service_depends_on_primary(
    depends_on: Option<&serde_yaml::Value>,
    primary_service: &str,
) -> bool {
    let Some(depends_on) = depends_on else {
        return false;
    };
    if let Some(sequence) = depends_on.as_sequence() {
        return sequence
            .iter()
            .filter_map(|value| value.as_str())
            .any(|value| value == primary_service);
    }
    if let Some(mapping) = depends_on.as_mapping() {
        return mapping
            .keys()
            .filter_map(|value| value.as_str())
            .any(|value| value == primary_service);
    }
    false
}

fn prepare_host_bind_mount_dirs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let runtime_data_root = repo_root.join(".effigy/runtime/data");
    for compose_file in &policy.compose_files {
        let raw = match policy.read_compose(compose_file) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let yaml: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(services) = yaml.get("services").and_then(|s| s.as_mapping()) else {
            continue;
        };
        for (_service_name, service_value) in services {
            let Some(volumes) = service_value.get("volumes").and_then(|v| v.as_sequence()) else {
                continue;
            };
            let bind_roots = volumes
                .iter()
                .filter_map(|volume| volume.as_str())
                .filter_map(|spec| bind_mount_root(repo_root, spec))
                .collect::<Vec<_>>();
            for volume in volumes {
                let Some(spec) = volume.as_str() else {
                    continue;
                };
                for host_path in candidate_host_mount_paths(repo_root, spec, &bind_roots) {
                    if let Some(extension) = host_path.extension() {
                        let ext_str = extension.to_string_lossy();
                        if matches!(
                            ext_str.as_ref(),
                            "conf" | "yml" | "yaml" | "toml" | "json" | "sql" | "ini" | "env"
                        ) {
                            continue;
                        }
                    }
                    let _ = std::fs::create_dir_all(&host_path);
                    #[cfg(unix)]
                    if host_path.starts_with(&runtime_data_root) {
                        let _ = relax_runtime_data_permissions(&host_path);
                    }
                }
            }
        }
    }
    Ok(())
}

fn candidate_host_mount_paths(
    repo_root: &Path,
    spec: &str,
    bind_roots: &[(PathBuf, String)],
) -> Vec<PathBuf> {
    if let Some(host_path) = parse_bind_mount_host_path(spec) {
        let host_path = Path::new(host_path);
        if host_path.is_absolute() && host_path.starts_with(repo_root) {
            return vec![host_path.to_path_buf()];
        }
        return Vec::new();
    }

    let Some((_source, target, _options)) = parse_mount_parts(spec) else {
        return Vec::new();
    };
    let target_path = Path::new(target);
    let Some((bind_source, _bind_target, suffix)) = bind_roots
        .iter()
        .filter_map(|(source, mounted_target)| {
            target_path
                .strip_prefix(Path::new(mounted_target))
                .ok()
                .map(|suffix| (source, mounted_target, suffix))
        })
        .max_by_key(|(_source, mounted_target, _suffix)| mounted_target.len())
    else {
        return Vec::new();
    };

    if !bind_source.starts_with(repo_root) {
        return Vec::new();
    }
    vec![bind_source.join(suffix)]
}

fn bind_mount_root(repo_root: &Path, spec: &str) -> Option<(PathBuf, String)> {
    let host_path = parse_bind_mount_host_path(spec)?;
    let (source, target, _options) = parse_mount_parts(spec)?;
    let host_path = Path::new(host_path);
    if !host_path.is_absolute() || !host_path.starts_with(repo_root) {
        return None;
    }
    Some((PathBuf::from(source), target.to_owned()))
}

#[cfg(unix)]
fn relax_runtime_data_permissions(root: &Path) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    relax_runtime_data_permissions_entry(root)?;
    if root.is_dir() {
        for entry in walk_runtime_data(root)? {
            relax_runtime_data_permissions_entry(&entry)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn walk_runtime_data(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut discovered = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            discovered.push(path);
        }
    }
    Ok(discovered)
}

#[cfg(unix)]
fn relax_runtime_data_permissions_entry(path: &Path) -> std::io::Result<()> {
    let metadata = fs::metadata(path)?;
    let mode = if metadata.is_dir() { 0o777 } else { 0o666 };
    let permissions = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions)
}

fn parse_bind_mount_host_path(spec: &str) -> Option<&str> {
    let host = spec.split(':').next()?;
    if host.starts_with('/') || host.starts_with('.') || host.starts_with('~') {
        Some(host)
    } else {
        None
    }
}

fn parse_mount_parts(spec: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = spec.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

#[cfg(test)]
mod tests;
