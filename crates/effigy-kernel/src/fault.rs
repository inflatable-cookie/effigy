//! Test-only fault points. Thread-local scope isolates strict fakes.
//!
//! A scenario first runs traced to list the points it crosses; each crossing
//! index can then be armed with one fault. Child processes arm through
//! `EFFIGY_KERNEL_TEST_FAULT` (see `arm_from_env`), read only here.
use crate::observe::{Observed, UnknownReason};
use std::cell::{Cell, RefCell};
use std::io;
use std::panic::Location;

macro_rules! points {
    ($($name:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) enum Point {
            $($name),*
        }
        impl Point {
            /// Every point, so a child process can be armed by name.
            pub(crate) const ALL: &'static [Point] = &[$(Point::$name),*];
        }
    };
}
points! {
    ObjectStageBefore,
    ObjectStageCreated,
    ObjectWriteBefore,
    ObjectWriteAfter,
    ObjectSealBefore,
    ObjectSealAfter,
    ObjectRenameBefore,
    ObjectRenameAfter,
    ObjectPublishBefore,
    ObjectPublishAfter,
    ObjectRefBefore,
    ObjectRefAfter,
    ObjectCleanupBefore,
    ObjectCleanupAfter,
    RemoteBeforeIdentity,
    RemoteAfterIntent,
    RemoteAfterRecord,
    RemoteBeforeAck,
    RemoteAfterAckBeforeRecord,
    ProbeFork,
    ProbeRegister,
    ProbeGuardAck,
    ProbeBeforeGo,
    ProbeAfterGo,
    ProbeCloseBefore,
    ProbeConnect,
    ProbeObservation,
    SocketBind,
    CaptureWrite,
    CaptureSync,
    CaptureCommitAfterPublish,
    CapturePendingRemoval,
    CapturePendingSync,
    CaptureSetupAfterStdout,
    CaptureSetupBeforeThread,
    LogRead,
    AnchorSpawn,
    AnchorRecord,
    AnchorStop,
    AnchorStopped,
    SpawnFork,
    SpawnBeforeRecord,
    SpawnBeforeGo,
    SpawnAfterGo,
    LifecycleBefore,
    LifecycleCommit,
    LifecycleAfter,
    SentinelBeforeSettle,
    SentinelAfterSettle,
    SentinelClose,
    GateAcquire,
    BeginBefore,
    BeginGated,
    BeginRecovered,
    BeginAdmitted,
    AdmissionReserveBefore,
    AdmissionReserveAfter,
    ReservationBefore,
    ReservationAfter,
    ActivationDecisionBefore,
    ActivationDecisionAfter,
    SessionStageBefore,
    SessionStageAfter,
    SelectionCommitBefore,
    SelectionCommitAfter,
    SelectionSwitchBefore,
    SelectionSwitchAfter,
    SelectionAbandonBefore,
    SelectionAbandonAfter,
    ActivationCommitBefore,
    ActivationMappingBefore,
    ActivationAttachmentBefore,
    ActivationCommitAfter,
    RunAttachBefore,
    RunAttachAfter,
    RunAttachReleaseBefore,
    RunAttachReleaseAfter,
    IntendBefore,
    IntendAfter,
    AppliedBefore,
    AppliedAfter,
    FailedBefore,
    FailedAfter,
    PhaseBefore,
    PhaseAfter,
    RequestBefore,
    RequestAfter,
    SettleBefore,
    SettleStopping,
    SettleEffectBefore,
    SettleEffectAfter,
    SettleCloseBefore,
    SettleCloseAfter,
    AdapterBefore,
    AdapterAfter,
    AttachBefore,
    AttachAfter,
    AttachmentReleaseBefore,
    AttachmentReleaseAfter,
    ClaimReserveBefore,
    ClaimReserveAfter,
    ClaimPublishBefore,
    ClaimPublishAfter,
    ClaimRetainBefore,
    ClaimRetainAfter,
    StoreCommitBefore,
    StoreCommitAfter,
    ReplaceOpen,
    ReplaceWrite,
    ReplaceFsync,
    ReplaceLinkPrev,
    ReplaceRename,
    ReplaceDirFsync,
    SourceDirOpen,
    SourceDirSync,
    EngineRecordBefore,
    EngineRecordAfter,
    EnvStateBefore,
    EnvStateAfter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Fault {
    /// SIGKILL self.
    Kill,
    Enospc,
    Eperm,
    /// The next observation on this thread returns `Unknown(Injected)`.
    Unknown,
    /// SIGSTOP self; the harness inspects the stalled holder, then continues it.
    Stall,
    /// The injected wall clock jumps forward by `CLOCK_JUMP_MS` from here on.
    ClockJump,
}
impl Fault {
    pub(crate) const ALL: [Fault; 6] = [
        Fault::Kill,
        Fault::Enospc,
        Fault::Eperm,
        Fault::Unknown,
        Fault::Stall,
        Fault::ClockJump,
    ];
    fn name(self) -> &'static str {
        match self {
            Fault::Kill => "kill",
            Fault::Enospc => "enospc",
            Fault::Eperm => "eperm",
            Fault::Unknown => "unknown",
            Fault::Stall => "stall",
            Fault::ClockJump => "clock",
        }
    }
    fn parse(name: &str) -> Self {
        *Self::ALL
            .iter()
            .find(|fault| fault.name() == name)
            .unwrap_or_else(|| panic!("unknown fault {name}"))
    }
}
/// Past every kernel retention window, so a wall-clock reader that grants
/// anything from elapsed time is caught.
pub(crate) const CLOCK_JUMP_MS: i64 = 40 * 24 * 60 * 60 * 1000;
/// One ordered record for oracle O8: what was observed, and what was destroyed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    Observed { subject: String, unknown: bool },
    Destructive { subject: String },
}
type Hook = Box<dyn Fn(Point)>;
thread_local! {
    static HOOK: RefCell<Option<Hook>> = const {RefCell::new(None)};
    static UNKNOWN: Cell<bool> = const { Cell::new(false) };
    static NEXT_UNKNOWN: Cell<bool> = const { Cell::new(false) };
    static NAMED: Cell<Option<(Point,Fault)>> = const {Cell::new(None)};
    static NTH: Cell<Option<(Point, usize, Fault)>> = const {Cell::new(None)};
    static PER_POINT: RefCell<Vec<(Point, usize)>> = const { RefCell::new(Vec::new()) };
    static ARMED: Cell<Option<(usize, Fault)>> = const { Cell::new(None) };
    static FIRED: Cell<bool> = const { Cell::new(false) };
    static CROSSED: Cell<usize> = const { Cell::new(0) };
    static TRACE: RefCell<Vec<Point>> = const { RefCell::new(Vec::new()) };
    static OBSERVATIONS: RefCell<Vec<&'static Location<'static>>> = const { RefCell::new(Vec::new()) };
    static OBSERVATION_ARMED: Cell<Option<usize>> = const { Cell::new(None) };
    static SKEW: Cell<i64> = const { Cell::new(0) };
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static SUSPENDED: Cell<bool> = const { Cell::new(false) };
    static HELPERS: RefCell<Option<std::path::PathBuf>> = const { RefCell::new(None) };
    static OPERATOR_UID: Cell<Option<u32>> = const { Cell::new(None) };
    static NETWORK_FS: Cell<bool> = const { Cell::new(false) };
}
/// Every observation site calls this first. It counts the crossing so the
/// driver can inject `Unknown` into each observation in turn.
#[track_caller]
pub(crate) fn injected<T>() -> Option<Observed<T>> {
    if SUSPENDED.with(Cell::get) {
        return None;
    }
    let index = OBSERVATIONS.with(|seen| {
        let mut seen = seen.borrow_mut();
        seen.push(Location::caller());
        seen.len() - 1
    });
    let armed = OBSERVATION_ARMED.with(Cell::get) == Some(index);
    if armed {
        FIRED.with(|fired| fired.set(true));
    }
    let next = NEXT_UNKNOWN.with(|flag| flag.replace(false));
    (armed || next || UNKNOWN.with(Cell::get)).then_some(Observed::Unknown(UnknownReason::Injected))
}
pub(crate) fn point(point: Point) -> io::Result<()> {
    TRACE.with(|trace| trace.borrow_mut().push(point));
    HOOK.with(|hook| {
        if let Some(callback) = hook.borrow().as_ref() {
            callback(point);
        }
    });
    let index = CROSSED.with(|crossed| {
        let index = crossed.get();
        crossed.set(index + 1);
        index
    });
    let nth = PER_POINT.with(|counts| {
        let mut counts = counts.borrow_mut();
        match counts.iter_mut().find(|(seen, _)| *seen == point) {
            Some((_, count)) => {
                *count += 1;
                *count - 1
            }
            None => {
                counts.push((point, 1));
                0
            }
        }
    });
    let named = NAMED.with(|cell| match cell.get() {
        Some((armed, fault)) if armed == point => {
            cell.set(None);
            Some((index, fault))
        }
        _ => None,
    });
    let by_nth = NTH.with(|cell| match cell.get() {
        Some((armed, n, fault)) if armed == point && n == nth => {
            cell.set(None);
            Some((index, fault))
        }
        _ => None,
    });
    match named.or(by_nth).or_else(|| ARMED.with(Cell::get)) {
        Some((armed, fault)) if armed == index => {
            FIRED.with(|fired| fired.set(true));
            match fault {
                Fault::Kill => loop {
                    // SAFETY: signalling our own pid; SIGKILL does not return.
                    unsafe { libc::kill(libc::getpid(), libc::SIGKILL) };
                },
                Fault::Enospc => Err(io::Error::from_raw_os_error(libc::ENOSPC)),
                Fault::Eperm => Err(io::Error::from_raw_os_error(libc::EPERM)),
                Fault::Unknown => {
                    NEXT_UNKNOWN.with(|flag| flag.set(true));
                    Ok(())
                }
                Fault::Stall => {
                    // SAFETY: signalling our own pid; execution resumes on SIGCONT.
                    unsafe { libc::kill(libc::getpid(), libc::SIGSTOP) };
                    Ok(())
                }
                Fault::ClockJump => {
                    SKEW.with(|skew| skew.set(CLOCK_JUMP_MS));
                    Ok(())
                }
            }
        }
        _ => Ok(()),
    }
}
/// Resets the crossing counter and trace; `Some` arms one crossing.
pub(crate) fn hook(callback: Option<Hook>) {
    HOOK.with(|hook| *hook.borrow_mut() = callback);
}
pub(crate) fn arm_point(point: Point, fault: Fault) {
    arm(None);
    NAMED.with(|cell| cell.set(Some((point, fault))));
}
/// Arms the `nth` crossing of `point`, so unrelated retries elsewhere in a
/// scenario cannot shift which boundary receives the fault.
pub(crate) fn arm_nth(point: Point, nth: usize, fault: Fault) {
    arm(None);
    NTH.with(|cell| cell.set(Some((point, nth, fault))));
}
/// Arms one observation crossing (counted by `injected`) to return `Unknown`.
pub(crate) fn arm_observation(index: usize) {
    arm(None);
    OBSERVATION_ARMED.with(|cell| cell.set(Some(index)));
}
pub(crate) fn arm(fault: Option<(usize, Fault)>) {
    NAMED.with(|cell| cell.set(None));
    NTH.with(|cell| cell.set(None));
    PER_POINT.with(|counts| counts.borrow_mut().clear());
    ARMED.with(|armed| armed.set(fault));
    FIRED.with(|fired| fired.set(false));
    CROSSED.with(|crossed| crossed.set(0));
    TRACE.with(|trace| trace.borrow_mut().clear());
    OBSERVATIONS.with(|seen| seen.borrow_mut().clear());
    OBSERVATION_ARMED.with(|cell| cell.set(None));
    NEXT_UNKNOWN.with(|flag| flag.set(false));
    SKEW.with(|skew| skew.set(0));
    EVENTS.with(|events| events.borrow_mut().clear());
}
pub(crate) fn trace() -> Vec<Point> {
    TRACE.with(|trace| trace.borrow().clone())
}
pub(crate) fn observations() -> Vec<&'static Location<'static>> {
    OBSERVATIONS.with(|seen| seen.borrow().clone())
}
/// Whether the armed fault was delivered since the last `arm`.
pub(crate) fn fired() -> bool {
    FIRED.with(Cell::get)
}
/// Injected wall-clock offset (ClockJump); zero unless armed.
pub(crate) fn clock_skew_ms() -> i64 {
    SKEW.with(Cell::get)
}
pub(crate) fn record(event: Event) {
    EVENTS.with(|events| events.borrow_mut().push(event));
}
pub(crate) fn events() -> Vec<Event> {
    EVENTS.with(|events| events.borrow().clone())
}
/// Records every spawned helper's start identity in the driver's log, so
/// quiescence can wait for helpers that never reached registration.
pub(crate) fn note_helper(pid: u32) {
    note_child(pid, "");
}
/// Anchors cannot write; recovery must run before waiting for them to exit.
pub(crate) fn note_anchor(pid: u32) {
    note_child(pid, "anchor ");
}
fn note_child(pid: u32, prefix: &str) {
    let Some(path) = HELPERS.with(|log| log.borrow().clone()) else {
        return;
    };
    let identity = uninjected(|| crate::ident::process_identity(pid));
    if let Observed::Verified(identity, _) = identity {
        use std::io::Write;
        let line = serde_json::to_string(&identity).expect("identity");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("helper log");
        writeln!(file, "{prefix}{line}").expect("helper log");
        file.sync_all().expect("helper log");
    }
}
/// Harness instruments read real facts: no injection, no observation count.
pub(crate) fn uninjected<T>(f: impl FnOnce() -> T) -> T {
    let previous = SUSPENDED.with(|flag| flag.replace(true));
    let value = f();
    SUSPENDED.with(|flag| flag.set(previous));
    value
}
/// The helper log; children inherit it through `EFFIGY_KERNEL_TEST_HELPERS`.
pub(crate) fn set_helper_log(path: Option<std::path::PathBuf>) {
    HELPERS.with(|log| *log.borrow_mut() = path);
}
/// `EFFIGY_KERNEL_TEST_FAULT` is `<index>:<fault>` or `<Point>#<nth>:<fault>`.
pub(crate) fn arm_from_env() {
    if let Some(path) = std::env::var_os("EFFIGY_KERNEL_TEST_HELPERS") {
        set_helper_log(Some(path.into()));
    }
    let Ok(value) = std::env::var("EFFIGY_KERNEL_TEST_FAULT") else {
        return;
    };
    let (at, fault) = value.rsplit_once(':').expect("fault spec");
    let fault = Fault::parse(fault);
    match at.split_once('#') {
        Some((point, nth)) => {
            let point = *Point::ALL
                .iter()
                .find(|candidate| format!("{candidate:?}") == point)
                .unwrap_or_else(|| panic!("unknown point {point}"));
            arm_nth(point, nth.parse().expect("fault nth"), fault);
        }
        None => arm(Some((at.parse().expect("fault index"), fault))),
    }
}
/// The child-process spelling of `arm_nth`.
pub(crate) fn env_spec(point: Point, nth: usize, fault: Fault) -> String {
    format!("{point:?}#{nth}:{}", fault.name())
}
pub(crate) fn set_unknown(value: bool) {
    UNKNOWN.with(|flag| flag.set(value));
}
/// Strict fake for the operator's uid, so wrong-owner files are testable.
pub(crate) fn set_operator_uid(uid: Option<u32>) {
    OPERATOR_UID.with(|cell| cell.set(uid));
}
pub(crate) fn operator_uid() -> Option<u32> {
    OPERATOR_UID.with(Cell::get)
}
/// Strict fake for `statfs`: reports the state root on a network filesystem.
pub(crate) fn set_network_fs(value: bool) {
    NETWORK_FS.with(|cell| cell.set(value));
}
pub(crate) fn network_fs() -> bool {
    NETWORK_FS.with(Cell::get)
}

#[test]
fn unknown_injected_at_every_platform_observation_grants_no_fact() {
    set_unknown(true);
    assert!(matches!(
        crate::ident::boot_identity(),
        Observed::Unknown(UnknownReason::Injected)
    ));
    assert!(matches!(
        crate::ident::process_identity(std::process::id()),
        Observed::Unknown(UnknownReason::Injected)
    ));
    assert!(matches!(
        crate::ident::boot_clock(),
        Observed::Unknown(UnknownReason::Injected)
    ));
    assert!(matches!(
        crate::ident::process_state(std::process::id()),
        Observed::Unknown(UnknownReason::Injected)
    ));
    set_unknown(false);
}
