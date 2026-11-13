use super::status::annotate_host_process_status;
use crate::runner::container_command::gateway_registration::deregister_gateway_routes_for_container;
use crate::runner::container_command::support::{
    resolve_repo_root_or_invocation_cwd_scope, ContainerRepoScope,
};
use crate::runner::container_command::{closeout, deregister_runtime_gateway_routes, RunnerError};
use crate::runner::environment;
use effigy_containers::environment::Operation;
use effigy_containers::exec::shutdown_container as shutdown_container_via_exec;
use effigy_containers::{load_container_policy, EffectiveContainerPolicy};
use effigy_runtime::write::{
    run_container_down, run_container_down_all_with_hook, run_container_down_under_path_with_hook,
};
use std::path::Path;

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
