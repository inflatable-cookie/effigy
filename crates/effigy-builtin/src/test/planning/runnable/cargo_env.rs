use std::collections::BTreeMap;
use std::path::Path;

use effigy_core::shell::shell_quote;

use effigy_manifest::ManifestCargoEnvMatchMode;

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

/// The program a command line names: the one fact this module decides from
/// rendered shell text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Program {
    /// A Cargo build, run directly or through Oxide's wrapper.
    Cargo,
    /// A shell that runs a nested command line.
    Shell,
    /// Anything else; the manifest's Cargo environment never applies.
    Other,
}

impl Program {
    /// Decode one executable token. A path to Cargo is still Cargo, and a path
    /// to a shell is still a shell.
    fn of(token: &str) -> Self {
        if matches!(token, "cargo" | "cargo-nextest" | "oxide")
            || token.ends_with("/cargo")
            || token.ends_with("/cargo-nextest")
            || token.ends_with("/oxide")
        {
            return Self::Cargo;
        }
        if matches!(
            token,
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "/bin/sh" | "/bin/bash" | "/bin/zsh"
        ) || token.ends_with("/sh")
            || token.ends_with("/bash")
            || token.ends_with("/zsh")
            || token.ends_with("/dash")
        {
            return Self::Shell;
        }
        Self::Other
    }
}

/// One command line, decoded once: the program it runs and, when that program
/// is a shell wrapper, the command line the shell runs instead.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandLine {
    program: Program,
    nested: Option<Box<CommandLine>>,
}

impl CommandLine {
    /// Whether running this line runs Cargo, through a shell wrapper or not.
    fn runs_cargo(&self) -> bool {
        self.program == Program::Cargo
            || self
                .nested
                .as_ref()
                .is_some_and(|nested| nested.runs_cargo())
    }

    /// The line's first whitespace token, with no prefix knowledge: what
    /// `executable-only` matching looks at.
    fn literal(command: &str) -> Option<Self> {
        Some(Self {
            program: Program::of(command.split_whitespace().next()?),
            nested: None,
        })
    }

    /// The program a prefixed line runs: skip `env` and its own arguments,
    /// `exec`/`command`, and leading `NAME=value` assignments.
    fn prefixed(command: &str) -> Option<Self> {
        let mut tokens = command.split_whitespace().peekable();
        tokens.peek()?;

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
                if is_env_assignment(token) {
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
        while tokens.peek().is_some_and(|token| is_env_assignment(token)) {
            tokens.next();
        }

        Some(Self {
            program: Program::of(tokens.next()?),
            nested: None,
        })
    }

    /// A shell wrapper: `sh -c`/`sh -lc` and the command line it runs.
    fn shell_wrapped(command: &str) -> Option<Self> {
        let mut tokens = command.split_whitespace();
        let shell = tokens.next()?;
        if Program::of(shell) != Program::Shell {
            return None;
        }
        let option = tokens.next()?;
        if option != "-c" && option != "-lc" {
            return None;
        }

        let nested = tokens.collect::<Vec<&str>>().join(" ");
        if nested.is_empty() {
            return None;
        }
        Some(Self {
            program: Program::Shell,
            nested: Some(Box::new(Self::prefixed(strip_wrapping_quotes(&nested))?)),
        })
    }
}

fn is_cargo_command_for_mode(command: &str, mode: ManifestCargoEnvMatchMode) -> bool {
    match mode {
        ManifestCargoEnvMatchMode::ExecutableOnly => {
            CommandLine::literal(command).is_some_and(|line| line.program == Program::Cargo)
        }
        ManifestCargoEnvMatchMode::PrefixAware => {
            CommandLine::prefixed(command).is_some_and(|line| line.program == Program::Cargo)
        }
        // A shell wrapper is only unwrapped when the line itself does not
        // already run Cargo: `sh -c '<cargo>'` and `env … cargo` both match,
        // and neither form hides the other.
        ManifestCargoEnvMatchMode::ShellAware => match CommandLine::prefixed(command) {
            Some(line) if line.program == Program::Cargo => true,
            _ => CommandLine::shell_wrapped(command).is_some_and(|line| line.runs_cargo()),
        },
    }
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

fn is_env_assignment(token: &&str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::{is_cargo_command_for_mode, CommandLine, Program};
    use effigy_manifest::ManifestCargoEnvMatchMode;

    fn cargo(mode: ManifestCargoEnvMatchMode, command: &str) -> bool {
        is_cargo_command_for_mode(command, mode)
    }

    #[test]
    fn executable_only_matches_the_literal_first_token() {
        let mode = ManifestCargoEnvMatchMode::ExecutableOnly;
        assert!(cargo(mode, "cargo nextest run"));
        assert!(cargo(mode, "/usr/local/bin/cargo test"));
        // A prefix means the first token is not the executable.
        assert!(!cargo(mode, "env CARGO_HOME=/cache cargo test"));
        assert!(!cargo(mode, "CARGO_HOME=/cache cargo test"));
        assert!(!cargo(mode, "sh -c 'cargo test'"));
        assert!(!cargo(mode, "npm run build"));
        assert!(!cargo(mode, ""));
    }

    #[test]
    fn prefix_aware_skips_env_exec_command_and_assignments() {
        let mode = ManifestCargoEnvMatchMode::PrefixAware;
        assert!(cargo(mode, "env CARGO_HOME=/cache cargo test"));
        assert!(cargo(mode, "env -i CARGO_HOME=/cache cargo test"));
        assert!(cargo(mode, "env -u RUSTUP_TOOLCHAIN cargo test"));
        assert!(cargo(mode, "env --unset=RUSTUP_TOOLCHAIN cargo test"));
        assert!(cargo(mode, "CARGO_HOME=/cache cargo test"));
        assert!(cargo(mode, "exec command cargo test"));
        assert!(cargo(mode, "oxide test --plan"));
        assert!(cargo(mode, "cargo-nextest run"));
        // A shell wrapper is not prefix-aware, and a non-Cargo executable
        // under the same prefixes still does not match.
        assert!(!cargo(mode, "sh -c 'cargo test'"));
        assert!(!cargo(mode, "env CARGO_HOME=/cache npm test"));
        assert!(!cargo(mode, "env"));
        assert!(!cargo(mode, "CARGO_HOME=/cache"));
    }

    #[test]
    fn shell_aware_unwraps_one_shell_wrapper() {
        let mode = ManifestCargoEnvMatchMode::ShellAware;
        assert!(cargo(mode, "sh -c 'cargo test'"));
        assert!(cargo(mode, "bash -lc \"cargo nextest run\""));
        assert!(cargo(mode, "sh -c 'env CARGO_HOME=/cache cargo test'"));
        // The wrapper must carry a Cargo command line, and only `-c`/`-lc`
        // are the wrapper form this mode understands.
        assert!(!cargo(mode, "sh -c 'npm test'"));
        assert!(!cargo(mode, "sh -e 'cargo test'"));
        assert!(!cargo(mode, "sh -c"));
        assert!(!cargo(mode, "npm test"));
    }

    #[test]
    fn one_decoder_names_the_program_a_line_runs() {
        assert_eq!(Program::of("cargo"), Program::Cargo);
        assert_eq!(Program::of("/opt/oxide"), Program::Cargo);
        assert_eq!(Program::of("zsh"), Program::Shell);
        assert_eq!(Program::of("/bin/dash"), Program::Shell);
        assert_eq!(Program::of("cargox"), Program::Other);

        let wrapped = CommandLine::shell_wrapped("sh -c 'cargo test'").expect("shell wrapper");
        assert_eq!(wrapped.program, Program::Shell);
        assert!(wrapped.runs_cargo());
        let prefixed = CommandLine::prefixed("env -i cargo test").expect("prefixed line");
        assert_eq!(prefixed.program, Program::Cargo);
    }
}
