use std::path::PathBuf;

use super::super::{RunnerError, TaskRuntimeArgs, TaskSelector};
use super::shell_quote;

pub(in crate::runner) fn normalize_builtin_test_suite(raw: &str) -> Option<&'static str> {
    match raw {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

pub(in crate::runner) fn parse_task_runtime_args(
    args: &[String],
) -> Result<TaskRuntimeArgs, RunnerError> {
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

pub(in crate::runner) fn parse_task_selector(raw: &str) -> Result<TaskSelector, RunnerError> {
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

pub(in crate::runner) fn parse_task_reference_invocation(
    raw: &str,
) -> Result<(TaskSelector, Vec<String>), RunnerError> {
    let parts = split_task_reference_words(raw)?;
    let Some(selector_raw) = parts.first() else {
        return Err(RunnerError::TaskInvocation(
            "task reference is required".to_owned(),
        ));
    };
    let selector = parse_task_selector(selector_raw)?;
    let args = parts.iter().skip(1).cloned().collect::<Vec<String>>();
    Ok((selector, args))
}

pub(in crate::runner) fn render_task_selector(selector: &TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}

fn split_task_reference_words(raw: &str) -> Result<Vec<String>, RunnerError> {
    let mut out = Vec::<String>::new();
    let mut current = String::new();
    let mut token_started = false;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaping = false;

    for ch in raw.chars() {
        if escaping {
            current.push(ch);
            token_started = true;
            escaping = false;
            continue;
        }
        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
                token_started = true;
            }
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            } else if ch == '\\' {
                escaping = true;
            } else {
                current.push(ch);
                token_started = true;
            }
            continue;
        }

        match ch {
            '\'' => {
                in_single = true;
                token_started = true;
            }
            '"' => {
                in_double = true;
                token_started = true;
            }
            '\\' => {
                escaping = true;
                token_started = true;
            }
            c if c.is_whitespace() => {
                if token_started {
                    out.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            _ => {
                current.push(ch);
                token_started = true;
            }
        }
    }

    if escaping {
        return Err(RunnerError::TaskInvocation(
            "task reference has trailing escape (`\\`)".to_owned(),
        ));
    }
    if in_single || in_double {
        return Err(RunnerError::TaskInvocation(
            "task reference has an unterminated quote".to_owned(),
        ));
    }
    if token_started {
        out.push(current);
    }
    Ok(out)
}

pub(in crate::runner) fn render_passthrough_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ")
}
