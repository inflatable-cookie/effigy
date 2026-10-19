//! Test-only fault points. Thread-local scope isolates strict fakes.
//!
//! A scenario first runs traced to list the points it crosses; each crossing
//! index can then be armed with one fault. Child processes arm through
//! `EFFIGY_KERNEL_TEST_FAULT=<index>:<kill|enospc|eperm>`, read only here.
use crate::observe::{Observed, UnknownReason};
use std::cell::{Cell, RefCell};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Point {
    RemoteBeforeIdentity,
    RemoteAfterIntent,
    RemoteAfterRecord,
    RemoteBeforeAck,
    RemoteAfterAckBeforeRecord,
    SocketBind,
    CaptureWrite,
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
    ClaimReleaseBefore,
    ClaimReleaseAfter,
    StoreCommitBefore,
    StoreCommitAfter,
    ReplaceOpen,
    ReplaceWrite,
    ReplaceFsync,
    ReplaceLinkPrev,
    ReplaceRename,
    ReplaceDirFsync,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    Kill,
    Enospc,
    Eperm,
}
type Hook = Box<dyn Fn(Point)>;
thread_local! {
    static HOOK: RefCell<Option<Hook>> = const {RefCell::new(None)};
    static UNKNOWN: Cell<bool> = const { Cell::new(false) };
    static NAMED: Cell<Option<(Point,Fault)>> = const {Cell::new(None)};
    static ARMED: Cell<Option<(usize, Fault)>> = const { Cell::new(None) };
    static CROSSED: Cell<usize> = const { Cell::new(0) };
    static TRACE: RefCell<Vec<Point>> = const { RefCell::new(Vec::new()) };
    static OPERATOR_UID: Cell<Option<u32>> = const { Cell::new(None) };
    static NETWORK_FS: Cell<bool> = const { Cell::new(false) };
}
pub(crate) fn injected<T>() -> Option<Observed<T>> {
    UNKNOWN.with(|flag| {
        flag.get()
            .then_some(Observed::Unknown(UnknownReason::Injected))
    })
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
    let named = NAMED.with(|cell| match cell.get() {
        Some((armed, fault)) if armed == point => {
            cell.set(None);
            Some((index, fault))
        }
        _ => None,
    });
    match named.or_else(|| ARMED.with(Cell::get)) {
        Some((armed, fault)) if armed == index => match fault {
            Fault::Kill => loop {
                // SAFETY: signalling our own pid; SIGKILL does not return.
                unsafe { libc::kill(libc::getpid(), libc::SIGKILL) };
            },
            Fault::Enospc => Err(io::Error::from_raw_os_error(libc::ENOSPC)),
            Fault::Eperm => Err(io::Error::from_raw_os_error(libc::EPERM)),
        },
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
pub(crate) fn arm(fault: Option<(usize, Fault)>) {
    NAMED.with(|cell| cell.set(None));
    ARMED.with(|armed| armed.set(fault));
    CROSSED.with(|crossed| crossed.set(0));
    TRACE.with(|trace| trace.borrow_mut().clear());
}
pub(crate) fn trace() -> Vec<Point> {
    TRACE.with(|trace| trace.borrow().clone())
}
pub(crate) fn arm_from_env() {
    let Ok(value) = std::env::var("EFFIGY_KERNEL_TEST_FAULT") else {
        return;
    };
    let (index, fault) = value.split_once(':').expect("fault spec");
    let fault = match fault {
        "kill" => Fault::Kill,
        "enospc" => Fault::Enospc,
        "eperm" => Fault::Eperm,
        other => panic!("unknown fault {other}"),
    };
    arm(Some((index.parse().expect("fault index"), fault)));
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
