use std::collections::BTreeMap;
use std::path::Path;

use effigy_core::shell::shell_quote;

use crate::runner::manifest::ManifestCargoEnvMatchMode;

pub(super) fn maybe_wrap_with_cargo_env(
    command: String,
    cargo_env: &BTreeMap<String, String>,
    cargo_env_match: ManifestCargoEnvMatchMode,
    root: &Path,
) -> String {
    if cargo_env.is_empty() || !is_cargo_command_for_mode(&command, cargo_env_match) {
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

    format!("env {env_args} sh -c {}", shell_quote(&command))
}

fn is_cargo_command_for_mode(command: &str, mode: ManifestCargoEnvMatchMode) -> bool {
    match mode {
        ManifestCargoEnvMatchMode::ExecutableOnly => {
            let mut tokens = command.split_whitespace();
            let Some(executable) = tokens.next() else {
                return false;
            };
            is_cargo_executable_token(executable)
        }
        ManifestCargoEnvMatchMode::PrefixAware => is_prefixed_cargo_command(command),
        ManifestCargoEnvMatchMode::ShellAware => {
            is_prefixed_cargo_command(command) || is_shell_wrapped_cargo_command(command)
        }
    }
}

fn is_prefixed_cargo_command(command: &str) -> bool {
    let mut tokens = command.split_whitespace().peekable();
    if tokens.peek().is_none() {
        return false;
    }

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

    is_cargo_executable_token(executable)
}

fn is_shell_wrapped_cargo_command(command: &str) -> bool {
    let mut tokens = command.split_whitespace();
    let Some(shell) = tokens.next() else {
        return false;
    };
    if !is_shell_executable_token(shell) {
        return false;
    }
    let Some(option) = tokens.next() else {
        return false;
    };
    if option != "-c" && option != "-lc" {
        return false;
    }

    let nested = tokens.collect::<Vec<&str>>().join(" ");
    if nested.is_empty() {
        return false;
    }
    let nested = strip_wrapping_quotes(&nested);
    is_prefixed_cargo_command(nested)
}

fn strip_wrapping_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn is_shell_executable_token(token: &str) -> bool {
    matches!(
        token,
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "/bin/sh" | "/bin/bash" | "/bin/zsh"
    ) || token.ends_with("/sh")
        || token.ends_with("/bash")
        || token.ends_with("/zsh")
        || token.ends_with("/dash")
}

fn is_cargo_executable_token(executable: &str) -> bool {
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
