//! Task-reference invocation parsing.
//!
//! Moved out of `src/runner/util/parsing/reference.rs` in batch 241 so
//! `effigy-managed`, the runner, and task-domain presentation surfaces
//! can reach these helpers from a shared crate.
//!
//! The error channel matches the rest of `effigy-tasks::parsing` —
//! string messages that callers wrap into their own error types
//! (`RunnerError::task_invocation`, `ManagedError::task_invocation`,
//! etc.) at the boundary.

use effigy_core::shell::shell_quote;

use crate::TaskSelector;

pub fn parse_task_reference_invocation(raw: &str) -> Result<(TaskSelector, Vec<String>), String> {
    let parts = split_task_reference_words(raw)?;
    let Some(selector_raw) = parts.first() else {
        return Err("task reference is required".to_owned());
    };
    let selector = crate::parse_task_selector(selector_raw)?;
    let args = parts.iter().skip(1).cloned().collect::<Vec<String>>();
    Ok((selector, args))
}

fn split_task_reference_words(raw: &str) -> Result<Vec<String>, String> {
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
        return Err("task reference has trailing escape (`\\`)".to_owned());
    }
    if in_single || in_double {
        return Err("task reference has an unterminated quote".to_owned());
    }
    if token_started {
        out.push(current);
    }
    Ok(out)
}

pub fn render_passthrough_args(args: &[String]) -> String {
    command_passthrough_args(args)
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Drop a leading `--` delimiter so `{args}` receives the tokens after it.
/// Nested `effigy` re-invocations still see the raw delimiter in `exec` argv.
pub fn command_passthrough_args(args: &[String]) -> &[String] {
    match args {
        [first, rest @ ..] if first == "--" => rest,
        other => other,
    }
}
