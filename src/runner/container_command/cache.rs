use effigy_runtime::data::{
    run_container_cache_list, run_container_cache_list_all, run_container_cache_list_under_path,
    run_container_cache_prune, run_container_cache_prune_all,
};

use crate::runner::command_context::{active_invocation_cwd, resolve_active_command_context};
use crate::runner::error::RunnerError;

use super::data::maybe_confirm_destructive_container_action;
use super::runtime_volume_capture;
use super::support::{resolve_repo_root_or_invocation_cwd_scope, ContainerRepoScope};

pub(super) fn run_container_cache_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    subcommand: &effigy_cli::ContainerCacheSubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    match subcommand {
        effigy_cli::ContainerCacheSubcommand::List {
            global: true,
            project,
            kind,
        } => {
            if repo_override.is_some() {
                return Err(RunnerError::task_invocation(
                    "`effigy container cache list --global` does not accept `--repo`; it inspects the Effigy Colima profile's named-volume inventory",
                ));
            }
            let cwd = active_invocation_cwd()?;
            run_container_cache_list_all(
                &cwd,
                None,
                project.as_deref(),
                kind.as_deref(),
                output_json,
                runtime_volume_capture,
            )
            .map_err(Into::into)
        }
        effigy_cli::ContainerCacheSubcommand::List { global: false, .. } => {
            run_container_cache_list_fallback(repo_override, name, output_json)
        }
        effigy_cli::ContainerCacheSubcommand::Prune {
            global: true,
            yes,
            project,
            kind,
        } => {
            if repo_override.is_some() {
                return Err(RunnerError::task_invocation(
                    "`effigy container cache prune --global` does not accept `--repo`; it prunes cache volumes from the Effigy Colima profile inventory",
                ));
            }
            maybe_confirm_destructive_container_action(
                "`effigy container cache prune --global`",
                "Purge safe cache volumes across the Effigy Colima profile. Running projects will be skipped.",
                output_json,
                *yes,
            )?;
            let cwd = active_invocation_cwd()?;
            run_container_cache_prune_all(
                &cwd,
                None,
                project.as_deref(),
                kind.as_deref(),
                output_json,
                runtime_volume_capture,
            )
            .map_err(Into::into)
        }
        effigy_cli::ContainerCacheSubcommand::Prune {
            global: false, yes, ..
        } => {
            let context = resolve_active_command_context(repo_override)?;
            let repo_root = &context.resolved.resolved_root;
            let policy = effigy_containers::load_container_policy(repo_root, name)?;
            maybe_confirm_destructive_container_action(
                &format!("`effigy container {} cache prune`", policy.name),
                &format!(
                    "Purge safe cache volumes for container `{}`. The container must be stopped first.",
                    policy.name
                ),
                output_json,
                *yes,
            )?;
            run_container_cache_prune(repo_root, name, output_json, runtime_volume_capture)
                .map_err(Into::into)
        }
    }
}

fn run_container_cache_list_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    run_container_cache_list(repo_root, name, output_json, runtime_volume_capture)
        .map_err(Into::into)
}

fn run_container_cache_list_fallback(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    match resolve_repo_root_or_invocation_cwd_scope(repo_override)? {
        ContainerRepoScope::RepoRoot(repo_root) => {
            run_container_cache_list_adapter(&repo_root, name, output_json)
        }
        ContainerRepoScope::InvocationCwd(cwd) => {
            run_container_cache_list_under_path(&cwd, name, output_json, runtime_volume_capture)
                .map_err(Into::into)
        }
    }
}
