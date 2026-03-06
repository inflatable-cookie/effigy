use std::collections::BTreeMap;
use std::path::Path;

use super::super::super::util::shell_quote;
use super::RunSpecContext;
use crate::runner::error::RunnerError;

pub(super) fn render_task_command(command: &str, context: RunSpecContext<'_>) -> String {
    wrap_command_with_task_env(
        render_command_template(command, context.repo_root, context.args_rendered),
        context.task_env,
        context.repo_root,
    )
}

pub(super) fn render_step_command(command: &str, context: RunSpecContext<'_>) -> String {
    render_command_template(command, context.repo_root, context.args_rendered)
}

pub(super) fn render_builtin_task_reference_invocation(
    task_ref: &str,
    args_rendered: &str,
) -> Result<String, RunnerError> {
    let executable = resolve_effigy_invocation_prefix()?;
    let task = shell_quote(task_ref);
    if args_rendered.is_empty() {
        Ok(format!("{executable} {task}"))
    } else {
        Ok(format!("{executable} {task} {args_rendered}"))
    }
}

pub(super) fn wrap_command_with_cwd(cwd: &Path, command: &str) -> String {
    format!(
        "(cd {} && {})",
        shell_quote(&cwd.display().to_string()),
        command
    )
}

pub(super) fn render_command_template(
    command: &str,
    repo_root: &Path,
    args_rendered: &str,
) -> String {
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    command
        .replace("{project}", &repo_rendered)
        .replace("{repo}", &repo_rendered)
        .replace("{args}", args_rendered)
}

pub(super) fn wrap_command_with_task_env(
    command: String,
    task_env: &BTreeMap<String, String>,
    project_root: &Path,
) -> String {
    if task_env.is_empty() {
        return command;
    }

    let env_args = task_env
        .iter()
        .map(|(key, value)| {
            let rendered = render_task_env_value(value, project_root);
            shell_quote(&format!("{key}={rendered}"))
        })
        .collect::<Vec<String>>()
        .join(" ");
    format!("env {env_args} sh -lc {}", shell_quote(&command))
}

fn render_task_env_value(value: &str, project_root: &Path) -> String {
    let project_rendered = project_root.display().to_string();
    value
        .replace("{project}", &project_rendered)
        .replace("{repo}", &project_rendered)
}

fn resolve_effigy_invocation_prefix() -> Result<String, RunnerError> {
    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    let executable = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        let manifest_path = shell_quote(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }
    Ok(shell_quote(&executable.display().to_string()))
}
