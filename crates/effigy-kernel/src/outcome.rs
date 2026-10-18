//! One structured result for rendering. Reporting failures are warnings only.
use crate::ctx::StopCause;
use crate::id::{Generation, ResourceId};
use crate::observe::{Observed, UnknownReason};
use serde::Serialize;
use std::ffi::OsString;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum WorkloadOutcome {
    NotStarted,
    Succeeded,
    Failed {
        code: i32,
    },
    Cancelled {
        cause: StopCause,
        signal: Option<i32>,
    },
    TimedOut {
        phase: String,
    },
    Interrupted,
    Unknown(UnknownReason),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CleanupOutcome {
    NotNeeded,
    Complete,
    Incomplete,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    Busy,
    InvalidInput,
    Io,
    Incomplete,
    Interrupted,
    Unknown,
}
#[derive(Debug, Clone, Serialize)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    pub message: String,
    pub details: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum EffectDisposition {
    Applied,
    Released,
    Retained,
    Irreversible,
    Failed,
    Unknown,
}
#[derive(Debug, Clone, Serialize)]
pub struct EffectReport {
    pub resource: ResourceId,
    pub generation: Generation,
    pub disposition: EffectDisposition,
}
#[derive(Debug, Clone, Serialize)]
pub struct Residue {
    pub kind: String,
    pub id: ResourceId,
    pub observation: Observed<String>,
    pub next: Invocation,
}
#[derive(Debug, Clone, Serialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

/// Grammar belongs to the engine. Construction requires its parser to accept
/// the exact argument vector; shell strings and quoting are never reconstructed.
#[derive(Debug, Clone, Serialize)]
pub struct Invocation(Vec<OsString>);
impl Invocation {
    pub fn validated(
        argv: Vec<OsString>,
        parser: impl FnOnce(&[OsString]) -> bool,
    ) -> Result<Self, InvalidOutcome> {
        if argv.is_empty() || !parser(&argv) {
            return Err(InvalidOutcome);
        }
        Ok(Self(argv))
    }
    pub fn argv(&self) -> &[OsString] {
        &self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredObligation {
    ProcessExit(i32),
    Applied,
    NotCompleted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidOutcome;

#[derive(Debug, Clone, Serialize)]
pub struct Outcome<T> {
    result: Result<T, ErrorInfo>,
    workload: WorkloadOutcome,
    cleanup: CleanupOutcome,
    effects: Vec<EffectReport>,
    residue: Vec<Residue>,
    next: Vec<Invocation>,
    warnings: Vec<Warning>,
}
impl<T> Outcome<T> {
    pub fn new(
        result: Result<T, ErrorInfo>,
        workload: WorkloadOutcome,
        cleanup: CleanupOutcome,
        obligations: &[RequiredObligation],
    ) -> Result<Self, InvalidOutcome> {
        if workload == WorkloadOutcome::Succeeded
            && obligations.iter().any(|item| {
                !matches!(
                    item,
                    RequiredObligation::ProcessExit(0) | RequiredObligation::Applied
                )
            })
        {
            return Err(InvalidOutcome);
        }
        if matches!(workload, WorkloadOutcome::Cancelled { signal: Some(signal), .. } if !(1..=127).contains(&signal))
        {
            return Err(InvalidOutcome);
        }
        if matches!(workload, WorkloadOutcome::Failed { code: 0 }) {
            return Err(InvalidOutcome);
        }
        Ok(Self {
            result,
            workload,
            cleanup,
            effects: vec![],
            residue: vec![],
            next: vec![],
            warnings: vec![],
        })
    }
    pub fn workload(&self) -> &WorkloadOutcome {
        &self.workload
    }
    pub fn result(&self) -> &Result<T, ErrorInfo> {
        &self.result
    }
    pub fn cleanup(&self) -> CleanupOutcome {
        self.cleanup
    }
    pub fn add_warning(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }
    pub fn add_next(&mut self, invocation: Invocation) {
        self.next.push(invocation);
    }
    pub fn add_effect(&mut self, report: EffectReport) {
        self.effects.push(report);
    }
    pub fn add_residue(&mut self, residue: Residue) {
        self.cleanup = CleanupOutcome::Incomplete;
        self.residue.push(residue);
    }
    pub fn exit_code(&self) -> i32 {
        match &self.workload {
            WorkloadOutcome::Failed { code } => *code,
            WorkloadOutcome::Cancelled {
                signal: Some(signal),
                ..
            } => 128 + signal,
            WorkloadOutcome::TimedOut { .. } => 124,
            WorkloadOutcome::Cancelled { .. }
            | WorkloadOutcome::Interrupted
            | WorkloadOutcome::Unknown(_) => 1,
            _ => {
                if matches!(&self.result, Err(error) if error.code == ErrorCode::Busy) {
                    return 75;
                }
                if self.result.is_err() || self.cleanup == CleanupOutcome::Incomplete {
                    1
                } else {
                    0
                }
            }
        }
    }
}
impl<T: Serialize> Outcome<T> {
    /// Both renderers consume this same report; errors remain structured values.
    pub fn report(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }
    pub fn json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.report()?)
    }
    /// The foundation's lossless text rendering; engine presentation stays above
    /// the kernel. No stringified report is placed in ErrorInfo.message.
    pub fn text(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.report()?)
    }
}
