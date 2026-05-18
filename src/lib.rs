//! Effigy is a policy-first task runner for monorepos and mixed-language repos.
//!
//! This crate exposes three main public surfaces:
//!
//! - CLI entrypoints and parsing helpers for embedding or testing the command
//!   surface ([`run_cli`]; the parsed command AST is `effigy_cli::parse_command`)
//! - the `effigy_changelog` crate for Northstar changelog parsing, validation,
//!   formatting, analysis, and release-note extraction
//! - supporting runtime modules such as env-schema resolution, task routing,
//!   and process management
//!
//! Operator-focused guidance lives in the repository guides:
//!
//! - guides map: `docs/guides/README.md`
//! - full command and flag lookup: `docs/guides/025-command-reference-matrix.md`
//! - JSON envelopes for automation: `docs/guides/017-json-output-contracts.md`
//! - env schema and `--env-schema` overrides: `docs/guides/050-env-schema-integration.md`
//! - release operations: `docs/guides/051-release-orchestration.md`
//! - release/distribution policy: `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
//! - changelog workflow and Northstar profile usage:
//!   `docs/guides/052-changelog-workflows-and-northstar-profile.md`
//!
//! Library users looking for the changelog API should depend on
//! `effigy-changelog` directly.

mod cli;
pub mod runner;
pub mod tui;

pub use cli::entrypoint::run_and_render_command;
pub use cli::entrypoint::run_cli;
pub use cli::execution_context::CliExecutionContext;
pub use cli::graph_watch_dispatch::run_graph_watch_command;
pub use cli::help_dispatch::{build_help_payload, run_help_command};
pub use cli::output::{
    build_binary_metadata, command_kind_and_name, emit_json_envelope_error,
    emit_json_envelope_success, emit_json_envelope_success_value, help_topic_label,
    parse_json_or_string,
};
pub use cli::parse_error::{parse_error_json_details, render_parse_error, PARSE_ERROR_HINT};
pub use cli::version_dispatch::{build_version_payload, run_version_command};
use effigy_ui::{Renderer, UiResult};
use std::path::Path;

/// Render the standard CLI header for the supplied repository root.
///
/// Thin wrapper that forwards to [`effigy_cli::header::render_cli_header`]
/// with the active binary display version, so local bootstrap installs can
/// surface a build suffix without changing release semver.
pub fn render_cli_header<R: Renderer>(renderer: &mut R, root: &Path) -> UiResult<()> {
    effigy_cli::header::render_cli_header(
        renderer,
        root,
        &effigy_core::build_info::display_version(),
    )
}

#[cfg(test)]
#[path = "tests/contract_test_support.rs"]
mod contract_test_support;

#[cfg(test)]
#[path = "../tests/shared/deploy_fixture_support.rs"]
mod deploy_fixture_support;

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/testing_tests.rs"]
mod testing_tests;
