use std::path::Path;
use std::process::Command as ProcessCommand;

use crate::ui::KeyValue;
use crate::ui::Renderer;
use crate::TaskInvocation;

use super::render::trace_renderer;
use super::{
    shell_quote, with_local_node_bin_path, DeferredCommand, LoadedCatalog, RunnerError,
    TaskRuntimeArgs, TaskSelector, DEFER_DEPTH_ENV, IMPLICIT_ROOT_DEFER_TEMPLATE,
};

pub(super) fn should_attempt_deferral(error: &RunnerError) -> bool {
    matches!(
        error,
        RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
    )
}

pub(super) fn select_deferral(
    selector: &TaskSelector,
    catalogs: &[LoadedCatalog],
    cwd: &Path,
    workspace_root: &Path,
) -> Option<DeferredCommand> {
    if let Some(prefix) = &selector.prefix {
        if let Some(explicit) = catalogs.iter().find(|catalog| &catalog.alias == prefix) {
            if let Some(template) = explicit.defer_run.as_ref() {
                return Some(DeferredCommand {
                    template: template.clone(),
                    working_dir: explicit.catalog_root.clone(),
                    source: format!(
                        "catalog {} ({})",
                        explicit.alias,
                        explicit.manifest_path.display()
                    ),
                });
            }
        }
    }

    let mut in_scope = catalogs
        .iter()
        .filter(|catalog| catalog.defer_run.is_some() && cwd.starts_with(&catalog.catalog_root))
        .collect::<Vec<&LoadedCatalog>>();
    in_scope.sort_by(|a, b| {
        b.depth
            .cmp(&a.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    if let Some(catalog) = in_scope.first() {
        if let Some(template) = catalog.defer_run.as_ref() {
            return Some(DeferredCommand {
                template: template.clone(),
                working_dir: catalog.catalog_root.clone(),
                source: format!(
                    "catalog {} ({})",
                    catalog.alias,
                    catalog.manifest_path.display()
                ),
            });
        }
    }

    let mut fallback = catalogs
        .iter()
        .filter(|catalog| catalog.defer_run.is_some())
        .collect::<Vec<&LoadedCatalog>>();
    fallback.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    if let Some(catalog) = fallback.first() {
        if let Some(template) = catalog.defer_run.as_ref() {
            return Some(DeferredCommand {
                template: template.clone(),
                working_dir: catalog.catalog_root.clone(),
                source: format!(
                    "catalog {} ({})",
                    catalog.alias,
                    catalog.manifest_path.display()
                ),
            });
        }
    }

    infer_implicit_root_deferral(workspace_root)
}

pub(super) fn run_deferred_request(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
) -> Result<String, RunnerError> {
    let current_depth = std::env::var(DEFER_DEPTH_ENV)
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    if current_depth >= 1 {
        return Err(RunnerError::DeferLoopDetected {
            depth: current_depth,
        });
    }

    let args_rendered = runtime_args.passthrough.join(" ");
    let request_rendered = task.name.clone();
    let repo_rendered = shell_quote(&deferral.working_dir.display().to_string());
    let command = deferral
        .template
        .replace("{request}", &request_rendered)
        .replace("{args}", &args_rendered)
        .replace("{repo}", &repo_rendered);

    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "sh".to_owned());
    let shell_arg = if shell.ends_with("zsh") || shell.ends_with("bash") {
        "-ic"
    } else {
        "-lc"
    };
    let mut process = ProcessCommand::new(&shell);
    process
        .arg(shell_arg)
        .arg(&command)
        .current_dir(&deferral.working_dir)
        .env(DEFER_DEPTH_ENV, (current_depth + 1).to_string());
    with_local_node_bin_path(&mut process, &deferral.working_dir);
    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.clone(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            return Ok(render_deferral_trace(task, deferral, &command, cause));
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

fn infer_implicit_root_deferral(workspace_root: &Path) -> Option<DeferredCommand> {
    let has_effigy_json = workspace_root.join("effigy.json").is_file();
    let has_composer_json = workspace_root.join("composer.json").is_file();
    if has_effigy_json && has_composer_json {
        return Some(DeferredCommand {
            template: IMPLICIT_ROOT_DEFER_TEMPLATE.to_owned(),
            working_dir: workspace_root.to_path_buf(),
            source: "implicit root deferral (composer.json + effigy.json)".to_owned(),
        });
    }
    None
}

fn render_deferral_trace(
    task: &TaskInvocation,
    deferral: &DeferredCommand,
    command: &str,
    cause: &RunnerError,
) -> String {
    let mut renderer = trace_renderer();
    let _ = renderer.section("Task Deferral");
    let _ = renderer.key_values(&[
        KeyValue::new("request", task.name.clone()),
        KeyValue::new("defer-source", deferral.source.clone()),
        KeyValue::new("working-dir", deferral.working_dir.display().to_string()),
        KeyValue::new("command", command.to_owned()),
        KeyValue::new("reason", cause.to_string()),
    ]);
    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}
