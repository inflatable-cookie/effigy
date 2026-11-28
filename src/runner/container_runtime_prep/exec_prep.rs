//! Preparing a task's exec runtime: the one apply, then exec readiness, then
//! the alias reconciliation that follows it.

use std::ffi::OsString;
use std::path::Path;

use effigy_containers::exec::{
    ensure_runtime_backend_running_for_bringup, run_compose_capture_with_env_owner,
    RetainedComposeOwner,
};
use effigy_containers::{load_container_exec_working_dir, EffectiveContainerPolicy};

use crate::runner::container_command::secret_env::ResolvedContainerSecretRuntime;
use crate::runner::error::RunnerError;

use super::activation::StartPolicy;
use super::cargo_inputs::refresh_changed_cargo_inputs;
use super::checkout_mount::verify_live_checkout_mount;
use super::prep::{
    ensure_runtime_exec_readiness_stage, prepare_runtime_mounts_stage,
    reconcile_runtime_aliases_stage, run_runtime_compose_up_stage, run_runtime_prep_steps,
};
use super::validation::validate_policy_runtime;

/// Prepare the stack a task runs in, on a fresh or a running environment
/// alike.
///
/// A running stack still takes the one apply: it is what reconciles the shared
/// services, delivers the secrets this session resolves, and lets Compose
/// recreate only the services whose declared inputs changed (effigy#371). A
/// skipped apply would be a stack whose delivery is whatever the bring-up that
/// started it had, which is exactly the borrow `effigy dev` cannot inherit
/// (effigy#1036). The prompt that used to hide under this apply is what
/// changed: an optional-only resolution no longer asks for a passphrase, and a
/// required one asks visibly.
pub(in crate::runner) fn prepare_container_exec_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    start: StartPolicy<'_>,
) -> Result<(), RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    let (_runtime_started, _colima_bringup_guard) =
        ensure_runtime_backend_running_for_bringup(policy, repo_root)?;
    let working_dir = load_container_exec_working_dir(repo_root, container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut bring_up = None;
    // The runtime this apply resolved, kept for the runtime-file delivery that
    // follows exec readiness.
    let applied_secrets = std::cell::RefCell::new(None::<ResolvedContainerSecretRuntime>);
    run_runtime_prep_steps(
        || Ok(()),
        || {
            // Task preparation runs the same apply as `container up`: the
            // shared services are started first, the same secrets reach the
            // same Compose args, and Compose recreates only changed services
            // (effigy#371).
            // The apply owns this reconcile run's custody, so a secret-bearing
            // VM Compose launch spawns under it instead of refusing (effigy#1003).
            let failure = std::cell::RefCell::new(None);
            let result = crate::runner::environment::reconcile_owned(
                repo_root,
                policy,
                start.retention,
                |run, effect, deadline| {
                    prepare_runtime_mounts_stage(repo_root, policy).and_then(|()| run_task_apply(
                        repo_root,
                        policy,
                        start.reconcile_shared_services,
                        crate::runner::container_command::support::ensure_shared_services_running,
                        |repo_root, policy| {
                            let secrets = crate::runner::container_command::secret_env::resolve_secret_runtime_for_apply(repo_root, policy)?;
                            *applied_secrets.borrow_mut() = Some(secrets.clone());
                            Ok(secrets)
                        },
                        |repo_root, policy, args, label, env| {
                            run_compose_capture_with_env_owner(
                                repo_root,
                                policy,
                                args,
                                label,
                                env,
                                deadline,
                                &mut RetainedComposeOwner::new(run, effect),
                            )
                            .map(|_| ())
                            .map_err(RunnerError::from)
                        },
                    ))
                    .and_then(|()| {
                        let has_named_volumes = policy.workspace_user.is_some()
                            && effigy_containers::load_workspace_ownership_plan(policy)
                                .map_err(|error| RunnerError::task_invocation(error.to_string()))?
                                .targets.iter().any(|target| {
                                    target.mount_kind == effigy_containers::WorkspaceMountKind::NamedVolume
                                });
                        if !has_named_volumes {
                            return Ok(());
                        }
                        crate::runner::container_command::verify_and_repair_detached_mounts(
                            || {
                                ensure_runtime_exec_readiness_stage(repo_root, policy)?;
                                crate::runner::workspace_command::permissions::verify_workspace_named_volumes(
                                    repo_root, policy, container_name,
                                )
                            },
                            |mounts| {
                                crate::runner::environment::ensure_mount_repair_unheld(policy)?;
                                eprintln!("[info] detached named-volume mount(s) {}; recreating service `{}` to reattach them while preserving named volumes", mounts.join(", "), policy.primary_service);
                                let args = crate::runner::container_command::detached_recreate_args(policy)?;
                                let env = applied_secrets.borrow().as_ref()
                                    .map(crate::runner::container_command::secret_env::compose_env)
                                    .unwrap_or_default();
                                run_compose_capture_with_env_owner(
                                    repo_root, policy, &args, "docker compose recreate detached volume",
                                    &env, deadline, &mut RetainedComposeOwner::new(run, effect),
                                ).map_err(RunnerError::from)?;
                                Ok(())
                            },
                        )
                    })
                    .map_err(|error| {
                        let message = error.to_string();
                        *failure.borrow_mut() = Some(error);
                        effigy_containers::environment::BackendError::Uncertain(message)
                    })
                },
            );
            let applied = result.map_err(|error| failure.into_inner().unwrap_or(error))?;
            if !applied.holders.is_empty()
                && policy.compose_source == effigy_containers::EffectiveComposeSource::Generated
                && policy.secret_delivery
                    == effigy_manifest::ManifestContainerSecretDelivery::RuntimeFiles
            {
                let secrets =
                    crate::runner::container_command::secret_env::resolve_secret_runtime_for_apply(
                        repo_root, policy,
                    )?;
                crate::runner::container_command::secret_env::stage_container_secret_runtime(
                    repo_root, policy, &secrets,
                )?;
                *applied_secrets.borrow_mut() = Some(secrets);
            }
            crate::runner::environment::refuse_held_detached_mounts(
                &applied,
                policy,
                container_name,
            )?;
            bring_up = Some(applied);
            Ok(())
        },
        || {
            let binding = ensure_runtime_exec_readiness_stage(repo_root, policy)?;
            deliver_applied_runtime_secret_files(
                repo_root,
                policy,
                applied_secrets.borrow().as_ref(),
                &binding,
            )?;
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
    ) -> Result<ResolvedContainerSecretRuntime, RunnerError>,
    run_compose: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        &[OsString],
        &str,
        &[(String, OsString)],
    ) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    apply_shared_services(policy, reconcile_shared_services, ensure_shared)?;
    let secrets = secret_env(repo_root, policy)?;
    crate::runner::container_command::secret_env::stage_container_secret_runtime(
        repo_root, policy, &secrets,
    )?;
    let env = crate::runner::container_command::secret_env::compose_env(&secrets);
    run_runtime_compose_up_stage(repo_root, policy, &env, run_compose)
}

/// Write the runtime files this apply resolved, now that the primary service
/// binding is verified, and report the keys this apply changed under it.
///
/// A stack this session started already received them from its `container up`.
/// A stack this session *reused* did not: a direct `container up` outside a
/// `dev` session cannot resolve the session's optional keys, so an `effigy dev`
/// that borrows that stack would otherwise run with the keys it just resolved
/// missing and nothing said about it (effigy#1036). Writing the files delivers
/// them to every process that starts next; a service already running keeps the
/// values it read, so the delivery names the keys it changed, once, and says
/// how a startup-only reader gets them. Nothing is carried between applies.
/// Restarting a live service from inside the apply would stop the very stack
/// this task is about to run in, so the report is the safe half of "rewrite and
/// refresh" here.
pub(super) fn deliver_applied_runtime_secret_files(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    secrets: Option<&ResolvedContainerSecretRuntime>,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    let Some(secrets) = secrets else {
        return Ok(());
    };
    let warnings = deliver_applied_runtime_secret_files_with(secrets, |secrets| {
        crate::runner::container_command::secret_env::materialize_container_secret_runtime(
            repo_root, policy, secrets, binding,
        )
    })?;
    for warning in warnings {
        eprintln!("[warn] {warning}");
    }
    Ok(())
}

/// The delivery decision, testable without a container: only a runtime-file
/// delivery that resolved values writes anything, and an empty delivery is
/// never written (the undelivered keys already warned).
pub(super) fn deliver_applied_runtime_secret_files_with(
    secrets: &ResolvedContainerSecretRuntime,
    write: impl FnOnce(&ResolvedContainerSecretRuntime) -> Result<Vec<String>, RunnerError>,
) -> Result<Vec<String>, RunnerError> {
    if !crate::runner::container_command::secret_env::writes_runtime_secret_files(secrets) {
        return Ok(Vec::new());
    }
    write(secrets)
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
