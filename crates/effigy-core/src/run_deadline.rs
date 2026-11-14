//! One opt-in absolute deadline for an Effigy invocation (effigy#992).
//!
//! `EFFIGY_RUN_DEADLINE_MS` states, once, how long the whole invocation may
//! take: admission, the workload, and Effigy's own settlement. The clock is
//! `std::time::Instant` (monotonic). Its origin is sampled exactly once, at
//! process start, before any capture, staging or admission, and it is never
//! restarted at a workload spawn or per step. No wall-clock conversion is
//! involved: every consumer compares monotonic instants.
//!
//! A settlement allowance is reserved out of the ceiling for Effigy's own
//! teardown (escalation grace, guard closure, cleanup and terminal evidence).
//! The workload therefore ends by `ceiling - allowance`, and settlement must
//! finish by `ceiling`. A deadline that cannot fit the allowance is refused
//! before anything starts.
//!
//! The feature is off by default: [current] is `None`, and every consumer
//! keeps its previous budget byte for byte.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The opt-in: milliseconds from this process's start.
pub const RUN_DEADLINE_ENV: &str = "EFFIGY_RUN_DEADLINE_MS";

/// What settlement may use out of the ceiling: TERM-to-KILL escalation, the
/// foreground guard's closure, effect cleanup and the terminal evidence.
/// Charge it against a phase's remaining time, never in addition to it.
pub const SETTLEMENT_ALLOWANCE: Duration = Duration::from_secs(10);

/// A resolved invocation deadline. `origin` is the monotonic process-start
/// sample and `ceiling` the absolute instant the invocation must end by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunDeadline {
    origin: Instant,
    ceiling: Instant,
    configured_ms: u64,
}

impl RunDeadline {
    /// A deadline `ms` from `origin`. The caller supplies the origin so a test
    /// can inject a clock instead of sleeping.
    pub fn from_origin(origin: Instant, ms: u64) -> Option<Self> {
        let ceiling = origin.checked_add(Duration::from_millis(ms))?;
        Some(Self {
            origin,
            ceiling,
            configured_ms: ms,
        })
    }

    /// The monotonic instant this invocation started at.
    pub fn origin(&self) -> Instant {
        self.origin
    }

    /// The absolute instant the invocation must end by.
    pub fn ceiling(&self) -> Instant {
        self.ceiling
    }

    /// The ceiling as configured, in milliseconds.
    pub fn configured_ms(&self) -> u64 {
        self.configured_ms
    }

    /// How long the whole invocation has left.
    pub fn remaining(&self) -> Duration {
        self.ceiling.saturating_duration_since(Instant::now())
    }

    /// The absolute instant the workload must end by: the ceiling less the
    /// settlement allowance. Never after [Self::ceiling].
    pub fn workload_ceiling(&self) -> Instant {
        self.ceiling
            .checked_sub(SETTLEMENT_ALLOWANCE)
            .unwrap_or(self.origin)
    }

    /// How long the workload has left before its own settlement reserve.
    pub fn workload_remaining(&self) -> Duration {
        self.workload_ceiling()
            .saturating_duration_since(Instant::now())
    }

    /// The run's evidence: the clock, its origin, the ceiling and the two
    /// phase budgets. Numbers are monotonic milliseconds; nothing here is a
    /// wall-clock time.
    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "clock": "monotonic",
            "origin": "process_start",
            "ceiling_ms": self.configured_ms,
            "settlement_allowance_ms": SETTLEMENT_ALLOWANCE.as_millis() as u64,
            "workload_budget_ms": self
                .configured_ms
                .saturating_sub(SETTLEMENT_ALLOWANCE.as_millis() as u64),
        })
    }

    /// The one line a text run writes so its deadline is visible in the
    /// evidence an operator keeps.
    pub fn describe(&self) -> String {
        format!(
            "run deadline {}ms: clock monotonic, origin process start, settlement allowance {}ms, \
             workload budget {}ms, {}ms remaining at admission\n",
            self.configured_ms,
            SETTLEMENT_ALLOWANCE.as_millis(),
            self.configured_ms
                .saturating_sub(SETTLEMENT_ALLOWANCE.as_millis() as u64),
            self.remaining().as_millis(),
        )
    }

    /// The value a child process is given: the workload budget left now. A
    /// child starts its own clock, so it may never receive the parent's
    /// configured span (that would extend the deadline), and it reserves its
    /// own settlement allowance inside what it is given.
    pub fn child_env_ms(&self) -> u64 {
        let ms = self.workload_remaining().as_millis();
        u64::try_from(ms).unwrap_or(u64::MAX)
    }
}

static CONFIGURED: OnceLock<Option<RunDeadline>> = OnceLock::new();
static CONFIGURED_ENV: OnceLock<Result<(), String>> = OnceLock::new();
// A deterministic clock injection for owned tests. Thread-local so parallel
// tests in one binary never see each other's deadline.
thread_local! {
    static INJECTED: RefCell<Option<Option<RunDeadline>>> = const { RefCell::new(None) };
}
/// A settlement that could not finish inside the reserved allowance. Process
/// wide because the footer decision is process wide.
static SETTLEMENT_TRUNCATED: AtomicBool = AtomicBool::new(false);

/// Parses one `EFFIGY_RUN_DEADLINE_MS` value against an explicit origin.
/// `None` means the variable is absent or blank: the feature is off.
pub fn parse(origin: Instant, raw: Option<&str>) -> Result<Option<RunDeadline>, String> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    let ms = raw
        .parse::<u64>()
        .map_err(|_| format!("{RUN_DEADLINE_ENV} must be a positive integer millisecond count"))?;
    validate(ms)?;
    RunDeadline::from_origin(origin, ms)
        .map(Some)
        .ok_or_else(|| format!("{RUN_DEADLINE_ENV}={ms} is beyond this machine's monotonic clock"))
}

/// Refuses a deadline that leaves no workload budget. The allowance is
/// reserved, not optional, so the whole invocation must be longer than it.
pub fn validate(ms: u64) -> Result<(), String> {
    if ms == 0 {
        return Err(format!("{RUN_DEADLINE_ENV} must be greater than zero"));
    }
    let allowance = SETTLEMENT_ALLOWANCE.as_millis() as u64;
    if ms <= allowance {
        return Err(format!(
            "{RUN_DEADLINE_ENV}={ms} leaves no workload budget: Effigy reserves \
             {allowance}ms for settlement, so the deadline must be greater than {allowance}ms"
        ));
    }
    Ok(())
}

fn configure_env_once() -> Result<(), String> {
    let origin = Instant::now();
    let raw = match std::env::var(RUN_DEADLINE_ENV) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(format!("could not read {RUN_DEADLINE_ENV}: {error}")),
    };
    let deadline = parse(origin, raw.as_deref())?;
    let _ = CONFIGURED.set(deadline);
    Ok(())
}

/// Samples the process-start origin and reads the opt-in once. Call this
/// first in `main`, before capture, staging or admission; a second call is a
/// no-op that returns the same result.
pub fn configure_from_env() -> Result<(), String> {
    CONFIGURED_ENV.get_or_init(configure_env_once).clone()
}

/// The process's resolved deadline, if the feature is on.
pub fn current() -> Option<RunDeadline> {
    if let Some(explicit) = INJECTED.with(|injected| *injected.borrow()) {
        return explicit;
    }
    CONFIGURED.get().copied().flatten()
}

/// The whole invocation's remaining time, if the feature is on.
pub fn remaining() -> Option<Duration> {
    current().map(|deadline| deadline.remaining())
}

/// The absolute instant the invocation must end by, if the feature is on.
pub fn ceiling() -> Option<Instant> {
    current().map(|deadline| deadline.ceiling())
}

/// The workload's remaining time, after the reserved settlement allowance.
pub fn workload_remaining() -> Option<Duration> {
    current().map(|deadline| deadline.workload_remaining())
}

/// The absolute instant the workload must end by, if the feature is on.
pub fn workload_ceiling() -> Option<Instant> {
    current().map(|deadline| deadline.workload_ceiling())
}

/// The value a nested Effigy child is given, if the feature is on.
pub fn child_env_ms() -> Option<u64> {
    current().map(|deadline| deadline.child_env_ms())
}

/// The run's evidence, if the feature is on.
pub fn evidence() -> Option<serde_json::Value> {
    current().map(|deadline| deadline.evidence())
}

/// Records that settlement could not finish inside the reserved allowance.
pub fn note_settlement_truncated() {
    SETTLEMENT_TRUNCATED.store(true, Ordering::SeqCst);
}

/// Whether an earlier settlement in this process was cut short by the
/// deadline, clearing the marker so one run reports it once.
pub fn take_settlement_truncated() -> bool {
    SETTLEMENT_TRUNCATED.swap(false, Ordering::SeqCst)
}

/// Installs a deterministic clock for the current thread. Test-only.
#[doc(hidden)]
pub fn inject_for_tests(deadline: Option<RunDeadline>) {
    INJECTED.with(|injected| *injected.borrow_mut() = Some(deadline));
}

/// Removes a test clock from the current thread. Test-only.
#[doc(hidden)]
pub fn clear_injected_for_tests() {
    INJECTED.with(|injected| *injected.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_or_blank_is_off() {
        let origin = Instant::now();
        assert_eq!(parse(origin, None).unwrap(), None);
        assert_eq!(parse(origin, Some("  ")).unwrap(), None);
    }

    #[test]
    fn a_short_deadline_is_refused_before_anything_starts() {
        let origin = Instant::now();
        let allowance = SETTLEMENT_ALLOWANCE.as_millis() as u64;
        let refused = parse(origin, Some(&allowance.to_string())).expect_err("no workload budget");
        assert!(refused.contains("settlement"), "{refused}");
        assert!(parse(origin, Some("0")).is_err());
        assert!(parse(origin, Some("soon")).is_err());
        assert!(parse(origin, Some(&(allowance + 1).to_string())).is_ok());
    }

    #[test]
    fn phase_budgets_reserve_settlement_inside_the_one_ceiling() {
        let origin = Instant::now();
        let deadline = parse(origin, Some("30000")).unwrap().expect("on");
        assert_eq!(deadline.origin(), origin);
        assert_eq!(deadline.ceiling(), origin + Duration::from_secs(30));
        assert_eq!(
            deadline.workload_ceiling(),
            origin + Duration::from_secs(30) - SETTLEMENT_ALLOWANCE
        );
        assert!(deadline.workload_ceiling() < deadline.ceiling());
        assert_eq!(deadline.configured_ms(), 30_000);
    }

    #[test]
    fn a_child_is_given_only_the_remaining_workload_budget() {
        let origin = Instant::now() - Duration::from_secs(25);
        let deadline = parse(origin, Some("30000")).unwrap().expect("on");
        // 5s of the ceiling is left; the 10s allowance does not fit in it.
        assert_eq!(deadline.workload_remaining(), Duration::ZERO);
        assert_eq!(deadline.child_env_ms(), 0);
        let fresh = parse(Instant::now(), Some("30000")).unwrap().expect("on");
        let child = fresh.child_env_ms();
        assert!(child <= 20_000, "{child}");
        assert!(child >= 19_900, "{child}");
    }

    #[test]
    fn evidence_names_the_clock_origin_and_phase_budgets() {
        let deadline = parse(Instant::now(), Some("30000")).unwrap().expect("on");
        let evidence = deadline.evidence();
        assert_eq!(evidence["clock"], "monotonic");
        assert_eq!(evidence["origin"], "process_start");
        assert_eq!(evidence["ceiling_ms"], 30_000);
        assert_eq!(evidence["settlement_allowance_ms"], 10_000);
        assert_eq!(evidence["workload_budget_ms"], 20_000);
        assert!(deadline.describe().contains("origin process start"));
    }

    #[test]
    fn an_injected_clock_reaches_every_reader_and_clears() {
        let deadline = parse(Instant::now(), Some("30000")).unwrap().expect("on");
        inject_for_tests(Some(deadline));
        assert_eq!(current(), Some(deadline));
        assert_eq!(ceiling(), Some(deadline.ceiling()));
        assert!(workload_remaining().is_some());
        clear_injected_for_tests();
        assert_eq!(current(), CONFIGURED.get().copied().flatten());
    }

    #[test]
    fn the_truncation_marker_reports_once() {
        clear_injected_for_tests();
        assert!(!take_settlement_truncated());
        note_settlement_truncated();
        assert!(take_settlement_truncated());
        assert!(!take_settlement_truncated());
    }
}
