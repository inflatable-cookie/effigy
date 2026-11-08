//! Execution-only recovery for Effigy's private, source-bound Linux builder.
//! A retained builder keeps its original admission. No stack is readmitted,
//! stopped or recreated; every build targets a freshly verified immutable ID.
//!
//! The one thing the build does refresh is the source it compiles. A VM reaches
//! only the host paths its Colima profile shares, so a development checkout
//! outside every shared mount binds its own staged source closure under the
//! profile's writable mount (effigy#715). The closure is staged before anything
//! renders or activates, proven through the container's own bind before the
//! build, and only the verified artifact bytes are copied back to the caller.

use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_containers::exec::{
    list_compose_containers_for_policy_including_stopped_with_deadline,
    list_running_compose_containers_for_policy_with_deadline, observed_workspace_artifact_builder,
    verify_workspace_artifact_builder, verify_workspace_artifact_builder_declaration,
};
use effigy_containers::{load_container_policy, ContainerAction};

use crate::runner::environment;
use crate::runner::error::RunnerError;
use crate::runner::task_engine::TaskRun;

use super::workspace_artifact_source;

/// Whether the builder's environment is recorded as retained: its record
/// published `active`, so an existing container is the one it describes.
fn builder_retained(policy: &effigy_containers::EffectiveContainerPolicy) -> bool {
    environment::record(policy).is_some_and(|record| record.active())
}

/// Restriction-only switch set by the Recipe E entry. `1` makes this build reuse
/// the already running, already configured profile and refuse every start,
/// create, restart, settings rewrite or unit start. Absence keeps ordinary
/// acquisition. Any other value refuses, so the switch can never widen authority.
const NO_CHANGE_ENV: &str = "EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING";

fn no_change_requested(raw: Option<&str>) -> Result<bool, RunnerError> {
    match raw {
        None => Ok(false),
        Some("1") => Ok(true),
        Some(other) => Err(RunnerError::task_invocation(format!(
            "{NO_CHANGE_ENV} must be `1` when set, not `{other}`"
        ))),
    }
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
    // Stage this checkout's source closure before anything renders, activates
    // or inspects: the bind the render names must already carry the bytes the
    // build will compile. A checkout inside a mount the profile shares needs no
    // staging and binds its own path, exactly as before.
    let staged = match effigy_containers::artifact_source::policy_stage(&policy)? {
        Some(plan) => {
            check_interruption()?;
            let staged = workspace_artifact_source::stage(repo_root, &policy, &plan)?;
            // The staged source is this build's evidence: it must be a record
            // whose closure was written and verified, never a pending one.
            staged
                .manifest
                .clone()
                .require_complete()
                .map_err(RunnerError::task_invocation)?;
            if staged.changed {
                eprintln!(
                    "[info] staged this checkout's source for the Linux builder at `{}`",
                    plan.source.display()
                );
            }
            Some((plan, staged))
        }
        None => None,
    };
    // Checkout identity and the external custody lease participate in render
    // labels. Establish them before taking the execution-only source snapshot;
    // this does not publish or replace the retained environment generation.
    policy = effigy_containers::policy::render::prepare_inputs(&policy)?;
    // The Compose client runs inside the VM for this backend, so its inputs must
    // be inside the mount too: the published render closure is staged beside the
    // source and every Compose plan below names those files. The render digest is
    // unchanged, so this neither publishes nor supersedes the environment.
    effigy_containers::artifact_source::stage_compose_render(&mut policy)?;
    // The builder's profile is a managed Colima profile like any other
    // environment: create it when the user deleted it, start it when stopped
    // and restart it when its managed settings drifted. A runtime inventory
    // cannot even run against a missing profile, so this comes before the first
    // one (effigy#316).
    let no_change = no_change_requested(std::env::var(NO_CHANGE_ENV).ok().as_deref())?;
    let runtime = if no_change {
        effigy_containers::exec::verify_runtime_backend_no_change(&policy, repo_root)
    } else {
        effigy_containers::exec::ensure_runtime_backend_running_report(&policy, repo_root)
    }
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
        if no_change {
            return Err(RunnerError::task_invocation(
                "no-change workspace artifact build refuses to create the builder container; the profile must already hold it",
            ));
        }
        // No recovery is needed on first activation. Use the maintained
        // lifecycle path and its existing readiness/deadline/ownership checks.
        check_interruption()?;
        super::lifecycle::run_container_up(
            repo_root,
            Some("linux-release"),
            super::UpSelection::Direct,
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
    if rows.len() != 1 {
        return Err(RunnerError::task_invocation("workspace artifact builder inventory must contain exactly one owned builder; no build attempted"));
    }
    let row = rows.remove(0);
    if row.service.as_deref() != Some("builder") || row.oneoff {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder service identity is unverified",
        ));
    }
    let binding = match verify_workspace_artifact_builder(repo_root, &policy, &row, deadline)
        .map_err(RunnerError::from)
    {
        Ok(binding) => binding,
        // A builder retained from the pre-staging declaration binds this
        // checkout's own host path, which the profile cannot reach. The
        // maintained activation replaces exactly this checkout's container with
        // the staged bind; it is never adopted, exec'd or hand-removed.
        Err(error) if staged.is_some() => {
            // A never-started builder's refusal names its own cause; the strict
            // running refusal would only repeat the missing `State.Status`.
            let observed =
                match observed_workspace_artifact_builder(repo_root, &policy, &row, deadline) {
                    Ok(observed) => observed,
                    Err(refusal)
                        if effigy_containers::exec::compose_status_is_created(&row.status) =>
                    {
                        return Err(RunnerError::from(refusal));
                    }
                    Err(_) => return Err(error),
                };
            let checkout = effigy_core::worktree_scope::checkout_root(repo_root)
                .canonicalize()
                .unwrap_or_else(|_| repo_root.to_path_buf());
            if observed.source != checkout {
                return Err(error);
            }
            if no_change {
                return Err(RunnerError::task_invocation(
                    "no-change workspace artifact build refuses to replace the owned builder container for its staged bind",
                ));
            }
            // The transition replaces only the identity observed above. Listing
            // and inspection are repeated right before it, so a builder that
            // started, was replaced or changed identity during the wait is never
            // the one the lifecycle acts on.
            check_interruption()?;
            confirm_transition_target(repo_root, &policy, &observed)?;
            check_interruption()?;
            super::lifecycle::run_container_up(
                repo_root,
                Some("linux-release"),
                super::UpSelection::Direct,
                false,
                true,
                false,
                effigy_containers::environment::Retention::Persistent,
                &[],
            )?;
            policy = load_container_policy(repo_root, Some("linux-release"))?;
            retained = builder_retained(&policy).then_some(());
            deadline = Instant::now() + Duration::from_secs(15);
            let mut replanned = list_compose_containers_for_policy_including_stopped_with_deadline(
                repo_root,
                &policy,
                Some(deadline),
            )?;
            let [replanned] = replanned.as_mut_slice() else {
                return Err(RunnerError::task_invocation(
                    "workspace artifact builder inventory after source staging must contain exactly one owned builder; no build attempted",
                ));
            };
            verify_workspace_artifact_builder(repo_root, &policy, replanned, deadline)?
        }
        Err(error) => return Err(error),
    };
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
    // Prove the bind carries this build's source through the container's own
    // view before compiling: an empty VM directory auto-created for an
    // unreachable host path, or a bind of some other source, fails here by name
    // instead of as a build error.
    if let Some((_, staged)) = &staged {
        check_interruption()?;
        verify_staged_source_in_builder(repo_root, &policy, &binding, staged)?;
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
    // The build wrote its artifact into the staged source. Only bytes its own
    // producer receipt blesses for this target and version reach the caller, and
    // only while the source still matches what was compiled: a caller edit or a
    // changed staged tree during a long build makes the artifact describe source
    // that is no longer the recorded one.
    if let Some((plan, staged_source)) = &staged {
        check_interruption()?;
        workspace_artifact_source::verify_source_unchanged(plan, staged_source)?;
        check_interruption()?;
        let triple = std::env::var("EFFIGY_LINUX_RELEASE_TRIPLE")
            .ok()
            .map(|triple| triple.trim().to_owned())
            .filter(|triple| !triple.is_empty())
            .unwrap_or_else(|| "x86_64-unknown-linux-gnu".to_owned());
        workspace_artifact_source::restore_produced_artifact(
            plan,
            repo_root,
            &triple,
            &effigy_core::build_info::active_version(),
        )?;
    }
    Ok(())
}

/// Repeat the inventory and identity observation the transition relies on: one
/// owned builder, with the same full ID, source and never-started state.
fn confirm_transition_target(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    observed: &effigy_containers::exec::ObservedWorkspaceArtifactBuilder,
) -> Result<(), RunnerError> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut rows = list_compose_containers_for_policy_including_stopped_with_deadline(
        repo_root,
        policy,
        Some(deadline),
    )?;
    let [row] = rows.as_mut_slice() else {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder inventory changed before the transition; no transition attempted",
        ));
    };
    let current = observed_workspace_artifact_builder(repo_root, policy, row, deadline)
        .map_err(RunnerError::from)?;
    if &current != observed {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder identity changed before the transition; no transition attempted",
        ));
    }
    Ok(())
}

/// Ask the builder, through its verified immutable ID, whether its `/workspace`
/// carries exactly the staged closure this build recorded.
///
/// The container reads the inventory that lives inside the bound tree, checks
/// the inventory's own digest (known here), verifies every listed file's digest
/// and counts the source files it can see. An empty VM directory, a bind of some
/// other source, a changed or missing file and an unaccounted extra file all
/// fail by name before anything is compiled.
fn verify_staged_source_in_builder(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    staged: &workspace_artifact_source::StagedSource,
) -> Result<(), RunnerError> {
    let expected_files = staged.manifest.files.len();
    let script = format!(
        r#"set -eu
cd /workspace
inventory={inventory}
[ -f "$inventory" ] || {{ echo "missing-inventory"; exit 41; }}
[ "$(sha256sum "$inventory" | cut -d' ' -f1)" = {inventory_digest} ] || {{ echo "inventory-mismatch"; exit 42; }}
while IFS= read -r line; do
  [ -n "$line" ] || continue
  sum=$(printf '%s' "$line" | cut -d' ' -f1)
  path=$(printf '%s' "$line" | cut -d' ' -f3-)
  got=$(sha256sum -- "$path" | cut -d' ' -f1)
  [ "$got" = "$sum" ] || {{ echo "digest-mismatch $path"; exit 43; }}
done < "$inventory"
count=$(find . -path ./.effigy -prune -o -path "./$inventory" -prune -o -type f -print | wc -l | tr -d ' ')
[ "$count" = {expected_files} ] || {{ echo "file-count $count"; exit 44; }}
echo source-closure-verified
"#,
        inventory = effigy_core::shell::shell_quote(
            effigy_containers::artifact_source::STAGE_INVENTORY_FILE
        ),
        inventory_digest = effigy_core::shell::shell_quote(&staged.inventory_digest),
        expected_files = expected_files,
    );
    let mut args = ["exec", "-T", "-w", "/workspace", "builder"]
        .map(OsString::from)
        .to_vec();
    args.extend(["sh".to_owned(), "-lc".to_owned(), script].map(OsString::from));
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args,
        ContainerAction::Exec,
        "workspace artifact source probe",
    )?;
    let output = crate::runner::exec_command::run_verified_compose_exec_plan(
        policy, &plan, binding, true, None, None,
    )?;
    let reported = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() && reported == "source-closure-verified" {
        return Ok(());
    }
    let cause = if reported.is_empty() {
        format!("status {}", output.status)
    } else {
        format!(
            "status {}: {}",
            output.status,
            reported.lines().next().unwrap_or_default()
        )
    };
    Err(RunnerError::task_invocation(format!(
        "the Linux builder's /workspace does not carry the {expected_files} staged source files this build recorded ({cause}); nothing was compiled"
    )))
}

#[cfg(test)]
mod tests {
    use super::no_change_requested;

    #[test]
    fn no_change_switch_only_narrows_and_refuses_unknown_values() {
        assert!(!no_change_requested(None).expect("absent keeps acquisition"));
        assert!(no_change_requested(Some("1")).expect("exact one"));
        for raw in ["", "0", "true", " 1", "yes"] {
            assert!(
                no_change_requested(Some(raw)).is_err(),
                "{raw:?} must refuse"
            );
        }
    }
}
