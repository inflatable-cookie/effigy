//! Explicit budgets and class capabilities; construction performs no I/O.
use serde::Serialize;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EffectClass {
    Static,
    Observe,
    RefreshDerived,
    Prepare,
    Mutate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Placement {
    Host,
    Guest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenReason {
    DevServer,
    FollowLogs,
    Shell,
}
#[derive(Debug, Clone, Copy)]
pub enum Deadline {
    At(Instant),
    Open(OpenReason),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum StopCause {
    Signal(i32),
    Requested,
    OwnerLost,
}

/// First cancellation wins; clones share cancellation without global state.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<OnceLock<StopCause>>);
impl CancelToken {
    pub fn cancel(&self, cause: StopCause) {
        let _ = self.0.set(cause);
    }
    pub fn cause(&self) -> Option<&StopCause> {
        self.0.get()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub capture_tail: usize,
    pub log_rotate: u64,
    pub log_keep: u8,
    pub line_max: usize,
    pub queue_bytes: usize,
    pub probe_concurrency: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interrupt {
    TimedOut { phase: String, elapsed: Duration },
    Cancelled { cause: StopCause },
}

#[derive(Debug, Clone)]
pub struct OperationContext {
    deadline: Deadline,
    started: Instant,
    cancel: CancelToken,
    limits: Limits,
    class: EffectClass,
    placement: Placement,
}
impl OperationContext {
    pub fn new(
        deadline: Deadline,
        cancel: CancelToken,
        limits: Limits,
        class: EffectClass,
        placement: Placement,
    ) -> Self {
        Self {
            deadline,
            started: Instant::now(),
            cancel,
            limits,
            class,
            placement,
        }
    }
    /// A child can shorten a budget, never extend it, including after expiry.
    pub fn child(&self, budget: Duration) -> Self {
        let now = Instant::now();
        // Overflow means the requested budget exceeds Instant's range. The
        // parent's finite deadline still wins; open parents fail closed.
        let requested = now.checked_add(budget);
        let deadline = match (self.deadline, requested) {
            (Deadline::At(parent), Some(child)) => Deadline::At(parent.min(child)),
            (Deadline::At(parent), None) => Deadline::At(parent),
            (Deadline::Open(_), Some(child)) => Deadline::At(child),
            (Deadline::Open(_), None) => Deadline::At(now),
        };
        Self {
            deadline,
            started: now,
            ..self.clone()
        }
    }
    /// Cleanup has a fresh token and a finite budget independent of the parent.
    /// A caller's second signal cancels this token explicitly to escalate.
    pub fn cleanup(&self, budget: Duration) -> Result<CleanupContext, BudgetOverflow> {
        let deadline = Instant::now().checked_add(budget).ok_or(BudgetOverflow)?;
        let mut operation = self.clone();
        operation.deadline = Deadline::At(deadline);
        operation.started = Instant::now();
        operation.cancel = CancelToken::default();
        Ok(CleanupContext(operation))
    }
    pub fn remaining(&self) -> Option<Duration> {
        match self.deadline {
            Deadline::At(at) => Some(at.saturating_duration_since(Instant::now())),
            Deadline::Open(_) => None,
        }
    }
    pub fn check(&self, phase: &str) -> Result<(), Interrupt> {
        if let Some(cause) = self.cancel.cause() {
            return Err(Interrupt::Cancelled {
                cause: cause.clone(),
            });
        }
        if matches!(self.remaining(), Some(left) if left.is_zero()) {
            return Err(Interrupt::TimedOut {
                phase: phase.into(),
                elapsed: self.started.elapsed(),
            });
        }
        Ok(())
    }
    pub fn deadline(&self) -> Deadline {
        self.deadline
    }
    pub fn cancel_token(&self) -> &CancelToken {
        &self.cancel
    }
    pub fn limits(&self) -> &Limits {
        &self.limits
    }
    pub fn class(&self) -> EffectClass {
        self.class
    }
    pub fn placement(&self) -> Placement {
        self.placement
    }
    pub fn observe(&self) -> Result<ObserveContext, ClassError> {
        if self.class == EffectClass::Static {
            return Err(ClassError);
        }
        // Observation always has a finite phase deadline, even when the
        // workload's lifetime is open. The caller must first choose child().
        if matches!(self.deadline, Deadline::Open(_)) {
            return Err(ClassError);
        }
        Ok(ObserveContext(self.clone()))
    }
    pub fn effects(&self) -> Result<EffectContext, ClassError> {
        match self.class {
            EffectClass::Static | EffectClass::Observe => Err(ClassError),
            _ => Ok(EffectContext(self.clone())),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetOverflow;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassError;

/// Read-only capability. No conversion to an effect capability exists.
/// ```compile_fail
/// # use effigy_kernel::ctx::ObserveContext;
/// fn write(cx: &ObserveContext) { cx.effects(); }
/// ```
/// ```compile_fail
/// # use effigy_kernel::ctx::{ObserveContext, EffectContext};
/// fn acquire(_: &EffectContext) {}
/// fn observe(cx: &ObserveContext) { acquire(cx); }
/// ```
/// ```compile_fail
/// # use effigy_kernel::ctx::{ObserveContext, EffectContext};
/// fn spawn(_: &EffectContext) {}
/// fn observe(cx: &ObserveContext) { spawn(cx); }
/// ```
/// ```compile_fail
/// # use effigy_kernel::ctx::{ObserveContext, EffectContext};
/// fn txn(_: &EffectContext) {}
/// fn observe(cx: &ObserveContext) { txn(cx); }
/// ```
#[derive(Debug, Clone)]
pub struct ObserveContext(OperationContext);
impl ObserveContext {
    pub fn check(&self, phase: &str) -> Result<(), Interrupt> {
        self.0.check(phase)
    }
    pub fn remaining(&self) -> Duration {
        self.0.remaining().unwrap_or(Duration::ZERO)
    }
    pub fn limits(&self) -> &Limits {
        &self.0.limits
    }
    pub fn placement(&self) -> Placement {
        self.0.placement
    }
}
#[derive(Debug, Clone)]
pub struct CleanupContext(OperationContext);
impl CleanupContext {
    pub fn operation(&self) -> &OperationContext {
        &self.0
    }
}
#[derive(Debug, Clone)]
pub struct EffectContext(OperationContext);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    DerivedStore,
    SourceStore,
    UserFile,
    AcquireSource,
    AdapterEffect,
    MintToken,
    HostAuthority,
}
impl EffectContext {
    pub fn operation(&self) -> &OperationContext {
        &self.0
    }
    /// Enforcement hook called before any later primitive performs an effect.
    pub fn require(&self, capability: Capability) -> Result<(), ClassError> {
        use Capability::*;
        use EffectClass::*;
        let allowed = match capability {
            DerivedStore => matches!(self.0.class, RefreshDerived | Mutate),
            SourceStore | UserFile => matches!(self.0.class, Prepare | Mutate),
            MintToken => {
                matches!(self.0.class, Prepare | Mutate) && self.0.placement == Placement::Host
            }
            AcquireSource => self.0.class == Prepare,
            AdapterEffect => self.0.class == Mutate,
            HostAuthority => self.0.placement == Placement::Host,
        };
        if allowed {
            Ok(())
        } else {
            Err(ClassError)
        }
    }
}
