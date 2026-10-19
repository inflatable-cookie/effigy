//! Bounded, read-only observation. Probe custody never opens state or a journal.
mod guard;
mod io;
mod process;
#[cfg(test)]
mod tests;

use crate::ctx::ObserveContext;
use crate::observe::{Evidence, Observed, UnknownReason};
pub use guard::{internal_entry, maintain, MaintenanceReport, ProbeRuntime};
pub use io::{HttpReadMethod, ObservationTarget, ProbeIo, StatusSet};
pub use process::{ProbeCommand, ProbeOutput};
use serde::Serialize;
use std::time::Duration;

pub trait Probe {
    fn observe(&self, cx: &ObserveContext) -> Observed<Readiness>;
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Readiness {
    Created,
    ExecReady,
    TcpOpen,
    SocketOwned { pid: u32 },
    HttpReady { status: u16 },
    LogMatched,
    Healthy,
    EndpointApplied { revision: u64 },
}
/// One deadline and ordered results. A command's exit is not a readiness fact;
/// adapters must verify their required predicate and exact generation.
pub fn wait_all(probes: &[&dyn Probe], cx: &ObserveContext) -> Vec<Observed<Readiness>> {
    let mut results: Vec<_> = probes.iter().map(|p| p.observe(cx)).collect();
    while results.iter().any(|r| !matches!(r, Observed::Verified(..))) {
        if cx.check("readiness").is_err() {
            break;
        }
        std::thread::sleep(cx.remaining().min(Duration::from_millis(5)));
        for (probe, result) in probes.iter().zip(&mut results) {
            // Recheck even successful predicates: all must hold at the same
            // observation round, rather than accumulating stale successes.
            *result = probe.observe(cx);
        }
    }
    results
}
fn evidence() -> Evidence {
    Evidence::new("kernel read-only probe".into(), None).expect("fixed source")
}
fn failure(error: std::io::Error) -> UnknownReason {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => UnknownReason::PermissionDenied,
        std::io::ErrorKind::NotFound => UnknownReason::ToolUnavailable("probe executable".into()),
        std::io::ErrorKind::TimedOut => UnknownReason::TimedOut {
            phase: "probe".into(),
        },
        _ => UnknownReason::Inconsistent(error.to_string()),
    }
}
fn check(cx: &ObserveContext, phase: &str) -> Result<(), UnknownReason> {
    cx.check(phase).map_err(|interrupt| match interrupt {
        crate::ctx::Interrupt::TimedOut { phase, .. } => UnknownReason::TimedOut { phase },
        other => UnknownReason::Inconsistent(format!("{other:?}")),
    })
}
