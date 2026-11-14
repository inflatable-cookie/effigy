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
//!
//! Owned tests drive phase progression through [now], an injectable monotonic
//! clock (an offset on the real `Instant`, never a wall-clock conversion).
//! Production always reads the real monotonic clock.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// The opt-in: milliseconds from this process's start.
pub const RUN_DEADLINE_ENV: &str = "EFFIGY_RUN_DEADLINE_MS";

/// What settlement may use out of the ceiling: TERM-to-KILL escalation, the
/// foreground guard's closure, effect cleanup and the terminal evidence.
/// Charge it against a phase's remaining time, never in addition to it.
pub const SETTLEMENT_ALLOWANCE: Duration = Duration::from_secs(10);

/// The phases whose remaining budget is read back into the run's evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Admission,
    Workload,
    Settlement,
    Terminal,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Workload => "workload",
            Self::Settlement => "settlement",
            Self::Terminal => "terminal",
        }
    }
}

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
    pub fn ceiling_at(&self) -> Instant {
        self.ceiling
    }

    /// The ceiling as configured, in milliseconds.
    pub fn configured_ms(&self) -> u64 {
        self.configured_ms
    }

    /// How long the whole invocation has left.
    pub fn remaining(&self) -> Duration {
        self.ceiling.saturating_duration_since(now())
    }

    /// The absolute instant the workload must end by: the ceiling less the
    /// settlement allowance. Never after [Self::ceiling_at].
    pub fn workload_ceiling(&self) -> Instant {
        self.ceiling
            .checked_sub(SETTLEMENT_ALLOWANCE)
            .unwrap_or(self.origin)
    }

    /// How long the workload has left before its own settlement reserve.
    pub fn workload_remaining(&self) -> Duration {
        self.workload_ceiling().saturating_duration_since(now())
    }

    /// The phase's own remaining budget: admission and the workload share the
    /// workload boundary; settlement and the terminal evidence share the
    /// absolute ceiling.
    pub fn phase_remaining(&self, phase: Phase) -> Duration {
        let at = match phase {
            Phase::Admission | Phase::Workload => self.workload_ceiling(),
            Phase::Settlement | Phase::Terminal => self.ceiling,
        };
        at.saturating_duration_since(now())
    }

    /// Milliseconds elapsed since the origin was sampled. This is the
    /// observable origin: the phase readback below is stated on the same
    /// monotonic axis, so the origin is the zero point a reader can verify.
    pub fn since_origin(&self) -> Duration {
        now().saturating_duration_since(self.origin)
    }

    /// The run's evidence: the clock, its observable origin, the ceiling, the
    /// phase budgets and each phase's observed remaining time.
    pub fn evidence(&self) -> serde_json::Value {
        let workload_budget = self
            .configured_ms
            .saturating_sub(SETTLEMENT_ALLOWANCE.as_millis() as u64);
        serde_json::json!({
            "clock": "monotonic",
            "origin": "process_start",
            "origin_sample": "before capture, staging or admission",
            "origin_readback": "elapsed_since_origin_ms",
            "since_origin_ms": self.since_origin().as_millis() as u64,
            "ceiling_ms": self.configured_ms,
            "settlement_allowance_ms": SETTLEMENT_ALLOWANCE.as_millis() as u64,
            "workload_budget_ms": workload_budget,
            "phase_budgets_ms": {
                "admission": workload_budget,
                "workload": workload_budget,
                "settlement": SETTLEMENT_ALLOWANCE.as_millis() as u64,
                "terminal": SETTLEMENT_ALLOWANCE.as_millis() as u64,
            },
            "phases": phase_evidence(),
        })
    }

    /// The one line a text run writes so its deadline is visible in the
    /// evidence an operator keeps.
    pub fn describe(&self) -> String {
        format!(
            "run deadline {}ms: clock monotonic, origin process start (sampled before capture, \
             staging or admission), {}ms since origin, settlement allowance {}ms, workload budget \
             {}ms, {}ms remaining at admission\n",
            self.configured_ms,
            self.since_origin().as_millis(),
            SETTLEMENT_ALLOWANCE.as_millis(),
            self.configured_ms
                .saturating_sub(SETTLEMENT_ALLOWANCE.as_millis() as u64),
            self.phase_remaining(Phase::Admission).as_millis(),
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
/// A deterministic clock injection for owned tests: milliseconds added to the
/// real monotonic clock. Zero in production, so [now] is the real clock.
static CLOCK_OFFSET_MS: AtomicU64 = AtomicU64::new(0);
static INJECTED: OnceLock<Mutex<Option<RunDeadline>>> = OnceLock::new();
/// A settlement that did not finish positively under the run deadline. Process
/// wide because the footer decision is process wide.
static SETTLEMENT_INCOMPLETE: AtomicBool = AtomicBool::new(false);
static PHASES: OnceLock<Mutex<Vec<PhaseReadback>>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
struct PhaseReadback {
    phase: Phase,
    since_origin_ms: u64,
    remaining_ms: u64,
    budget_ms: u64,
}

/// The monotonic clock every deadline consumer uses. Production is
/// `Instant::now`; an owned test can advance it without sleeping.
pub fn now() -> Instant {
    let offset = CLOCK_OFFSET_MS.load(Ordering::SeqCst);
    if offset == 0 {
        return Instant::now();
    }
    Instant::now()
        .checked_add(Duration::from_millis(offset))
        .unwrap_or_else(Instant::now)
}

/// Records one phase's observed remaining budget in the run's evidence. The
/// latest observation of a phase wins, so the readback names where each phase
/// actually ended rather than only where it began.
pub fn note_phase(phase: Phase) {
    let Some(deadline) = current() else {
        return;
    };
    let observation = PhaseReadback {
        phase,
        since_origin_ms: deadline.since_origin().as_millis() as u64,
        remaining_ms: deadline.phase_remaining(phase).as_millis() as u64,
        budget_ms: match phase {
            Phase::Admission | Phase::Workload => deadline
                .configured_ms
                .saturating_sub(SETTLEMENT_ALLOWANCE.as_millis() as u64),
            Phase::Settlement | Phase::Terminal => SETTLEMENT_ALLOWANCE.as_millis() as u64,
        },
    };
    let mut phases = phases().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = phases.iter_mut().find(|existing| existing.phase == phase) {
        *existing = observation;
    } else {
        phases.push(observation);
    }
}

/// Clears the phase readback at the start of a run.
pub fn clear_phases() {
    phases()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

fn phases() -> &'static Mutex<Vec<PhaseReadback>> {
    PHASES.get_or_init(|| Mutex::new(Vec::new()))
}

fn phase_evidence() -> serde_json::Value {
    let phases = phases().lock().unwrap_or_else(PoisonError::into_inner);
    let mut object = serde_json::Map::new();
    for readback in phases.iter() {
        object.insert(
            readback.phase.name().to_owned(),
            serde_json::json!({
                "since_origin_ms": readback.since_origin_ms,
                "remaining_ms": readback.remaining_ms,
                "budget_ms": readback.budget_ms,
            }),
        );
    }
    serde_json::Value::Object(object)
}

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
    // The true process-start sample: a test clock is never installed here.
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
    let injected = INJECTED.get_or_init(|| Mutex::new(None));
    if let Some(explicit) = *injected.lock().unwrap_or_else(PoisonError::into_inner) {
        return Some(explicit);
    }
    CONFIGURED.get().copied().flatten()
}

/// The whole invocation's remaining time, if the feature is on.
pub fn remaining() -> Option<Duration> {
    current().map(|deadline| deadline.remaining())
}

/// The absolute instant the invocation must end by, if the feature is on.
pub fn ceiling() -> Option<Instant> {
    current().map(|deadline| deadline.ceiling_at())
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

/// Records that settlement did not finish positively under the run deadline.
pub fn note_settlement_incomplete() {
    SETTLEMENT_INCOMPLETE.store(true, Ordering::SeqCst);
}

/// Whether an earlier settlement in this process was incomplete, clearing the
/// marker so one run reports it once.
pub fn take_settlement_incomplete() -> bool {
    SETTLEMENT_INCOMPLETE.swap(false, Ordering::SeqCst)
}

/// Installs a deadline for owned tests. Test-only.
#[doc(hidden)]
pub fn inject_for_tests(deadline: Option<RunDeadline>) {
    let injected = INJECTED.get_or_init(|| Mutex::new(None));
    *injected.lock().unwrap_or_else(PoisonError::into_inner) = deadline;
}

/// Clears a deadline installed by [inject_for_tests]. Test-only.
#[doc(hidden)]
pub fn clear_injected_for_tests() {
    inject_for_tests(None);
}

/// Advances the deterministic clock owned tests share. Test-only.
#[doc(hidden)]
pub fn advance_clock_for_tests(by: Duration) {
    let ms = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
    CLOCK_OFFSET_MS.fetch_add(ms, Ordering::SeqCst);
}

/// Resets the deterministic clock. Test-only.
#[doc(hidden)]
pub fn clear_clock_for_tests() {
    CLOCK_OFFSET_MS.store(0, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn injected(ms: u64) -> RunDeadline {
        let deadline = RunDeadline::from_origin(Instant::now(), ms).expect("deadline");
        inject_for_tests(Some(deadline));
        clear_clock_for_tests();
        clear_phases();
        deadline
    }

    fn clear() {
        clear_injected_for_tests();
        clear_clock_for_tests();
        clear_phases();
    }

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
        assert_eq!(deadline.ceiling_at(), origin + Duration::from_secs(30));
        assert_eq!(
            deadline.workload_ceiling(),
            origin + Duration::from_secs(30) - SETTLEMENT_ALLOWANCE
        );
        assert!(deadline.workload_ceiling() < deadline.ceiling_at());
        assert_eq!(deadline.configured_ms(), 30_000);
    }

    #[test]
    fn the_clock_advances_every_phase_without_sleeping() {
        let deadline = injected(30_000);
        let admission = deadline.phase_remaining(Phase::Admission);
        assert!(admission > Duration::from_secs(19));
        advance_clock_for_tests(Duration::from_secs(15));
        assert!(deadline.phase_remaining(Phase::Admission) < Duration::from_secs(5));
        assert!(deadline.phase_remaining(Phase::Settlement) > Duration::from_secs(14));
        advance_clock_for_tests(Duration::from_secs(6));
        assert_eq!(deadline.phase_remaining(Phase::Admission), Duration::ZERO);
        assert!(deadline.phase_remaining(Phase::Settlement) > Duration::from_secs(8));
        advance_clock_for_tests(Duration::from_secs(10));
        assert_eq!(deadline.phase_remaining(Phase::Settlement), Duration::ZERO);
        assert!(deadline.since_origin() > Duration::from_secs(30));
        clear();
    }

    #[test]
    fn the_evidence_records_the_observable_origin_and_each_phase() {
        let deadline = injected(30_000);
        advance_clock_for_tests(Duration::from_secs(2));
        note_phase(Phase::Admission);
        advance_clock_for_tests(Duration::from_secs(3));
        note_phase(Phase::Workload);
        advance_clock_for_tests(Duration::from_secs(10));
        note_phase(Phase::Settlement);
        note_phase(Phase::Terminal);
        let evidence = deadline.evidence();
        assert_eq!(evidence["clock"], "monotonic");
        assert_eq!(evidence["origin"], "process_start");
        assert_eq!(evidence["origin_readback"], "elapsed_since_origin_ms");
        assert_eq!(evidence["ceiling_ms"], 30_000);
        assert_eq!(evidence["phase_budgets_ms"]["admission"], 20_000);
        assert_eq!(evidence["phase_budgets_ms"]["settlement"], 10_000);
        assert_eq!(evidence["phases"]["admission"]["since_origin_ms"], 2_000);
        assert_eq!(evidence["phases"]["workload"]["since_origin_ms"], 5_000);
        assert_eq!(evidence["phases"]["settlement"]["since_origin_ms"], 15_000);
        let settlement_remaining = evidence["phases"]["settlement"]["remaining_ms"]
            .as_u64()
            .unwrap();
        assert!(
            (14_900..=15_000).contains(&settlement_remaining),
            "{settlement_remaining}"
        );
        assert!(deadline
            .describe()
            .contains("origin process start (sampled before capture"));
        clear();
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
    fn the_incomplete_marker_reports_once() {
        assert!(!take_settlement_incomplete());
        note_settlement_incomplete();
        assert!(take_settlement_incomplete());
        assert!(!take_settlement_incomplete());
    }
}
