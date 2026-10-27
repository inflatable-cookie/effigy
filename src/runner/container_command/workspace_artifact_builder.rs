//! Execution-only recovery for Effigy's private, source-bound Linux builder.
//! A retained builder keeps its original admission. No stack is readmitted,
//! stopped or recreated; every build targets a freshly verified immutable ID.

use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_containers::exec::{
    list_compose_containers_for_policy_including_stopped_with_deadline,
    list_running_compose_containers_for_policy_with_deadline, verify_workspace_artifact_builder,
    verify_workspace_artifact_builder_declaration,
};
use effigy_containers::{load_container_policy, ContainerAction};

use crate::runner::environment;
use crate::runner::error::RunnerError;
use crate::runner::task_engine::TaskRun;

/// Whether the builder's environment is recorded as retained: its record
/// published `active`, so an existing container is the one it describes.
fn builder_retained(policy: &effigy_containers::EffectiveContainerPolicy) -> bool {
    environment::record(policy).is_some_and(|record| record.active())
}

pub(in crate::runner) fn build(repo_root: &Path, command: &[String]) -> Result<(), RunnerError> {
    // The maintained task already holds its ordinary task lock. This shared
    // operation lock also serializes callers using the helper from another
    // catalog/task declaration, without trying to reacquire that outer lock.
    let _section = TaskRun::exclusive(repo_root, "shared:linux-workspace-artifact-builder", 0)?;
    let stop = effigy_rhai::install_stop_requested_flag()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let check_interruption = || {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            Err(RunnerError::task_invocation(
                "workspace artifact build interrupted; original admission retained",
            ))
        } else {
            Ok(())
        }
    };
    check_interruption()?;
    let mut policy = load_container_policy(repo_root, Some("linux-release"))?;
    if command.is_empty()
        || policy.primary_service != "builder"
        || policy.profile != "effigy-release"
        || !policy.shared_services.is_empty()
        || !policy.declared_ports.is_empty()
        || !policy.dns_routes.is_empty()
        || policy.dns_domain.is_some()
        || !policy.host_processes.is_empty()
    {
        return Err(RunnerError::task_invocation(
            "workspace artifact build requires the private no-route Linux builder declaration",
        ));
    }
    verify_workspace_artifact_builder_declaration(&policy)?;
    effigy_containers::validate_container_policy(repo_root, &policy)?;
    effigy_containers::validate_compose_backend_runtime(repo_root, &policy)?;
    // Checkout identity and the external custody lease participate in render
    // labels. Establish them before taking the execution-only source snapshot;
    // this does not publish or replace the retained environment generation.
    policy = effigy_containers::policy::render::prepare_inputs(&policy)?;
    // The builder's profile is a managed Colima profile like any other
    // environment: create it when the user deleted it, start it when stopped
    // and restart it when its managed settings drifted. A runtime inventory
    // cannot even run against a missing profile, so this comes before the first
    // one (effigy#316).
    let runtime =
        effigy_containers::exec::ensure_runtime_backend_running_report(&policy, repo_root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    // Preserve the existing runtime drift guard; this operation grants no
    // authority to change backend, profile or the owned builder's generation.
    let mut deadline = Instant::now() + Duration::from_secs(15);
    let running = list_running_compose_containers_for_policy_with_deadline(
        repo_root,
        &policy,
        Some(deadline),
    )?;
    if let Some(message) =
        super::support::running_container_runtime_mismatch(repo_root, &policy, &running)
    {
        return Err(RunnerError::task_invocation(message));
    }
    // The build's inventory gets a fresh budget after the mismatch check.
    deadline = Instant::now() + Duration::from_secs(15);
    let mut retained = builder_retained(&policy).then_some(());
    // A profile Effigy just created or restarted discards the container the
    // record described. Recreating the builder is the repair that transition
    // owns.
    if runtime.profile_created_or_restarted {
        retained = None;
    }
    let mut rows = list_compose_containers_for_policy_including_stopped_with_deadline(
        repo_root,
        &policy,
        Some(deadline),
    )?;
    if rows.is_empty() {
        if retained.is_some() {
            return Err(RunnerError::task_invocation("workspace artifact builder record has no matching container; retained state was not removed or adopted"));
        }
        // No recovery is needed on first activation. Use the maintained
        // lifecycle path and its existing readiness/deadline/ownership checks.
        check_interruption()?;
        super::lifecycle::run_container_up(
            repo_root,
            Some("linux-release"),
            false,
            true,
            false,
            effigy_containers::environment::Retention::Persistent,
            &[],
        )?;
        policy = load_container_policy(repo_root, Some("linux-release"))?;
        retained = builder_retained(&policy).then_some(());
        deadline = Instant::now() + Duration::from_secs(15);
        rows = list_compose_containers_for_policy_including_stopped_with_deadline(
            repo_root,
            &policy,
            Some(deadline),
        )?;
    }
    let [row] = rows.as_slice() else {
        return Err(RunnerError::task_invocation("workspace artifact builder inventory must contain exactly one owned builder; no build attempted"));
    };
    if row.service.as_deref() != Some("builder") || row.oneoff {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder service identity is unverified",
        ));
    }
    let binding = verify_workspace_artifact_builder(repo_root, &policy, row, deadline)?;
    let mut args = ["exec", "-T", "-w", "/workspace", "builder"]
        .map(OsString::from)
        .to_vec();
    args.extend(command.iter().map(OsString::from));
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        &policy,
        args,
        ContainerAction::Exec,
        "workspace artifact build",
    )?;
    // The build holds its environment: no transition stops the builder
    // while it executes (R4).
    // A builder started outside the reconciler has no active record yet; the
    // build then runs unheld, exactly as before.
    let mut held = environment::Held::begin(repo_root, &policy, "workspace-artifact-build")?;
    if let Some(held) = held.as_mut() {
        held.attach_active()?;
    }
    if builder_retained(&policy).then_some(()) != retained {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder environment changed before build; no exec attempted",
        ));
    }
    check_interruption()?;
    let output = crate::runner::exec_command::run_verified_compose_exec_plan(
        &policy, &plan, &binding, false, None, None,
    )?;
    check_interruption()?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact build failed with status {}; builder and original admission retained for retry",
            output.status
        )));
    }
    Ok(())
}
