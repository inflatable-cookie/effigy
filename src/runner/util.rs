use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use super::{RunnerError, TaskRuntimeArgs, TaskSelector};

pub(super) fn normalize_builtin_test_suite(raw: &str) -> Option<&'static str> {
    match raw {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

pub(super) fn parse_task_runtime_args(args: &[String]) -> Result<TaskRuntimeArgs, RunnerError> {
    let mut repo: Option<PathBuf> = None;
    let mut verbose_root = false;
    let mut passthrough: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--repo" {
            let Some(value) = args.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(
                    "task argument --repo requires a value".to_owned(),
                ));
            };
            repo = Some(PathBuf::from(value));
            i += 2;
            continue;
        }
        if arg == "--verbose-root" {
            verbose_root = true;
            i += 1;
            continue;
        }
        passthrough.push(arg.clone());
        i += 1;
    }
    Ok(TaskRuntimeArgs {
        repo_override: repo,
        verbose_root,
        passthrough,
    })
}

pub(super) fn parse_task_selector(raw: &str) -> Result<TaskSelector, RunnerError> {
    if let Some((prefix, task_name)) = raw.rsplit_once('/') {
        if prefix.trim().is_empty() || task_name.trim().is_empty() {
            return Err(RunnerError::TaskInvocation(
                "task name must be `<task>` or `<catalog>/<task>`".to_owned(),
            ));
        }
        return Ok(TaskSelector {
            prefix: Some(prefix.trim().to_owned()),
            task_name: task_name.trim().to_owned(),
        });
    }

    if raw.trim().is_empty() {
        return Err(RunnerError::TaskInvocation(
            "task name is required".to_owned(),
        ));
    }

    Ok(TaskSelector {
        prefix: None,
        task_name: raw.trim().to_owned(),
    })
}

pub(super) fn parse_task_reference_invocation(
    raw: &str,
) -> Result<(TaskSelector, Vec<String>), RunnerError> {
    let mut parts = raw.split_whitespace();
    let Some(selector_raw) = parts.next() else {
        return Err(RunnerError::TaskInvocation(
            "task reference is required".to_owned(),
        ));
    };
    let selector = parse_task_selector(selector_raw)?;
    let args = parts.map(str::to_owned).collect::<Vec<String>>();
    Ok((selector, args))
}

pub(super) fn render_task_selector(selector: &TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}

pub(super) fn with_local_node_bin_path(process: &mut ProcessCommand, cwd: &Path) {
    let local_bin = cwd.join("node_modules/.bin");
    if !local_bin.is_dir() {
        return;
    }
    let local_rendered = local_bin.display().to_string();
    let merged = match std::env::var("PATH") {
        Ok(path) if !path.is_empty() => format!("{local_rendered}:{path}"),
        _ => local_rendered,
    };
    process.env("PATH", merged);
}

pub(super) fn shell_quote(raw: &str) -> String {
    if raw.is_empty() {
        return "''".to_owned();
    }
    let escaped = raw.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}
