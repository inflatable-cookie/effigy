use std::path::Path;

use effigy_containers::EffectiveContainerPolicy;

use super::RunnerError;
use crate::runner::script_command::execute_repo_rhai_script;

pub(super) fn execute_pull_production_hook(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    hook: &str,
) -> Result<(), RunnerError> {
    let hook = hook.trim();
    if hook.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` has an empty `pull_production` hook declaration",
            policy.name
        )));
    }

    let hook_args = vec![policy.name.clone()];
    if let Some(path) = hook.strip_prefix("rhai:") {
        let script = resolve_repo_relative_hook_path(
            repo_root,
            path.trim(),
            &format!("containers.{}.data.pull_production", policy.name),
        )?;
        return execute_repo_rhai_script(
            repo_root,
            &format!("container:{}:data:pull-production", policy.name),
            &script,
            &hook_args,
        );
    }

    let script = resolve_repo_relative_hook_path(
        repo_root,
        hook,
        &format!("containers.{}.data.pull_production", policy.name),
    )?;
    let output = std::process::Command::new("sh")
        .arg(&script)
        .arg(&policy.name)
        .current_dir(repo_root)
        .env("EFFIGY_CONTAINER_NAME", &policy.name)
        .env("EFFIGY_CONTAINER_PROFILE", &policy.profile)
        .env("EFFIGY_CONTAINER_PROJECT_NAME", &policy.project_name)
        .env("EFFIGY_CONTAINER_PRIMARY_SERVICE", &policy.primary_service)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("pull_production hook ({})", script.display()),
            error,
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RunnerError::task_invocation(format!(
            "pull_production hook for container `{}` failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
            policy.name,
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

fn resolve_repo_relative_hook_path(
    repo_root: &Path,
    raw: &str,
    field: &str,
) -> Result<std::path::PathBuf, RunnerError> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(RunnerError::task_invocation(format!(
            "`{field}` must stay repo-relative in this batch"
        )));
    }
    Ok(repo_root.join(path))
}
