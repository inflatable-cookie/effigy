use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;

/// Default starter emitted when no name is supplied.
///
/// Kept in sync with the `DEFAULT_STARTER` constant in `scaffold.rs`;
/// both point at the baseline scaffold promoted into the bundled
/// starter catalog in batch 1 of roadmap `g02.021`.
pub(super) const DEFAULT_STARTER: &str = "minimal";

/// What `effigy init` was asked to do.
pub(super) enum InitMode {
    /// Check or apply the default idempotent repo initiation surface.
    Ensure { mode: AgentInitMode },
    /// Emit a named starter into the target repo.
    Emit { starter_name: String },
    /// List registered starters instead of emitting.
    List,
}

#[derive(Clone, Copy)]
pub(super) enum AgentInitMode {
    Check,
    Apply,
    Repair,
}

pub(super) struct InitRequest {
    pub(super) mode: InitMode,
    pub(super) output_json: bool,
    pub(super) force: bool,
    pub(super) dry_run: bool,
}

pub(super) fn parse_init_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<InitRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut force = false;
    let mut dry_run = false;
    let mut list = false;
    let mut check = false;
    let mut apply = false;
    let mut repair = false;

    // Anything the flag matcher rejects is collected here; we partition it
    // into unknown flags vs. positional names below so the error message
    // can be specific to init's shape.
    let collected = parser.parse_loop_collect_unknown(|parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [
                ("--json", &mut output_json),
                ("--force", &mut force),
                ("--dry-run", &mut dry_run),
                ("--list", &mut list),
                ("--check", &mut check),
                ("--apply", &mut apply),
                ("--repair", &mut repair),
            ],
        ) {
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;

    let (unknown_flags, positional_names): (Vec<String>, Vec<String>) =
        collected.into_iter().partition(|arg| arg.starts_with('-'));
    if !unknown_flags.is_empty() {
        return Err(BuiltinError::task_invocation(format!(
            "`{}` received unknown flag(s): {}",
            task.name,
            unknown_flags.join(", ")
        )));
    }
    if positional_names.len() > 1 {
        return Err(BuiltinError::task_invocation(format!(
            "`{}` accepts at most one starter name; got: {}",
            task.name,
            positional_names.join(", ")
        )));
    }

    let starter_name = positional_names.into_iter().next();

    if list {
        if starter_name.is_some() {
            return Err(BuiltinError::task_invocation(
                "`effigy init --list` cannot be combined with a starter name",
            ));
        }
        if force || dry_run {
            return Err(BuiltinError::task_invocation(
                "`effigy init --list` cannot be combined with `--force` or `--dry-run`",
            ));
        }
        return Ok(InitRequest {
            mode: InitMode::List,
            output_json,
            force: false,
            dry_run: false,
        });
    }

    if check || apply || repair {
        if starter_name.is_some() {
            return Err(BuiltinError::task_invocation(
                "`--check`, `--apply`, and `--repair` cannot be combined with a starter name",
            ));
        }
        if force || dry_run {
            return Err(BuiltinError::task_invocation(
                "`effigy init --check|--apply|--repair` cannot be combined with `--force` or `--dry-run`",
            ));
        }
        let selected_modes = [check, apply, repair]
            .into_iter()
            .filter(|selected| *selected)
            .count();
        if selected_modes > 1 {
            return Err(BuiltinError::task_invocation(
                "`effigy init` accepts only one of `--check`, `--apply`, or `--repair`",
            ));
        }
        let mode = if apply {
            AgentInitMode::Apply
        } else if repair {
            AgentInitMode::Repair
        } else {
            AgentInitMode::Check
        };
        return Ok(InitRequest {
            mode: InitMode::Ensure { mode },
            output_json,
            force: false,
            dry_run: false,
        });
    }

    if starter_name.is_none() && !force && !dry_run {
        return Ok(InitRequest {
            mode: InitMode::Ensure {
                mode: AgentInitMode::Apply,
            },
            output_json,
            force: false,
            dry_run: false,
        });
    }

    let starter_name = starter_name.unwrap_or_else(|| DEFAULT_STARTER.to_string());
    Ok(InitRequest {
        mode: InitMode::Emit { starter_name },
        output_json,
        force,
        dry_run,
    })
}
