use effigy_tasks::TaskRuntimeArgs;

use crate::{ManagedError, DEFAULT_MANAGED_PROFILE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedInvocation {
    Run {
        headless: bool,
    },
    Status {
        profile: String,
    },
    Logs {
        profile: String,
        process: Option<String>,
        follow: bool,
    },
    Stop {
        profile: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedManagedInvocation {
    pub action: ManagedInvocation,
    pub runtime_args: TaskRuntimeArgs,
}

pub fn parse_managed_invocation(
    runtime_args: &TaskRuntimeArgs,
) -> Result<ParsedManagedInvocation, ManagedError> {
    let mut cleaned = runtime_args.clone();
    let mut args = cleaned.passthrough.clone();
    let headless = remove_flag(&mut args, "--headless");

    let action = match args.first().map(String::as_str) {
        Some("status") => {
            args.remove(0);
            ManagedInvocation::Status {
                profile: parse_profile_only(&args, "status")?,
            }
        }
        Some("stop") => {
            args.remove(0);
            ManagedInvocation::Stop {
                profile: parse_profile_only(&args, "stop")?,
            }
        }
        Some("logs") => {
            args.remove(0);
            parse_logs(&args)?
        }
        _ => {
            cleaned.passthrough = args;
            ManagedInvocation::Run { headless }
        }
    };

    Ok(ParsedManagedInvocation {
        action,
        runtime_args: cleaned,
    })
}

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let found = args.iter().any(|arg| arg == flag);
    args.retain(|arg| arg != flag);
    found
}

fn parse_profile_only(args: &[String], command: &str) -> Result<String, ManagedError> {
    if args.is_empty() {
        return Ok(DEFAULT_MANAGED_PROFILE.to_owned());
    }
    if args.len() == 1 && !args[0].starts_with('-') {
        return Ok(args[0].clone());
    }
    if args.len() == 2 && args[0] == "--profile" {
        return non_empty_value(&args[1], "--profile");
    }
    Err(ManagedError::task_invocation(format!(
        "managed `{command}` accepts only an optional profile name or `--profile <NAME>`"
    )))
}

fn parse_logs(args: &[String]) -> Result<ManagedInvocation, ManagedError> {
    let mut profile = DEFAULT_MANAGED_PROFILE.to_owned();
    let mut process = None;
    let mut follow = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--follow" | "-f" => follow = true,
            "--profile" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| {
                    ManagedError::task_invocation("managed `logs --profile` requires a value")
                })?;
                profile = non_empty_value(value, "--profile")?;
            }
            value if value.starts_with('-') => {
                return Err(ManagedError::task_invocation(format!(
                    "unknown managed logs argument `{value}`"
                )));
            }
            value if process.is_none() => process = Some(value.to_owned()),
            value => {
                return Err(ManagedError::task_invocation(format!(
                    "unexpected managed logs argument `{value}`"
                )));
            }
        }
        index += 1;
    }
    Ok(ManagedInvocation::Logs {
        profile,
        process,
        follow,
    })
}

fn non_empty_value(value: &str, flag: &str) -> Result<String, ManagedError> {
    if value.trim().is_empty() {
        return Err(ManagedError::task_invocation(format!(
            "managed `{flag}` requires a non-empty value"
        )));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{parse_managed_invocation, ManagedInvocation};
    use effigy_tasks::TaskRuntimeArgs;

    fn args(values: &[&str]) -> TaskRuntimeArgs {
        TaskRuntimeArgs {
            repo_override: None,
            verbose_root: false,
            env_schema_override: None,
            passthrough: values.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[test]
    fn headless_flag_is_removed_without_losing_profile() {
        let parsed = parse_managed_invocation(&args(&["admin", "--headless"])).expect("parse");
        assert_eq!(parsed.action, ManagedInvocation::Run { headless: true });
        assert_eq!(parsed.runtime_args.passthrough, ["admin"]);
    }

    #[test]
    fn logs_accepts_process_follow_and_profile() {
        let parsed =
            parse_managed_invocation(&args(&["logs", "api", "--follow", "--profile", "admin"]))
                .expect("parse");
        assert_eq!(
            parsed.action,
            ManagedInvocation::Logs {
                profile: "admin".to_owned(),
                process: Some("api".to_owned()),
                follow: true,
            }
        );
    }
}
