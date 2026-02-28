use std::fs;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use serde_json::json;

use crate::resolver::resolve_target_root;
use crate::TaskInvocation;

use super::catalog::select_catalog_and_task;
use super::deferral::{run_deferred_request, select_deferral, should_attempt_deferral};
use super::locking::{acquire_scopes, LockScope};
use super::managed::{render_task_run_spec, resolve_managed_task_plan, run_or_render_managed_task};
use super::render::render_task_resolution_trace;
use super::util::{
    parse_task_runtime_args, parse_task_selector, shell_quote, with_local_node_bin_path,
};
use super::{
    discover_catalogs, try_run_builtin_task, LoadedCatalog, ManifestManagedRun, ManifestTask,
    RunnerError,
};

pub(super) fn task_run_preview(task: &ManifestTask) -> String {
    if let Some(run) = task.run.as_ref() {
        return match run {
            ManifestManagedRun::Command(command) => command.clone(),
            ManifestManagedRun::Sequence(steps) => format!("<sequence:{}>", steps.len()),
        };
    }
    if let Some(mode) = task.mode.as_ref() {
        return format!("<managed:{mode}>");
    }
    "<none>".to_owned()
}

pub(super) fn catalog_task_label(catalog: &LoadedCatalog, task_name: &str) -> String {
    if catalog.depth == 0 {
        task_name.to_owned()
    } else {
        format!("{}/{}", catalog.alias, task_name)
    }
}

pub(super) fn run_manifest_task(task: &TaskInvocation) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    run_manifest_task_with_cwd(task, cwd)
}

pub(super) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    let invocation_cwd = fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
    let runtime_args = parse_task_runtime_args(&task.args)?;
    let (passthrough_without_json, output_json) = strip_task_json_flag(&runtime_args.passthrough);
    let runtime_args_for_execution = super::TaskRuntimeArgs {
        repo_override: runtime_args.repo_override.clone(),
        verbose_root: runtime_args.verbose_root,
        passthrough: passthrough_without_json,
    };
    let resolved = resolve_target_root(cwd, runtime_args.repo_override.clone())?;
    let selector = parse_task_selector(&task.name)?;
    let catalogs = match discover_catalogs(&resolved.resolved_root) {
        Ok(catalogs) => catalogs,
        Err(RunnerError::TaskCatalogsMissing { .. }) => Vec::new(),
        Err(error) => return Err(error),
    };
    let selection = match select_catalog_and_task(&selector, &catalogs, &invocation_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            if matches!(selector.task_name.as_str(), "repo-pulse" | "health") {
                let request = selector
                    .prefix
                    .as_ref()
                    .map(|prefix| format!("{prefix}/{}", selector.task_name))
                    .unwrap_or_else(|| selector.task_name.clone());
                return Err(RunnerError::TaskInvocation(format!(
                    "`{request}` is no longer a built-in command. Use `effigy doctor` for consolidated health checks, or define `tasks.health` in your manifest for project-owned checks."
                )));
            }
            if let Some(output) = try_run_builtin_task(
                &selector,
                task,
                &runtime_args,
                &resolved.resolved_root,
                &catalogs,
                &invocation_cwd,
            )? {
                return Ok(output);
            }
            if should_attempt_deferral(&error) {
                if let Some(deferral) = select_deferral(
                    &selector,
                    &catalogs,
                    &invocation_cwd,
                    &resolved.resolved_root,
                ) {
                    return run_deferred_request(task, &runtime_args, &deferral, &error);
                }
            }
            return Err(error);
        }
    };

    let repo_for_task = selection.catalog.catalog_root.clone();
    if let Some(plan) = resolve_managed_task_plan(
        &selector,
        selection.catalog,
        selection.task,
        &runtime_args_for_execution,
        &catalogs,
        &selection.catalog.catalog_root,
    )? {
        let _lock_guards = acquire_scopes(
            &resolved.resolved_root,
            &[
                LockScope::Workspace,
                LockScope::Task(selector.task_name.clone()),
                LockScope::Profile {
                    task: selector.task_name.clone(),
                    profile: plan.profile.clone(),
                },
            ],
        )?;
        return run_or_render_managed_task(
            &selector.task_name,
            &repo_for_task,
            &selection.catalog.manifest_path,
            plan,
        );
    }

    let args_rendered = runtime_args_for_execution
        .passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let run_spec =
        selection
            .task
            .run
            .as_ref()
            .ok_or_else(|| RunnerError::TaskMissingRunCommand {
                task: selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            })?;
    let command = render_task_run_spec(
        &selector.task_name,
        run_spec,
        &args_rendered,
        &selection.catalog.catalog_root,
        &catalogs,
        &selection.catalog.catalog_root,
        0,
    )?;
    let _lock_guards = acquire_scopes(
        &resolved.resolved_root,
        &[
            LockScope::Workspace,
            LockScope::Task(selector.task_name.clone()),
        ],
    )?;

    let mut process = ProcessCommand::new("sh");
    process.arg("-lc").arg(&command).current_dir(&repo_for_task);
    with_local_node_bin_path(&mut process, &repo_for_task);
    if output_json {
        let output = process
            .output()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: command.clone(),
                error,
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let rendered = render_task_command_json(
            &selector.task_name,
            &selector,
            &repo_for_task,
            &command,
            output.status.code(),
            &stdout,
            &stderr,
        )?;
        if output.status.success() {
            return Ok(rendered);
        }
        return Err(RunnerError::CommandJsonFailure { rendered });
    }

    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.clone(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            let trace = render_task_resolution_trace(
                &resolved,
                &selector,
                &selection,
                &repo_for_task,
                &command,
            );
            return Ok(trace);
        }
        return Ok(String::new());
    }

    Err(RunnerError::TaskCommandFailure {
        command,
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn strip_task_json_flag(args: &[String]) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg.clone());
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg.clone());
    }
    (stripped, json_mode)
}

fn render_task_command_json(
    task_name: &str,
    selector: &super::TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<String, RunnerError> {
    let selector_rendered = selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone());
    let payload = json!({
        "schema": "effigy.task.run.v1",
        "schema_version": 1,
        "ok": exit_code == Some(0),
        "task": task_name,
        "selector": selector_rendered,
        "command": command,
        "cwd": cwd.display().to_string(),
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}
