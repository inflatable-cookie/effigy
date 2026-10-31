//! Explicit budgets and class capabilities; construction performs no I/O.
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectClass {
    Static,
    Observe,
    RefreshDerived,
    Prepare,
    Mutate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopCause {
    Signal(i32),
    Requested,
    OwnerLost,
    /// The session's own listener ended: a custody failure, never an owner's
    /// request. Additive so a settled session names what actually happened.
    ListenerFailed,
}

/// First cancellation wins; clones share cancellation without global state.
#[derive(Debug, Default)]
struct Cancellation {
    cause: OnceLock<StopCause>,
    source: Option<fn() -> Option<StopCause>>,
}
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<Cancellation>);
impl CancelToken {
    pub fn cancel(&self, cause: StopCause) {
        let _ = self.0.cause.set(cause);
    }
    pub fn cause(&self) -> Option<&StopCause> {
        if let Some(source) = self.0.source {
            if let Some(cause) = source() {
                let _ = self.0.cause.set(cause);
            }
        }
        self.0.cause.get()
    }
    /// Only the main-installed router supplies this source. Libraries merely
    /// poll the token; they need no handler or router poll loop to see a signal.
    pub(crate) fn from_signal_source(source: fn() -> Option<StopCause>) -> Self {
        Self(Arc::new(Cancellation {
            cause: OnceLock::new(),
            source: Some(source),
        }))
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

/// A cancellation a person can act on: which signal or request ended the work.
impl std::fmt::Display for StopCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(signal) => write!(f, "{}", crate::human::signal_name(*signal)),
            Self::Requested => f.write_str("a stop request"),
            Self::OwnerLost => f.write_str("the owning process exiting"),
            Self::ListenerFailed => f.write_str("the session listener failing"),
        }
    }
}

/// What ended the work, in words: the deadline that elapsed, or the stop.
impl std::fmt::Display for Interrupt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut { phase, elapsed } => write!(
                f,
                "timed out after {} {}",
                crate::human::human_duration(*elapsed),
                phase_phrase(phase)
            ),
            Self::Cancelled { cause } => write!(f, "cancelled by {cause}"),
        }
    }
}

/// The phase a caller named is the operation in progress, so it reads as part
/// of the sentence rather than as a bare internal label.
fn phase_phrase(phase: &str) -> String {
    match phase {
        "settle" => "while waiting for the environment to settle".to_owned(),
        "scope inventory" => "while reading the checkout's scope inventory".to_owned(),
        "" => "while waiting for the operation's deadline".to_owned(),
        other => format!("during {other}"),
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn timed_out_names_the_wait_and_the_phase() {
        let interrupt = Interrupt::TimedOut {
            phase: "settle".into(),
            elapsed: Duration::from_millis(247_800),
        };
        assert_eq!(
            interrupt.to_string(),
            "timed out after 4m08s while waiting for the environment to settle"
        );
        let other = Interrupt::TimedOut {
            phase: "store transaction".into(),
            elapsed: Duration::from_secs(3),
        };
        assert_eq!(
            other.to_string(),
            "timed out after 3s during store transaction"
        );
    }

    #[test]
    fn cancelled_names_its_cause() {
        assert_eq!(
            Interrupt::Cancelled {
                cause: StopCause::Signal(libc::SIGINT)
            }
            .to_string(),
            "cancelled by SIGINT"
        );
        assert_eq!(
            Interrupt::Cancelled {
                cause: StopCause::OwnerLost
            }
            .to_string(),
            "cancelled by the owning process exiting"
        );
    }
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
    /// A fresh finite wait budget that keeps this operation's cancellation,
    /// limits, class and placement. Unlike `child` it may be *longer* than
    /// the parent's remaining time: the store's own writer window must not
    /// shrink to a workload deadline (§5.5, K4).
    pub(crate) fn patience(&self, window: Duration) -> Self {
        let now = Instant::now();
        let deadline = now.checked_add(window).unwrap_or(now);
        Self {
            deadline: Deadline::At(deadline),
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
        Ok(ObserveContext {
            operation: self.clone(),
            probes: None,
        })
    }
    pub fn effects(&self) -> Result<EffectContext, ClassError> {
        match self.class {
            EffectClass::Static | EffectClass::Observe => Err(ClassError),
            _ => Ok(EffectContext(self.clone())),
        }
    }
}
/// A wait or child deadline does not fit the operation's budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetOverflow;
/// The operation's class does not permit the effect it was asked to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassError;
impl std::fmt::Display for BudgetOverflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "the requested wait does not fit inside the operation's remaining budget; shorten the wait or start a longer-budgeted operation",
        )
    }
}
impl std::error::Error for BudgetOverflow {}
impl std::fmt::Display for ClassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "this operation is read-only, so it cannot run the effect it was asked for; rerun it as a mutating command",
        )
    }
}
impl std::error::Error for ClassError {}

/// Read-only capability. No conversion to an effect capability exists.
/// ```compile_fail
/// # use effigy_kernel::ctx::ObserveContext;
/// fn write(cx: &ObserveContext) { cx.effects(); }
/// ```
/// ```compile_fail
/// # use effigy_kernel::{ctx::ObserveContext, gate::{Gate, Wait}, root::StateRoot};
/// fn acquire(cx: &ObserveContext, root: &StateRoot) {
///     Gate::acquire(cx, root, &[], Wait::FailFast);
/// }
/// ```
/// ```compile_fail
/// # use effigy_kernel::{ctx::ObserveContext, custody::{Custody, ProcessSpec}, run::Run};
/// fn spawn(cx: &ObserveContext, run: &mut Run<'_>, spec: ProcessSpec) {
///     Custody::spawn(run, spec, cx);
/// }
/// ```
/// ```compile_fail
/// # use effigy_kernel::{ctx::ObserveContext, store::Store};
/// fn txn(cx: &ObserveContext, store: &mut Store) {
///     store.txn(cx, |_| Ok(()));
/// }
/// ```
/// ```compile_fail
/// # use effigy_kernel::{ctx::ObserveContext, probe::maintain};
/// fn maintenance(cx: &ObserveContext) { maintain(cx); }
/// ```
#[derive(Debug, Clone)]
pub struct ObserveContext {
    pub(crate) operation: OperationContext,
    pub(crate) probes: Option<std::sync::Arc<crate::probe::ProbeRuntime>>,
}
impl ObserveContext {
    pub fn check(&self, phase: &str) -> Result<(), Interrupt> {
        self.operation.check(phase)
    }
    pub fn remaining(&self) -> Duration {
        self.operation.remaining().unwrap_or(Duration::ZERO)
    }
    pub fn limits(&self) -> &Limits {
        &self.operation.limits
    }
    pub fn placement(&self) -> Placement {
        self.operation.placement
    }
}
#[derive(Debug, Clone)]
pub struct CleanupContext(OperationContext);
impl CleanupContext {
    pub fn operation(&self) -> &OperationContext {
        &self.0
    }
    /// The same cancellation, limits and capability with a window that starts
    /// now. A cleanup step takes its own window: a context minted when a
    /// long-lived owner started says nothing about how long the step may take.
    pub(crate) fn window(&self, window: Duration) -> Self {
        Self(self.0.patience(window))
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
    /// A finite phase budget, retaining this capability's class, placement and
    /// cancellation. Open workload lifetime never implies an unbounded DB wait.
    pub fn child(&self, budget: Duration) -> Self {
        Self(self.0.child(budget))
    }

    pub fn operation(&self) -> &OperationContext {
        &self.0
    }
    /// A finite wait budget of its own that keeps this context's cancellation.
    pub(crate) fn patience(&self, window: Duration) -> Self {
        Self(self.0.patience(window))
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
