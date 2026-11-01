//! Guest-side Level 1 command filter. Host resolution remains authoritative.
//!
//! This filter is an early refusal only; passing it does not grant authority.
//! In particular, the host must call `unsupported_resolved_task` after resolving
//! a task's lock and control action from its own trusted catalog.

use effigy_cli::{Command, ContainerSubcommand, RunsAction};

const OTHER: &str = "workspace Level 2 unsupported: operation; invoke the host CLI independently";
const CONTAINER: &str =
    "workspace Level 2 unsupported: container operation; invoke the host CLI independently";
const RUN_LOGS: &str = "workspace Level 2 unsupported: run logs; invoke the host CLI independently";
const GATEWAY: &str = "workspace Level 2 unsupported: gateway; invoke the host CLI independently";
const RELEASE: &str = "workspace Level 2 unsupported: release; invoke the host CLI independently";
const INSTALL: &str =
    "workspace Level 2 unsupported: install or uninstall; invoke the host CLI independently";
const SHARED_LOCK: &str =
    "workspace Level 2 unsupported: shared-lock task; invoke the host CLI independently";
const PAUSE: &str = "workspace Level 2 unsupported: pause; invoke the host CLI independently";
const UNKNOWN_TASK: &str = "workspace task policy unknown; no operation admitted";

/// Return a stable, argument-free refusal for classes outside the Level 1 scope.
///
/// Task requests require a second check after host-side resolution. Hidden
/// producer entries are left to the existing stricter internal-entry refusal.
pub(crate) fn unsupported(cmd: &Command) -> Option<&'static str> {
    match cmd {
        Command::Version
        | Command::Tasks(_)
        | Command::Task(_)
        | Command::Workspace(_)
        | Command::Doctor(_)
        | Command::Help(_)
        | Command::HelpGroup(_)
        | Command::InternalScriptRun(_)
        | Command::InternalGateway(_)
        | Command::InternalGatewayIdentity(_)
        | Command::InternalGuestProbe(_)
        | Command::InternalContainerLeaseReaper(_) => None,
        Command::Container(args) => match args.subcommand {
            ContainerSubcommand::Down { .. }
            | ContainerSubcommand::Status { .. }
            | ContainerSubcommand::Logs { .. }
            | ContainerSubcommand::Restart { .. } => None,
            _ => Some(CONTAINER),
        },
        Command::Runs(args) => match args.action {
            RunsAction::List { .. } | RunsAction::Status { .. } | RunsAction::Stop { .. } => None,
            RunsAction::Logs { .. } => Some(RUN_LOGS),
        },
        Command::Gateway(_) => Some(GATEWAY),
        Command::Release(_) => Some(RELEASE),
        Command::Bootstrap(_) | Command::Uninstall(_) => Some(INSTALL),
        _ => Some(OTHER),
    }
}

/// Trusted classification of a host-resolved task, never a guest assertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResolvedTaskPolicy {
    LevelOne,
    SharedLock,
    Pause,
    Unknown,
}

/// Refuse unsupported or indeterminate host-resolved task policy before admission.
pub(crate) fn unsupported_resolved_task(policy: ResolvedTaskPolicy) -> Option<&'static str> {
    match policy {
        ResolvedTaskPolicy::LevelOne => None,
        ResolvedTaskPolicy::SharedLock => Some(SHARED_LOCK),
        ResolvedTaskPolicy::Pause => Some(PAUSE),
        ResolvedTaskPolicy::Unknown => Some(UNKNOWN_TASK),
    }
}
