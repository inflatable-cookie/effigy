use super::status::annotate_host_process_status;
use crate::runner::container_command::gateway_registration::deregister_gateway_routes_for_container;
use crate::runner::container_command::profile_inventory::{
    list_containers, list_networks, list_volumes,
};
use crate::runner::container_command::retire::{finish, release_gateway, retire_owned, same_path};
use crate::runner::container_command::support::{
    resolve_repo_root_or_invocation_cwd_scope, ContainerRepoScope,
};
use crate::runner::container_command::{closeout, deregister_runtime_gateway_routes, RunnerError};
use crate::runner::environment;
use effigy_containers::environment::Operation;
use effigy_containers::exec::shutdown_container as shutdown_container_via_exec;
use effigy_containers::{
    load_container_policy, named_volume_belongs_to, EffectiveContainerPolicy, OwnedResource,
    ResourceKind, RetireOutcome,
};
use effigy_runtime::write::{
    run_container_down, run_container_down_all_with_hook, run_container_down_under_path_with_hook,
};
use std::collections::BTreeSet;
use std::path::Path;

/// `container down --project <PROJECT> --profile <PROFILE>`: tear one project's
/// stack down on one profile, from any directory.
pub(in crate::runner::container_command) fn run_container_down_project_command(
    profile: &str,
    project: &str,
    remove_volumes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let outcome = teardown_project_on_profile(profile, project, remove_volumes)?;
    finish(outcome, None, output_json)
}

/// Remove the Effigy-owned stack of `project` on `profile`, without a checkout
/// or a manifest. It reads only that profile and never starts a stopped one.
///
/// Selects, in removal order, the project's Effigy-labelled containers (their
/// Compose project or `com.effigy.project` names `project`), its Compose
/// networks, and, only when `remove_volumes` is set, its volumes, including
/// persistent ones. Unlabelled and other projects' resources are never
/// touched. Gateway routes are withdrawn for each checkout a removed container
/// named, once no container on this profile still belongs to that checkout.
///
/// `Err` means the profile could not be read or a listing failed, so a caller
/// (Barge's lease sweep) keeps its lease and retries. A returned outcome that is
/// not `ok()` names the resources that remain.
pub(in crate::runner) fn teardown_project_on_profile(
    profile: &str,
    project: &str,
    remove_volumes: bool,
) -> Result<RetireOutcome, RunnerError> {
    let cwd = std::env::temp_dir();
    require_running_profile(&cwd, profile)?;

    let mut checkouts = BTreeSet::new();
    let mut selected = Vec::new();
    for row in list_containers(&cwd, profile)? {
        let belongs = row.project.as_deref() == Some(project)
            || row.effigy_project.as_deref() == Some(project);
        if !belongs || !row.carries_effigy_ownership() {
            continue;
        }
        if let Some(path) = row.owner_path() {
            checkouts.insert(path);
        }
        selected.push(row.resource(ResourceKind::Container, profile));
    }
    for name in list_networks(&cwd, profile, &[project.to_owned()])? {
        selected.push(OwnedResource {
            kind: ResourceKind::Network,
            name,
            profile: profile.to_owned(),
            persist: false,
        });
    }
    if remove_volumes {
        for row in list_volumes(&cwd, profile)? {
            if row.project.as_deref() != Some(project) {
                continue;
            }
            let owned = row.owner_path().is_some()
                || row.managed
                || named_volume_belongs_to(&row.name, project);
            if owned {
                selected.push(row.resource(profile));
            }
        }
    }

    let projects = BTreeSet::from([project.to_owned()]);
    for checkout in &checkouts {
        crate::runner::fs_relay::stop_checkout_stack_relays(
            Path::new(checkout),
            &projects,
            Some(profile),
        );
    }

    let mut outcome = if selected.is_empty() {
        RetireOutcome::default()
    } else {
        retire_owned(&cwd, &selected, false)?
    };

    if checkouts.is_empty() {
        return Ok(outcome);
    }
    let remaining = list_containers(&cwd, profile)?;
    for checkout in &checkouts {
        let still_in_use = remaining.iter().any(|row| {
            row.owner_path()
                .is_some_and(|path| same_path(&path, checkout))
        });
        if !still_in_use {
            outcome
                .gateway
                .routes
                .extend(release_gateway(checkout)?.routes);
        }
    }
    Ok(outcome)
}

/// A profile that is not running publishes nothing and cannot be read without
/// starting it, which a project teardown never does.
fn require_running_profile(cwd: &Path, profile: &str) -> Result<(), RunnerError> {
    let running =
        effigy_containers::exec::running_colima_profiles(cwd).map_err(RunnerError::from)?;
    if running.iter().any(|name| name == profile) {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "Colima profile `{profile}` is not running, so its containers cannot be read without starting it; start the profile and retry"
    )))
}

pub(in crate::runner::container_command) fn run_container_down_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    global: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if global {
        if repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container down --global` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_down_all_with_hook(
            output_json,
            deregister_runtime_gateway_routes,
            environment::stop_hook(Operation::Down),
        )
        .map_err(Into::into);
    }

    match resolve_repo_root_or_invocation_cwd_scope(repo_override)? {
        ContainerRepoScope::RepoRoot(repo_root) => {
            run_container_down_adapter(&repo_root, name, output_json)
        }
        ContainerRepoScope::InvocationCwd(cwd) => {
            let rendered = run_container_down_under_path_with_hook(
                &cwd,
                name,
                output_json,
                deregister_runtime_gateway_routes,
                environment::stop_hook(Operation::Down),
            )?;
            Ok(annotate_host_process_status(
                &cwd,
                name,
                output_json,
                rendered,
            ))
        }
    }
}

/// An attached bring-up that failed stops its scope as one stop transition,
/// after the reconcile run has closed.
pub(super) fn lifecycle_cleanup_failed_container_up(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    environment::stop(repo_root, policy, Operation::Down, |_| {
        crate::runner::fs_relay::stop_stack_relay(repo_root, policy);
        closeout::cleanup_failed_container_up(
            repo_root,
            policy,
            |repo_root, policy| {
                shutdown_container_via_exec(repo_root, policy).map_err(RunnerError::from)
            },
            |policy| deregister_gateway_routes_for_container(policy).map(|_| ()),
        )
    })
}

/// `container down`: one stop transition. It refuses while any run other
/// than a settling caller's own holds the environment (R4).
pub(super) fn run_container_down_adapter(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    load_container_policy(repo_root, name)?;
    let rendered = run_container_down(
        repo_root,
        name,
        output_json,
        deregister_runtime_gateway_routes,
        environment::stop_hook(Operation::Down),
    )
    .map_err(RunnerError::from)?;
    Ok(annotate_host_process_status(
        repo_root,
        name,
        output_json,
        rendered,
    ))
}

/// A managed run's settle releasing the environment it started: the
/// ordinary `container down`, exempting only `run`'s own hold, which the
/// kernel grants once that run is settling (contract 053 §4, R4).
pub(in crate::runner) fn release_managed_environment(
    repo_root: &Path,
    name: Option<&str>,
    run: effigy_kernel::id::RunId,
) -> Result<String, RunnerError> {
    load_container_policy(repo_root, name)?;
    run_container_down(
        repo_root,
        name,
        false,
        deregister_runtime_gateway_routes,
        environment::stop_hook_exempting(Operation::Down, Some(run)),
    )
    .map_err(RunnerError::from)
}
