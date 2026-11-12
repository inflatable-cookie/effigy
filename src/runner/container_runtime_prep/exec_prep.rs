//! Preparing a task's exec runtime: the one apply, then exec readiness, then
//! the alias reconciliation that follows it.

use std::ffi::OsString;
use std::path::Path;

use effigy_containers::exec::{ensure_colima_running, run_compose_capture_with_env};
use effigy_containers::{load_container_exec_working_dir, EffectiveContainerPolicy};

use crate::runner::error::RunnerError;

use super::activation::StartPolicy;
use super::cargo_inputs::refresh_changed_cargo_inputs;
use super::checkout_mount::verify_live_checkout_mount;
use super::prep::{
    ensure_runtime_exec_readiness_stage, prepare_runtime_mounts_stage,
    reconcile_runtime_aliases_stage, run_runtime_compose_up_stage, run_runtime_prep_steps,
};
use super::validation::validate_policy_runtime;

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
pub(super) fn run_task_apply(
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
pub(super) fn apply_shared_services(
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::tests::test_policy;

    /// A data command's apply passes `false` and must never start a shared
    /// service; the ordinary task apply still does (effigy#477 review).
    #[test]
    fn apply_shared_services_honours_the_reconciliation_decision() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let policy = test_policy(PathBuf::from("docker-compose.yml"));

        let started = AtomicBool::new(false);
        super::apply_shared_services(&policy, false, |_| {
            started.store(true, Ordering::SeqCst);
            Ok(vec!["unrelated shared mysql started".to_owned()])
        })
        .expect("apply without shared reconciliation");
        assert!(
            !started.load(Ordering::SeqCst),
            "a data command's activation must not ensure an unrelated shared service"
        );

        let started = AtomicBool::new(false);
        super::apply_shared_services(&policy, true, |_| {
            started.store(true, Ordering::SeqCst);
            Ok(Vec::new())
        })
        .expect("apply with shared reconciliation");
        assert!(
            started.load(Ordering::SeqCst),
            "the ordinary task apply still reconciles shared services"
        );
    }
}
