//! The one command-class refusal a container-mode guest keeps.
//!
//! A guest runs everything it can locally (Tom, 2026-11-19). The exception is a
//! hidden producer entry point: those belong to the host-side producer, and no
//! guest runs one.

use effigy_cli::Command;

const INTERNAL: &str = "hidden internal entry points are refused in a container-mode guest";

/// A hidden producer entry point, which no guest runs.
pub(crate) fn hidden_internal_entry(cmd: &Command) -> Option<&'static str> {
    match cmd {
        Command::InternalScriptRun(_)
        | Command::InternalGateway(_)
        | Command::InternalGatewayIdentity(_)
        | Command::InternalGuestProbe(_)
        | Command::InternalContainerLeaseReaper(_) => Some(INTERNAL),
        _ => None,
    }
}
