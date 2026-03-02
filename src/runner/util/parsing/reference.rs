use super::super::super::{RunnerError, TaskSelector};
use super::super::shell_quote;

pub(super) fn parse_task_reference_invocation(
    raw: &str,
) -> Result<(TaskSelector, Vec<String>), RunnerError> {
    let parts = split_task_reference_words(raw)?;
    let Some(selector_raw) = parts.first() else {
        return Err(RunnerError::TaskInvocation(
            "task reference is required".to_owned(),
        ));
    };
    let selector = super::selector::parse_task_selector(selector_raw)?;
    let args = parts.iter().skip(1).cloned().collect::<Vec<String>>();
    Ok((selector, args))
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

pub(super) fn render_passthrough_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ")
}
