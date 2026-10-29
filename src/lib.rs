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

pub use cli::entrypoint::run_and_render_command;
pub use cli::entrypoint::run_cli;
pub use cli::execution_context::CliExecutionContext;
pub use cli::help_dispatch::{
    build_help_group_payload_for_root, build_help_payload, run_help_command, run_help_group_command,
};
pub use cli::output::{
    build_binary_metadata, command_kind_and_name, emit_json_envelope_error,
    emit_json_envelope_error_with_human, emit_json_envelope_success,
    emit_json_envelope_success_value, help_topic_label, parse_json_or_string,
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
    if effigy_ui::steps::in_managed_pane() {
        return Ok(());
    }
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

/// Main-only kernel helper and signal dispatch, before command discovery.
pub fn run_main(raw_args: Vec<String>) -> std::io::Result<()> {
    use effigy_kernel::signal::{RouterPolicy, SignalRouter};
    // The session access a parent Effigy process handed this one is read from
    // its one-shot descriptor here, before any command or helper runs: never
    // argv, never the environment (contract 032, 052 K12; effigy#338).
    runner::adopt_session_secret_access();
    let entry = raw_args.first().map(String::as_str).unwrap_or("");
    // The per-run sentinel, supervisor and probe guard re-exec this binary
    // (contract 052 §5.8) before any CLI parsing, signal handling or spawn.
    if let Some(code) = runner::run_internal_kernel_entry(entry) {
        std::process::exit(code);
    }
    runner::cleanup_managed_secrets()?;
    runner::capture_startup_executable();
    // Source preparation and operator gateway commands own the router from
    // the start; nested task runs adopt it before custody spawns.
    let uses_kernel = effigy_cli::parse_command(raw_args.clone())
        .as_ref()
        .is_ok_and(|command| {
            runner::bundle_command::is_source_prepare_command(command)
                || matches!(command, effigy_cli::Command::Gateway(args)
                    if args.private.is_none() && matches!(args.subcommand,
                        effigy_cli::GatewaySubcommand::Up
                        | effigy_cli::GatewaySubcommand::Setup { .. }
                        | effigy_cli::GatewaySubcommand::SetupTls))
        });
    if uses_kernel {
        let router = SignalRouter::install(RouterPolicy::Inline)?;
        runner::bundle_command::set_source_cancel(router.root_cancel());
        runner::adopt_signal_router(router);
    }
    run_cli(raw_args);
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/route_table_fixture.rs"]
mod route_table_fixture;
