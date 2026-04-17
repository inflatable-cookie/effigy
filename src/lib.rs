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

mod cli;
mod data_loading;
pub mod env_schema;
pub mod runner;
pub mod tasks;
pub mod testing;
pub mod tui;

pub use effigy_core::resolver;
pub(crate) use effigy_core::{fs_probe, path_error_text, path_probe};

pub use cli::entrypoint::run_cli;
pub use cli::execution_context::CliExecutionContext;
pub use cli::help_dispatch::{build_help_payload, run_help_command};
pub use cli::output::{
    command_kind_and_name, emit_json_envelope_error, emit_json_envelope_success,
    emit_json_envelope_success_value, help_topic_label, parse_json_or_string,
};
pub use cli::parse_error::{parse_error_json_details, render_parse_error, PARSE_ERROR_HINT};
pub use cli::runner_dispatch::run_and_render_command;
pub use cli::version_dispatch::{build_version_payload, run_version_command};
pub use cli_help::render_help_with_deferred_builtins;
pub use effigy_changelog as changelog;
pub use effigy_cli::{
    apply_global_json_flag, command_requests_json, parse_command, strip_global_json_flag,
    strip_global_json_flags, BootstrapArgs, ChangelogArgs, ChangelogSubcommand, CliParseError,
    Command, ContainerArgs, ContainerSubcommand, ContractsArgs, ContractsCheckMode,
    ContractsSelectionPrintMode, ContractsSubcommand, DemoArgs, DemoHistoryOutcome, DemoListGap,
    DemoListGroupBy, DemoListMode, DemoListQuery, DemoListStatus, DemoSubcommand, DistributionArgs,
    DistributionSubcommand, DocsArgs, DocsBlockRequirement, DocsSubcommand, DoctorArgs, HelpTopic,
    InternalRhaiArgs, ReleaseArgs, ReleaseSubcommand, TaskInvocation, TasksArgs,
};
use effigy_ui::{Renderer, UiResult};
use std::path::Path;

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
