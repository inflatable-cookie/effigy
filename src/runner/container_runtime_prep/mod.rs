use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};

mod gateway;
mod prep;
mod running;
mod validation;

use effigy_containers::compose::compose_args;
use effigy_containers::exec::{ensure_colima_running, run_compose_capture_with_env};
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
use running::{check_runtime_running_state_stage, ensure_runtime_running_stage};
use validation::validate_policy_runtime;

use crate::runner::environment::Held;
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};
use effigy_containers::environment::Retention;
use effigy_kernel::id::RunId;

// Inventory and strict inspection require separate runtime transport hops. They
// must not consume the short workload probe's execution window. Explicit caller
// deadlines still bound both phases without being extended.
pub(in crate::runner) const OWNED_EXEC_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(15);

/// One `compose config --hash` reads a small map: service names and hashes.
const COMPOSE_HASH_STDOUT_BYTES: u64 = 64 * 1024;
const COMPOSE_HASH_STDERR_BYTES: u64 = 16 * 1024;

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
    /// Whether this activation may reconcile the environment's shared services.
    /// A data command that proves its own target sets this false, so its
    /// activation never publishes, starts or replaces a shared service it does
    /// not target.
    pub(in crate::runner) reconcile_shared_services: bool,
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
    if !system_was_running && !start.reconcile_shared_services {
        // Bringing the environment up runs the ordinary apply, which reconciles
        // every shared service. A command that must not touch them asks the
        // user for the environment command instead.
        return Err(RunnerError::task_invocation(format!(
            "container `{}` is not running, and this command must not start it because bringing it up would reconcile its shared services. Run `effigy container {} up` first, then re-run.",
            container_name.unwrap_or(policy.name.as_str()),
            container_name.unwrap_or(policy.name.as_str()),
        )));
    }
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
    /// Whether this apply reconciles the environment's shared services.
    pub(in crate::runner) reconcile_shared_services: bool,
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
    let reconcile_shared_services = request.reconcile_shared_services;
    let plan = runtime_activation_plan_from_request(repo_root, policy, request);
    activate_container_runtime_plan_for_task_using(
        &plan,
        policy,
        reconcile_shared_services,
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
/// Whether an activation of `policy` may reconcile that environment's shared
/// services.
///
/// A data seed's session leaves them alone, but the decision belongs to the
/// policy actually activated: a catalog-routed step into another catalog's
/// container is judged by that catalog's declaration, so a seed that started in
/// an environment without shared services still cannot start, publish or
/// replace an unrelated shared service elsewhere (effigy#477).
pub(in crate::runner) fn reconcile_shared_services_for(
    policy: &EffectiveContainerPolicy,
    session_context: RuntimeSessionContext,
) -> bool {
    !(session_context.data_seed_session && !policy.shared_services.is_empty())
}

pub(in crate::runner) fn activate_routed_container_runtime(
    repo_root: &Path,
    container_name: &str,
) -> Result<ContainerTaskActivation, RunnerError> {
    effigy_core::worktree_scope::prepare_checkout_token(repo_root).map_err(|error| {
        RunnerError::task_invocation(format!("cannot record the checkout identity: {error}"))
    })?;
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
            reconcile_shared_services: reconcile_shared_services_for(&policy, session_context),
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
    reconcile_shared_services: bool,
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
            reconcile_shared_services,
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
            // Task preparation runs the same apply as `container up`: the
            // shared services are started first, the same secrets reach the
            // same Compose args, and Compose recreates only changed services
            // (effigy#371).
            let apply = || {
                run_task_apply(
                    repo_root,
                    policy,
                    start.reconcile_shared_services,
                    crate::runner::container_command::support::ensure_shared_services_running,
                    crate::runner::container_command::secret_env::compose_secret_env_for_apply,
                    |repo_root, policy, args, label, env| {
                        run_compose_capture_with_env(repo_root, policy, args, label, env)
                            .map(|_| ())
                            .map_err(RunnerError::from)
                    },
                )
                .map_err(|error| {
                    effigy_containers::environment::BackendError::Uncertain(error.to_string())
                })
            };
            bring_up = Some(crate::runner::environment::reconcile(
                repo_root,
                policy,
                start.retention,
                apply,
            )?);
            Ok(())
        },
        || {
            let binding = ensure_runtime_exec_readiness_stage(repo_root, policy)?;
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
    if let Some(bring_up) = bring_up {
        crate::runner::environment::activate(&bring_up, policy, start.holders)?;
    } else {
        // Reusing an already-running stack still prepares through Gate(Env).
        crate::runner::workspace_command::publish_workspace_guest(repo_root, policy)?;
    }
    Ok(())
}

/// A task's environment apply, as one unit: start the shared services, resolve
/// the secrets the stack delivers, then run the one Compose apply with exactly
/// the arguments `container up` passes.
fn run_task_apply(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    reconcile_shared_services: bool,
    ensure_shared: impl FnOnce(&EffectiveContainerPolicy) -> Result<Vec<String>, RunnerError>,
    secret_env: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
    ) -> Result<Vec<(String, OsString)>, RunnerError>,
    run_compose: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        &[OsString],
        &str,
        &[(String, OsString)],
    ) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    apply_shared_services(policy, reconcile_shared_services, ensure_shared)?;
    let env = secret_env(repo_root, policy)?;
    run_runtime_compose_up_stage(repo_root, policy, &env, run_compose)
}

/// Start the environment's shared services, unless this apply must leave them
/// alone. A data command that proved its own target passes `false`, so its
/// activation can never publish, start or replace an unrelated shared service.
fn apply_shared_services(
    policy: &EffectiveContainerPolicy,
    reconcile_shared_services: bool,
    ensure_shared: impl FnOnce(&EffectiveContainerPolicy) -> Result<Vec<String>, RunnerError>,
) -> Result<(), RunnerError> {
    if !reconcile_shared_services {
        return Ok(());
    }
    for note in ensure_shared(policy)? {
        eprintln!("[info] shared service ready: {note}");
    }
    Ok(())
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

/// What the backend reports about the exact image one verified container runs.
///
/// These are the image's own facts, read from the image the container names:
/// the host never derives them from the guest's `uname`, from its own
/// architecture or from the artifact it wants to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ObservedImagePlatform {
    pub(in crate::runner) os: String,
    pub(in crate::runner) architecture: String,
    pub(in crate::runner) variant: String,
}

/// Observe the image platform behind one verified container id.
///
/// Two bounded runtime reads: the container's own inspect names its image, then
/// the image's inspect reports the OS, architecture and variant. A capture that
/// fails, a missing or non-string field, or an expired deadline is `Unknown`; no
/// default, host architecture or guest report stands in for it.
pub(in crate::runner) fn observed_image_platform(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_id: &str,
    deadline: Instant,
) -> effigy_kernel::observe::Observed<ObservedImagePlatform> {
    use effigy_kernel::observe::{Evidence, Observed, UnknownReason};

    let unknown = |reason: String| Observed::Unknown(UnknownReason::Inconsistent(reason));
    let container = match effigy_containers::exec::inspect_containers_for_policy(
        repo_root,
        policy,
        std::slice::from_ref(&container_id.to_owned()),
        Some(deadline),
    ) {
        Ok(output) => output.stdout,
        Err(error) => return unknown(format!("container inspect is unavailable: {error}")),
    };
    let image = match image_reference(&container) {
        Ok(image) => image,
        Err(reason) => return unknown(reason),
    };
    let inspected = match effigy_containers::exec::inspect_image_for_policy(
        repo_root,
        policy,
        &image,
        Some(deadline),
    ) {
        Ok(output) => output.stdout,
        Err(error) => return unknown(format!("image inspect is unavailable: {error}")),
    };
    let platform = match parse_image_platform(&inspected) {
        Ok(platform) => platform,
        Err(reason) => return unknown(reason),
    };
    Observed::Verified(
        platform,
        Evidence::new("backend image inspect".into(), None).expect("fixed evidence"),
    )
}

/// The image one container inspect record names.
fn image_reference(container_inspect: &[u8]) -> Result<String, String> {
    serde_json::from_slice::<serde_json::Value>(container_inspect)
        .ok()
        .and_then(|records| {
            records
                .as_array()?
                .first()?
                .get("Image")?
                .as_str()
                .map(str::to_owned)
        })
        .map(|image| image.trim().to_owned())
        .filter(|image| !image.is_empty())
        .ok_or_else(|| "the container inspect names no image".to_owned())
}

/// The platform one image inspect record reports. A missing or non-string OS or
/// architecture is unknown; the variant may legitimately be absent.
fn parse_image_platform(image_inspect: &[u8]) -> Result<ObservedImagePlatform, String> {
    let record = serde_json::from_slice::<serde_json::Value>(image_inspect)
        .ok()
        .and_then(|records| records.as_array()?.first().cloned())
        .ok_or_else(|| "the image inspect returned no record".to_owned())?;
    let field = |name: &str| {
        record
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let (Some(os), Some(architecture)) = (field("Os"), field("Architecture")) else {
        return Err("the image inspect names no OS or architecture".to_owned());
    };
    Ok(ObservedImagePlatform {
        os,
        architecture,
        variant: field("Variant").unwrap_or_default(),
    })
}

pub(in crate::runner) use crate::runner::workspace_command::provisioning::BoundedReadRefusal;

/// What proves that applying one rendered policy cannot recreate a live
/// predecessor of this environment.
///
/// The environment record's historical `desired` digest is *not* live evidence: it
/// says what Effigy last published, not what the running container was created
/// from. The one fact that does is Compose's own config hash: the container carries
/// the hash of the service config it was created from, and the render this
/// activation is about to apply can be asked for the hash it would produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum PredecessorCompatibility {
    /// This project has no running container, so nothing can be recreated.
    NoPredecessor,
    /// Every running container of this project was created from exactly the
    /// service config this render produces: `(full container id, config hash)`.
    Matches(Vec<(String, String)>),
}

impl PredecessorCompatibility {
    /// How many live containers were compared.
    pub(in crate::runner) fn compared(&self) -> usize {
        match self {
            Self::NoPredecessor => 0,
            Self::Matches(containers) => containers.len(),
        }
    }
}

/// Observe the live recreate risk of one rendered policy, within one deadline.
///
/// `policy` must be the one a prior [`seal_render_for_preflight`] produced: its
/// Compose inputs name the retained private seal, so this observation only reads
/// and never stages a second tree.
///
/// Unknown, a changed container id, a changed config hash and a backend that
/// cannot report Compose config hashes all refuse; only a live stack whose every
/// service config is byte-identical to what this render produces is compatible.
///
/// Interpolation is not replayed: the hash is computed from the frozen rendered
/// closure and the project, so a service config that depends on apply-time
/// environment values can differ and refuses, which is the safe direction.
/// The backend reads the live-predecessor guard performs.
///
/// Production reads the runtime; a test supplies a deterministic fixture, so the
/// adapter → Compose config hash → fenced reconcile path can be exercised without
/// a live container runtime. Every method is bounded by the caller's deadline.
pub(in crate::runner) trait PredecessorBackend {
    fn inventory(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<effigy_containers::exec::RunningComposeContainer>>;
    fn inspect(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        name: &str,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<u8>>;
    fn config_hashes(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> Result<std::collections::BTreeMap<String, String>, RunnerError>;
}

/// The production reads: the runtime inventory, its inspect and `compose
/// config --hash`.
#[derive(Debug, Default)]
pub(in crate::runner) struct RuntimePredecessorBackend;

impl PredecessorBackend for RuntimePredecessorBackend {
    fn inventory(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<effigy_containers::exec::RunningComposeContainer>>
    {
        match effigy_containers::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            repo_root,
            policy,
            Some(deadline),
        ) {
            Ok(rows) => effigy_kernel::observe::Observed::Verified(
                rows,
                effigy_kernel::observe::Evidence::new("project inventory".into(), None)
                    .expect("fixed evidence"),
            ),
            Err(error) => effigy_kernel::observe::Observed::Unknown(
                effigy_kernel::observe::UnknownReason::Inconsistent(format!(
                    "the project inventory is unavailable: {error}"
                )),
            ),
        }
    }

    fn inspect(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        name: &str,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<u8>> {
        use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
        match effigy_containers::exec::inspect_containers_for_policy(
            repo_root,
            policy,
            std::slice::from_ref(&name.to_owned()),
            Some(deadline),
        ) {
            Ok(output) => Observed::Verified(
                output.stdout,
                Evidence::new("container inspect".into(), None).expect("fixed evidence"),
            ),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(format!(
                "the predecessor `{name}` could not be inspected: {error}"
            ))),
        }
    }

    fn config_hashes(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
        compose_service_config_hashes(repo_root, policy, deadline)
    }
}

pub(in crate::runner) fn observed_predecessor_compatibility(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> effigy_kernel::observe::Observed<PredecessorCompatibility> {
    let mut backend = RuntimePredecessorBackend;
    observed_predecessor_compatibility_with(repo_root, policy, deadline, &mut backend)
}

/// The same observation over an injected backend seam. Production passes
/// [`RuntimePredecessorBackend`]; a test supplies a deterministic fixture.
pub(in crate::runner) fn observed_predecessor_compatibility_with(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
    backend: &mut dyn PredecessorBackend,
) -> effigy_kernel::observe::Observed<PredecessorCompatibility> {
    use effigy_kernel::observe::{Evidence, Observed, UnknownReason};

    let unknown = |reason: String| Observed::Unknown(UnknownReason::Inconsistent(reason));
    // Ownership resolution canonicalizes the checkout and reads its scope token:
    // both are filesystem work that can stall, so it runs on the publication's
    // bounded worker and an expired budget is unknown, never a held caller.
    let owned_root = repo_root.to_path_buf();
    let owned_policy = policy.clone();
    let ownership = match crate::runner::workspace_command::provisioning::bounded_publication_read(
        "workspace-predecessor-ownership",
        deadline,
        move || {
            effigy_containers::exec::OwnershipContext::resolve(&owned_root, &owned_policy)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        },
    ) {
        Ok(ownership) => ownership,
        Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::TimedOut) => {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "predecessor ownership".into(),
            })
        }
        Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::Unavailable(
            reason,
        )) => return unknown(format!("the checkout's ownership is unproven: {reason}")),
    };
    let rows = match backend.inventory(repo_root, policy, deadline) {
        effigy_kernel::observe::Observed::Verified(rows, _) => rows,
        effigy_kernel::observe::Observed::Unknown(reason) => return Observed::Unknown(reason),
        effigy_kernel::observe::Observed::Absent(_) => {
            return unknown("the project inventory is absent".into())
        }
    };
    let running = rows
        .into_iter()
        .filter(|row| {
            row.project_name.as_deref() == Some(policy.project_name.as_str())
                && !row.oneoff
                && effigy_containers::exec::compose_status_is_running(&row.status)
        })
        .collect::<Vec<_>>();
    if running.is_empty() {
        return Observed::Verified(
            PredecessorCompatibility::NoPredecessor,
            Evidence::new("project inventory".into(), None).expect("fixed evidence"),
        );
    }
    let intended = match backend.config_hashes(repo_root, policy, deadline) {
        Ok(hashes) => hashes,
        Err(error) => {
            return unknown(format!(
                "this backend cannot report the render's Compose config hashes: {error}"
            ));
        }
    };
    let mut compared = Vec::new();
    for row in running {
        let inspected = match backend.inspect(repo_root, policy, &row.container_name, deadline) {
            effigy_kernel::observe::Observed::Verified(bytes, _) => bytes,
            _ => {
                return unknown(format!(
                    "the predecessor `{}` inspect is unavailable",
                    row.container_name
                ))
            }
        };
        let verified = match ownership.verify_inspected(repo_root, policy, &row, &inspected) {
            Ok(verified) => verified,
            Err(error) => {
                return unknown(format!(
                    "the predecessor `{}` ownership or identity changed: {error}",
                    row.container_name
                ));
            }
        };
        let (service, live_hash) = match inspected_config_hash(&inspected) {
            Ok(facts) => facts,
            Err(reason) => {
                return unknown(format!(
                    "the predecessor `{}` reports no Compose config hash: {reason}",
                    row.container_name
                ));
            }
        };
        let Some(expected) = intended.get(&service) else {
            return unknown(format!(
                "the live predecessor runs service `{service}`, which this render does not declare"
            ));
        };
        if expected != &live_hash {
            return unknown(format!(
                "the live predecessor for service `{service}` was created from config `{live_hash}`, \
                 not the render's `{expected}`: applying would recreate it"
            ));
        }
        compared.push((verified.id().to_owned(), live_hash));
    }
    // Listing order is not evidence: two reads of the same stack must compare
    // equal, so the frozen facts are ordered by container id.
    compared.sort();
    Observed::Verified(
        PredecessorCompatibility::Matches(compared),
        Evidence::new("Compose config hash".into(), None).expect("fixed evidence"),
    )
}

/// The verified private seal of one frozen render, for a read-only preflight.
///
/// The kernel owner (`Objects::seal_for_preflight`) stages the closure in a
/// private tree, `O_EXCL`/0600, verifies it with the kernel's own closure
/// verifier inside the deadline, and returns a receipt. Nothing canonical is
/// written, no gate is taken and no desired state changes; only
/// `Objects::publish_sealed_desired` under `Gate(Env)` can make those bytes
/// canonical.
pub(in crate::runner) struct SealedRender {
    // Read by the handoff accessors below; the one apply consumes the receipt.
    #[allow(dead_code)]
    /// The kernel's verified receipt. Retaining it keeps the private tree leased
    /// for as long as this value lives, and it is the only thing the approved
    /// publisher (`Objects::publish_sealed_desired`) will promote.
    receipt: effigy_kernel::objects::SealedPreflight,
    /// A clone of the policy whose Compose inputs name the sealed tree, so the
    /// backend hashes exactly the bytes the apply will publish.
    policy: EffectiveContainerPolicy,
    /// The sealed tree's root, from the kernel's documented staging layout
    /// `state/staging/preflight-<token>/<digest>`.
    root: PathBuf,
}

#[allow(dead_code)] // The consuming caller (#452) reads these in its gated apply.
impl SealedRender {
    /// The kernel receipt this preflight verified. The caller publishes it with
    /// `Objects::publish_sealed_desired` under `Gate(Env)`, on the one apply.
    pub(in crate::runner) fn receipt(&self) -> &effigy_kernel::objects::SealedPreflight {
        &self.receipt
    }

    /// The frozen closure the receipt describes.
    pub(in crate::runner) fn closure(&self) -> Option<&effigy_kernel::objects::Closure> {
        self.policy.render.as_ref().map(|render| &render.closure)
    }

    /// The policy whose Compose inputs name the sealed tree. The compatibility
    /// observation reads this, so it never stages a second seal.
    pub(in crate::runner) fn policy(&self) -> &EffectiveContainerPolicy {
        &self.policy
    }

    /// The sealed tree the receipt names.
    pub(in crate::runner) fn root(&self) -> &Path {
        &self.root
    }
}

/// Seal this policy's frozen render for the preflight, inside the publication's
/// one deadline and without any authority a Prepare has.
pub(in crate::runner) fn seal_render_for_preflight(
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> Result<SealedRender, BoundedReadRefusal> {
    use effigy_kernel::ctx::{
        CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement,
    };

    let live = |phase: &str| {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                BoundedReadRefusal::Unavailable(format!(
                    "the render preflight deadline expired {phase}"
                ))
            })
    };
    live("before sealing the render")?;
    let Some(render) = policy.render.as_ref() else {
        return Err(BoundedReadRefusal::Unavailable(
            "the workspace render has no frozen closure to seal".into(),
        ));
    };
    let cx = OperationContext::new(
        Deadline::At(deadline),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Prepare,
        Placement::Host,
    )
    .effects()
    .map_err(|_| {
        BoundedReadRefusal::Unavailable(
            "the render preflight cannot take a Prepare capability".into(),
        )
    })?;
    // Opening the state root walks and opens directories, and the seal stages and
    // verifies files. Both are synchronous, so the pair runs on the owner's
    // bounded worker: the caller stops waiting at the deadline. An abandoned seal
    // only writes a private, leased staging tree that recovery reclaims; nothing
    // canonical can come of it, because only a returned receipt can be published.
    let closure = render.closure.clone();
    let token_deadline = deadline;
    let repo_root = policy.repo_root.clone();
    let (sealed, sealed_root) =
        match crate::runner::workspace_command::provisioning::bounded_publication_read(
            "workspace-render-seal",
            deadline,
            move || {
                let root = effigy_containers::environment::state_root(&cx, repo_root.as_path())
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                let sealed = effigy_kernel::objects::Objects::seal_for_preflight(
                    &cx,
                    &root,
                    &closure,
                    token_deadline,
                )
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                // The receipt names no canonical object; its private tree follows
                // the documented layout `staging/preflight-<token>/<digest>`. A
                // wrong guess is fail-closed: Compose cannot read it, so the guard
                // refuses.
                let sealed_root = root
                    .state_directory()
                    .path()
                    .join("staging")
                    .join(format!("preflight-{}", sealed.token()))
                    .join(sealed.digest());
                Ok((sealed, sealed_root))
            },
        ) {
            Ok(pair) => pair,
            Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::TimedOut) => {
                return Err(BoundedReadRefusal::TimedOut)
            }
            Err(
                crate::runner::workspace_command::provisioning::BoundedReadRefusal::Unavailable(
                    reason,
                ),
            ) => return Err(BoundedReadRefusal::Unavailable(reason)),
        };
    let mut policy = policy.clone();
    let frozen = render.closure.files().clone();
    let compose_files = policy.compose_files.clone();
    let render_path = render.path.clone();
    let read_root = sealed_root.clone();
    // The trusted private tree still lives on a filesystem that can stall. A
    // returned seal does not make subsequent synchronous reads deadline-safe.
    // An abandoned verifier only reads; it cannot promote or mutate the tree.
    policy.compose_files =
        crate::runner::workspace_command::provisioning::bounded_publication_read(
            "workspace-compose-inputs",
            deadline,
            move || {
                verified_compose_inputs(&frozen, &compose_files, &render_path, &read_root)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
            },
        )?;
    live("after sealing the render")?;
    Ok(SealedRender {
        receipt: sealed,
        policy,
        root: sealed_root,
    })
}

/// Verify that the frozen closure owns every Compose input the apply will pass,
/// then return those inputs pointed at the sealed private tree.
///
/// The 2026-11-01 Supervisor ruling freezes `checkout/.effigy/runtime/compose/
/// <name>.compose.yml` in the closure with the rest of the render, so a generated
/// input is not a special case: an input missing from the closure, an input
/// outside the rendered checkout, an absent sealed file and a sealed file whose
/// bytes differ all refuse by name. Nothing is hashed or applied from a path the
/// closure does not vouch for.
fn verified_compose_inputs(
    frozen: &std::collections::BTreeMap<String, Vec<u8>>,
    compose_files: &[std::path::PathBuf],
    render_path: &Path,
    sealed_root: &Path,
) -> Result<Vec<std::path::PathBuf>, BoundedReadRefusal> {
    let mut selected = Vec::with_capacity(compose_files.len());
    for file in compose_files {
        let Ok(relative) = file.strip_prefix(render_path) else {
            return Err(BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-outside-render-closure: `{}` is not under the rendered \
                 checkout, so the frozen closure cannot carry it",
                file.display()
            )));
        };
        // The admitted Compose inputs already carry the render's own `checkout/`
        // prefix, which is exactly how the render builder keys them in the
        // closure, so the render-relative path is the closure key and the sealed
        // path.
        let name = relative.to_string_lossy().replace('\\', "/");
        let Some(frozen_bytes) = frozen.get(name.as_str()) else {
            return Err(BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-outside-render-closure: `{name}` is not part of the \
                 frozen closure, so the preflight cannot hash verified bytes (owner: the render \
                 must freeze every Compose input the apply reads)"
            )));
        };
        // The render is published by the reconcile that follows this preflight, so
        // its own path is not materialised yet: the bytes Compose will read come
        // from the sealed tree, and they must be the frozen ones verbatim.
        let sealed_path = sealed_root.join(&name);
        let sealed_bytes = std::fs::read(sealed_path.as_path()).map_err(|error| {
            BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-unreadable: `{name}` is not in the sealed tree: {error}"
            ))
        })?;
        if sealed_bytes != *frozen_bytes {
            return Err(BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-closure-bytes-differ: `{name}` in the sealed tree is not \
                 the byte image the frozen closure holds"
            )));
        }
        selected.push(sealed_path);
    }
    Ok(selected)
}

/// The service -> config-hash map one rendered policy produces.
///
/// `compose config --hash` is the backend's own answer, computed from the same
/// frozen closure the apply uses. A backend without it (or an unparsable answer)
/// refuses: an unsupported comparison is never treated as compatible.
fn compose_service_config_hashes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
    let args = effigy_containers::compose::compose_args(policy, ["config", "--hash", "*"]);
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_args(
        repo_root,
        policy,
        args,
        effigy_containers::ContainerAction::Status,
        "workspace compose config hash",
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut budget = crate::runner::exec_command::BoundedCommandBudget::new(
        Instant::now(),
        deadline,
        COMPOSE_HASH_STDOUT_BYTES,
        COMPOSE_HASH_STDERR_BYTES,
    );
    let captured = budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            crate::runner::exec_command::BoundedCommandPhase::WorkspacePreparation,
        )
        .map_err(|failure| {
            RunnerError::task_invocation(format!(
                "compose config --hash did not complete in the {} phase; the backend cannot                  report this render's service hashes",
                failure.phase.label()
            ))
        })?;
    if !captured.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "compose config --hash failed: {}",
            String::from_utf8_lossy(&captured.stderr).trim()
        )));
    }
    parse_service_config_hashes(&captured.stdout)
}

/// Parse `compose config --hash` output: one `service hash` line per service.
fn parse_service_config_hashes(
    stdout: &[u8],
) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
    let text = String::from_utf8_lossy(stdout);
    let mut hashes = std::collections::BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((service, hash)) = line.split_once(char::is_whitespace) else {
            return Err(RunnerError::task_invocation(
                "compose config --hash printed no service hash pair",
            ));
        };
        let hash = hash.trim();
        if service.is_empty() || hash.is_empty() {
            return Err(RunnerError::task_invocation(
                "compose config --hash printed an empty service or hash",
            ));
        }
        hashes.insert(service.to_owned(), hash.to_owned());
    }
    if hashes.is_empty() {
        return Err(RunnerError::task_invocation(
            "compose config --hash reported no services",
        ));
    }
    Ok(hashes)
}

/// The Compose service and config hash one inspected container records.
fn inspected_config_hash(stdout: &[u8]) -> Result<(String, String), String> {
    let raw: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|error| error.to_string())?;
    let labels = raw
        .as_array()
        .and_then(|records| records.first())
        .and_then(|record| record.get("Config"))
        .and_then(|config| config.get("Labels"))
        .ok_or_else(|| "the inspect names no labels".to_owned())?;
    let label = |name: &str| {
        labels
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("the inspect names no `{name}` label"))
    };
    Ok((
        label("com.docker.compose.service")?,
        label("com.docker.compose.config-hash")?,
    ))
}

/// Verify the owned binding of `service`: the running container that this
/// checkout and scope own. Readiness itself is Running plus the service's own
/// health check, which the bring-up wait already decided (C1); an exec that
/// cannot run in the container reports its own error.
pub(in crate::runner) fn verify_service_binding(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    deadline: Option<Instant>,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    verified_service_binding(
        repo_root,
        policy,
        service,
        deadline.unwrap_or_else(|| Instant::now() + OWNED_EXEC_VERIFICATION_TIMEOUT),
    )
}

/// The owned binding of the environment's primary service, for the stages that
/// need an inspected full id (mount verification, alias reconciliation).
/// `deadline` is the bring-up's observation budget when the caller shares one;
/// the standard inventory budget applies when it does not.
pub(in crate::runner) fn ensure_primary_service_binding(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    verify_service_binding(repo_root, policy, &policy.primary_service, deadline)
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
