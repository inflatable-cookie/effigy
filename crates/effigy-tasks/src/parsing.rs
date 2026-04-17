use std::path::PathBuf;

use crate::{TaskRuntimeArgs, TaskSelector};

pub fn normalize_builtin_test_suite(raw: &str) -> Option<&'static str> {
    match raw {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

pub fn parse_task_runtime_args(args: &[String]) -> Result<TaskRuntimeArgs, String> {
    let mut repo: Option<PathBuf> = None;
    let mut verbose_root = false;
    let mut env_schema_override: Option<PathBuf> = None;
    let mut passthrough: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--repo" {
            let Some(value) = args.get(i + 1) else {
                return Err("task argument --repo requires a value".to_owned());
            };
            repo = Some(PathBuf::from(value));
            i += 2;
            continue;
        }
        if arg == "--env-schema" {
            let Some(value) = args.get(i + 1) else {
                return Err("task argument --env-schema requires a value".to_owned());
            };
            env_schema_override = Some(PathBuf::from(value));
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
        env_schema_override,
        passthrough,
    })
}

pub fn parse_task_selector(raw: &str) -> Result<TaskSelector, String> {
    if let Some((prefix, task_name)) = raw.rsplit_once('/') {
        if prefix.trim().is_empty() || task_name.trim().is_empty() {
            return Err("task name must be `<task>` or `<catalog>/<task>`".to_owned());
        }
        return Ok(TaskSelector {
            prefix: Some(prefix.trim().to_owned()),
            task_name: task_name.trim().to_owned(),
        });
    }

    if raw.trim().is_empty() {
        return Err("task name is required".to_owned());
    }

    Ok(TaskSelector {
        prefix: None,
        task_name: raw.trim().to_owned(),
    })
}

pub fn render_task_selector(selector: &TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}

#[cfg(test)]
#[path = "parsing/tests.rs"]
mod tests;
