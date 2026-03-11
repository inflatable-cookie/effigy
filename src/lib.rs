//! Effigy is a policy-first task runner for monorepos and mixed-language repos.
//!
//! This crate exposes three main public surfaces:
//!
//! - CLI entrypoints and parsing helpers for embedding or testing the command
//!   surface
//! - the [`changelog`] library for Northstar changelog parsing, validation,
//!   formatting, analysis, and release-note extraction
//! - supporting runtime modules such as env-schema resolution, task routing,
//!   and process management
//!
//! Operator-focused guidance lives in the repository guides:
//!
//! - release operations: `docs/guides/051-release-orchestration.md`
//! - release/distribution policy: `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
//! - changelog workflow and Northstar profile usage:
//!   `docs/guides/052-changelog-workflows-and-northstar-profile.md`
//!
//! Library users looking for the changelog API should start with [`changelog`].

pub mod changelog;
mod cli;
mod data_loading;
pub mod env_schema;
mod fs_probe;
mod path_error_text;
mod path_probe;
pub mod process_manager;
pub mod resolver;
pub mod runner;
pub mod tasks;
pub mod testing;
pub mod tui;
pub mod ui;

pub use cli::entrypoint::run_cli;
pub use cli::execution_context::CliExecutionContext;
pub use cli::help_dispatch::{build_help_payload, run_help_command};
pub use cli::output::{
    command_kind_and_name, emit_json_envelope_error, emit_json_envelope_success,
    emit_json_envelope_success_value, help_topic_label, parse_json_or_string,
};
pub use cli::parse::{
    apply_global_json_flag, command_requests_json, parse_command, strip_global_json_flag,
    strip_global_json_flags, CliParseError,
};
pub use cli::parse_error::{parse_error_json_details, render_parse_error, PARSE_ERROR_HINT};
pub use cli::runner_dispatch::run_and_render_command;
use std::path::{Path, PathBuf};
use ui::{Renderer, UiResult};

/// Top-level parsed command used by the CLI dispatcher and tests.
///
/// Library consumers normally obtain this from [`parse_command`], then pass it
/// into [`run_and_render_command`] or their own dispatch layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Built-in changelog command family.
    Changelog(ChangelogArgs),
    /// Built-in release command family.
    Release(ReleaseArgs),
    /// Built-in doctor command family.
    Doctor(DoctorArgs),
    /// Built-in task-listing command family.
    Tasks(TasksArgs),
    /// Manifest-defined task invocation.
    Task(TaskInvocation),
    /// Help topic request.
    Help(HelpTopic),
}

/// Help topics supported by the built-in help renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    /// General top-level help.
    General,
    /// Changelog help.
    Changelog,
    /// Release help.
    Release,
    /// Doctor help.
    Doctor,
    /// Tasks help.
    Tasks,
    /// Built-in test orchestration help.
    Test,
    /// Watch help.
    Watch,
    /// Init help.
    Init,
    /// Migration help.
    Migrate,
}

/// Parsed arguments for the built-in changelog command family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogArgs {
    /// Which changelog subcommand should run.
    pub subcommand: ChangelogSubcommand,
    /// Optional file override. When absent, the CLI defaults to `CHANGELOG.md`.
    pub file: Option<PathBuf>,
    /// Whether the command should render JSON-compatible output.
    pub output_json: bool,
}

/// Supported changelog subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangelogSubcommand {
    /// Validate changelog structure and profile compliance.
    Validate,
    /// Format the changelog, optionally writing the result back to disk.
    Format { write: bool },
    /// Analyze `Unreleased` content and suggest the next version bump.
    Analyze,
    /// Extract one version body for release-note source material.
    Extract { version: String },
}

/// Parsed arguments for the built-in release command family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseArgs {
    /// Which release subcommand should run.
    pub subcommand: ReleaseSubcommand,
    /// Optional repository root override.
    pub repo_override: Option<PathBuf>,
    /// Whether the command should render JSON-compatible output.
    pub output_json: bool,
}

/// Supported release subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseSubcommand {
    /// Inspect release readiness and optionally execute configured gates.
    Status {
        /// Whether configured release gates should be run as part of status.
        check_gates: bool,
    },
    /// Run configured release gates as a standalone command.
    Gates,
    /// Recover or inspect an existing `.release-prepared.json` state file.
    Resume {
        /// Whether stale prepared state is explicitly allowed.
        allow_stale: bool,
    },
    /// Install and verify a tagged Effigy binary from git.
    VerifyInstall {
        /// Optional tag override. When absent, the caller must provide one
        /// through surrounding workflow context.
        tag: Option<String>,
        /// Optional git repository URL override for install verification.
        repo_url: Option<String>,
    },
    /// Dry-run the full release flow without writing files or state.
    Simulate {
        /// Optional semver override for previewing a non-default release.
        version_override: Option<String>,
    },
    /// Preview or apply release preparation mutations.
    Prepare {
        /// Whether this is the non-destructive preview path.
        plan: bool,
        /// Whether configured release gates should be run during prepare.
        check_gates: bool,
        /// Whether preparation should apply immediately without interactive
        /// confirmation.
        yes: bool,
        /// Optional semver override for the selected release version.
        version_override: Option<String>,
    },
    /// Preflight or perform the irreversible release execution step.
    Execute {
        /// Whether this is the non-destructive preflight path.
        plan: bool,
        /// Whether execution should proceed non-interactively.
        yes: bool,
        /// Whether stale prepared state is explicitly allowed.
        allow_stale: bool,
    },
}

/// Parsed arguments for the built-in doctor command family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorArgs {
    /// Optional repository root override.
    pub repo_override: Option<PathBuf>,
    /// Whether the command should render JSON-compatible output.
    pub output_json: bool,
    /// Whether doctor should attempt automatic fixes where supported.
    pub fix: bool,
    /// Whether verbose diagnostic output is requested.
    pub verbose: bool,
    /// Optional explain-mode task invocation.
    pub explain: Option<TaskInvocation>,
}

/// Parsed arguments for the built-in tasks command family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    /// Optional repository root override.
    pub repo_override: Option<PathBuf>,
    /// Optional task name filter.
    pub task_name: Option<String>,
    /// Optional selector resolution probe.
    pub resolve_selector: Option<String>,
    /// Whether the command should render JSON-compatible output.
    pub output_json: bool,
    /// Whether JSON output should be pretty-printed.
    pub pretty_json: bool,
}

/// A manifest-defined task plus passthrough arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInvocation {
    /// Task selector or task name.
    pub name: String,
    /// Additional arguments passed through to the task runtime.
    pub args: Vec<String>,
}

mod cli_help;

/// Render built-in help for a specific topic through the supplied renderer.
pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    cli_help::render_help(renderer, topic)
}

/// Render the standard CLI header for the supplied repository root.
pub fn render_cli_header<R: Renderer>(renderer: &mut R, root: &Path) -> UiResult<()> {
    cli_help::render_cli_header(renderer, root)
}

#[cfg(test)]
#[path = "tests/contract_test_support.rs"]
mod contract_test_support;

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
