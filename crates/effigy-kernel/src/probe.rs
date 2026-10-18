//! Probe API types only. Execution and guard custody land after process custody.
use crate::ctx::ObserveContext;
use crate::observe::Observed;
use serde::Serialize;

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
