use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::builtin::test::planning::{BuiltinTestRunnable, BuiltinTestTarget};
use crate::runner::util::shell_quote;

pub(super) fn collect_builtin_test_runnable_targets(
    targets: &[BuiltinTestTarget],
) -> Vec<BuiltinTestRunnable> {
    targets
        .iter()
        .flat_map(|target| {
            let plans = target.plans.clone();
            let multi = plans.len() > 1;
            plans
                .into_iter()
                .map(|plan| BuiltinTestRunnable {
                    name: if multi {
                        format!("{}/{}", target.name, plan.suite)
                    } else {
                        target.name.clone()
                    },
                    runner: plan.suite,
                    root: target.root.clone(),
                    command: maybe_wrap_with_cargo_env(
                        plan.command,
                        &target.cargo_env,
                        &target.root,
                    ),
                })
                .collect::<Vec<BuiltinTestRunnable>>()
        })
        .collect::<Vec<BuiltinTestRunnable>>()
}

pub(super) fn apply_passthrough_to_runnable(
    runnable: Vec<BuiltinTestRunnable>,
    passthrough: &[String],
) -> Vec<BuiltinTestRunnable> {
    let args_rendered = passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    runnable
        .into_iter()
        .map(|mut entry| {
            if !args_rendered.is_empty() {
                entry.command = format!("{} {}", entry.command, args_rendered);
            }
            entry
        })
        .collect::<Vec<BuiltinTestRunnable>>()
}

fn maybe_wrap_with_cargo_env(
    command: String,
    cargo_env: &BTreeMap<String, String>,
    root: &Path,
) -> String {
    if cargo_env.is_empty() || !is_cargo_command(&command) {
        return command;
    }

    let rendered_root = root.display().to_string();
    let env_args = cargo_env
        .iter()
        .map(|(key, value)| {
            let rendered = value
                .replace("{project}", &rendered_root)
                .replace("{repo}", &rendered_root);
            shell_quote(&format!("{key}={rendered}"))
        })
        .collect::<Vec<String>>()
        .join(" ");

    format!("env {env_args} sh -lc {}", shell_quote(&command))
}

fn is_cargo_command(command: &str) -> bool {
    let mut tokens = command.split_whitespace().peekable();
    if tokens.peek().is_none() {
        return false;
    }

    // Support common env/exec wrappers before the actual executable token.
    if tokens.peek().is_some_and(|token| *token == "env") {
        tokens.next();
        while let Some(token) = tokens.peek() {
            if *token == "-i" {
                tokens.next();
                continue;
            }
            if *token == "-u" {
                tokens.next();
                let _ = tokens.next();
                continue;
            }
            if token.starts_with('-') {
                tokens.next();
                continue;
            }
            if is_env_assignment_token(token) {
                tokens.next();
                continue;
            }
            break;
        }
    }

    while tokens
        .peek()
        .is_some_and(|token| *token == "exec" || *token == "command")
    {
        tokens.next();
    }
    while tokens.peek().is_some_and(is_env_assignment_token) {
        tokens.next();
    }

    let Some(executable) = tokens.next() else {
        return false;
    };

    executable == "cargo"
        || executable == "cargo-nextest"
        || executable.ends_with("/cargo")
        || executable.ends_with("/cargo-nextest")
}

fn is_env_assignment_token(token: &&str) -> bool {
    let Some((key, _)) = token.split_once('=') else {
        return false;
    };
    if key.is_empty() {
        return false;
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
