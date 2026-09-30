//! Explicit Cargo package scope for built-in test routing.
//!
//! Rust detection adds `--workspace` when the target root declares a Cargo
//! `[workspace]`. Passthrough that selects packages explicitly (`-p`,
//! `--package`) must not widen to that workspace: Cargo treats the flags
//! additively, so `--workspace -p foo` runs every member. The auto-added
//! flag therefore yields when passthrough names packages. Passthrough that
//! carries its own workspace selection (`--workspace`, `--all`) also makes
//! the auto flag yield, because repeating the flag is a runner usage error;
//! the passthrough's own flag keeps the workspace scope. Combining an
//! explicit workspace selection with an explicit package selection is
//! rejected as ambiguous instead of silently broadening.
//!
//! Detection only reads the Cargo-level prefix of the passthrough: tokens
//! after the runner's own `--` belong to the test binary and are never
//! Cargo selectors.

use effigy_core::shell::shell_quote;

/// A composed suite command plus why the auto `--workspace` flag was
/// dropped, if it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::test) struct ScopedCommand {
    pub(in crate::test) command: String,
    /// The auto flag is absent from the composed command. True when
    /// passthrough names packages (the flag yielded to the explicit scope)
    /// or carries its own workspace selection (the flag yielded to avoid a
    /// duplicate the runners reject).
    pub(in crate::test) workspace_flag_dropped: bool,
    /// The auto flag yielded specifically because passthrough selected
    /// packages explicitly, narrowing the run below the whole workspace.
    pub(in crate::test) package_scope_narrowed: bool,
}

pub(in crate::test) fn compose_scoped_command(
    base_command: &str,
    auto_workspace_scope: bool,
    passthrough: &[String],
) -> ScopedCommand {
    let args_rendered = passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let package_scope = passthrough_has_explicit_package_selection(passthrough);
    let workspace_selection = passthrough_has_explicit_workspace_selection(passthrough);
    let package_scope_narrowed = auto_workspace_scope && package_scope && !workspace_selection;
    let drop_workspace_flag = auto_workspace_scope && (package_scope || workspace_selection);
    let base = if drop_workspace_flag {
        base_command
            .split_whitespace()
            .filter(|token| *token != WORKSPACE_FLAG)
            .collect::<Vec<&str>>()
            .join(" ")
    } else {
        base_command.to_owned()
    };
    let command = if args_rendered.is_empty() {
        base
    } else {
        format!("{base} {args_rendered}")
    };
    ScopedCommand {
        command,
        workspace_flag_dropped: drop_workspace_flag,
        package_scope_narrowed,
    }
}

const WORKSPACE_FLAG: &str = "--workspace";

/// The Cargo-level prefix of the passthrough. A `--` token inside the
/// passthrough is appended verbatim, so it becomes Cargo's own argument
/// separator: everything after it is forwarded to the test binary and is
/// never a Cargo package or workspace selector. Effigy's own `--` boundary
/// is consumed earlier by flag extraction, so any `--` found here is one the
/// runner will see.
fn cargo_level_passthrough(passthrough: &[String]) -> &[String] {
    match passthrough.iter().position(|arg| arg == "--") {
        Some(index) => &passthrough[..index],
        None => passthrough,
    }
}

/// True when passthrough names Cargo packages explicitly. Covers `-p <pkg>`,
/// `-p<pkg>`, `-p=<pkg>`, `--package <pkg>`, and `--package=<pkg>`; repeated
/// selections each count. A trailing `-p` with no value is still treated as
/// a package selection: the runner rejects the malformed argument instead of
/// the composition widening to the workspace. Tokens after the runner's
/// `--` are test-binary arguments and never count.
pub(in crate::test) fn passthrough_has_explicit_package_selection(passthrough: &[String]) -> bool {
    cargo_level_passthrough(passthrough).iter().any(|arg| {
        arg == "-p"
            || arg == "--package"
            || (arg.starts_with("--package=") && arg.len() > "--package=".len())
            || (arg.starts_with("-p") && arg.len() > 2)
    })
}

/// True when passthrough explicitly selects the workspace. Cargo accepts
/// `--workspace` and its `--all` alias on both `cargo test` and nextest;
/// neither runner accepts a `-w` short form, so none is recognized here.
/// Tokens after the runner's `--` are test-binary arguments and never count.
pub(in crate::test) fn passthrough_has_explicit_workspace_selection(
    passthrough: &[String],
) -> bool {
    cargo_level_passthrough(passthrough)
        .iter()
        .any(|arg| arg == WORKSPACE_FLAG || arg == "--all")
}

#[cfg(test)]
#[path = "scope/tests.rs"]
mod tests;
