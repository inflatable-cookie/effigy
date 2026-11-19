//! `effigy state artifact inspect|stage|capture`.
//!
//! v0.16 retired the layered state workflow (`state plan`, `apply`, `capture`,
//! `capture-set`, `history` and their `[state.*]` stacks); see
//! `docs/knowledge/deployment-data-workflows.md`. Only the plain artifact verbs
//! remain, and they live in [`super::state_command_artifact`].

use effigy_cli::StateArgs;

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::error::RunnerError;
use crate::runner::state_command_artifact::run_state_artifact;

pub(super) fn run_state(args: StateArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    run_state_artifact(
        args.subcommand,
        &context.resolved.resolved_root,
        &context.invocation_cwd,
        args.output_json,
    )
}
