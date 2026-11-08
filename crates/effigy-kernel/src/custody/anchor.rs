//! An executable anchor stays in its birth group until workload closure. It
//! ignores cooperative signals; escalation targets workload identities only.
use super::*;
use crate::ident::{self, ProcessState};

const NO_LIVE_MEMBER: &str = "no live workload member to signal";

// These labels contain only categories, never observation payloads.
#[derive(Clone, Copy)]
pub(super) struct VerificationFailure {
    pub stage: &'static str,
    pub category: &'static str,
}

fn required<T>(stage: &'static str, observation: Observed<T>) -> Result<T, VerificationFailure> {
    match observation {
        Observed::Verified(value, _) => Ok(value),
        Observed::Absent(proof) => Err(VerificationFailure {
            stage,
            category: match proof {
                AbsenceProof::StartIdentityDiffers { .. } | AbsenceProof::BootChanged { .. } => {
                    "replaced"
                }
                _ => "absent",
            },
        }),
        Observed::Unknown(_) => Err(VerificationFailure {
            stage,
            category: "unreadable",
        }),
    }
}

// Lazy observations preserve the original short-circuit order and count.
pub(super) fn verify_with(
    generation: impl FnOnce() -> Observed<()>,
    state: impl FnOnce() -> Observed<ProcessState>,
    nonce: impl FnOnce() -> Result<bool, &'static str>,
    birth: impl FnOnce() -> Result<(), &'static str>,
    group: impl FnOnce() -> i32,
    pgid: u32,
) -> Result<(), VerificationFailure> {
    required("generation", generation())?;
    if required("state", state())? == ProcessState::Zombie {
        return Err(VerificationFailure {
            stage: "state",
            category: "zombie",
        });
    }
    match nonce() {
        Ok(true) => {}
        Ok(false) => {
            return Err(VerificationFailure {
                stage: "nonce",
                category: "mismatch",
            })
        }
        Err(category) => {
            return Err(VerificationFailure {
                stage: "nonce",
                category,
            })
        }
    }
    birth().map_err(|category| VerificationFailure {
        stage: "birth",
        category,
    })?;
    let current = group();
    if current != pgid as i32 {
        return Err(VerificationFailure {
            stage: "current-group",
            category: if current < 0 {
                "unreadable"
            } else {
                "mismatch"
            },
        });
    }
    Ok(())
}

fn verification(r: &ProcessReceipt, anchor: &ProcessIdentity) -> Result<(), VerificationFailure> {
    verify_with(
        || platform::generation(anchor),
        || ident::process_state(anchor.pid),
        || platform::anchor_nonce(anchor.pid).map(|nonce| nonce == r.nonce),
        || {
            if platform::not_earlier(anchor, r) {
                Ok(())
            } else if anchor.boot != r.identity.boot {
                Err("boot-mismatch")
            } else {
                Err("birth-order-or-kind")
            }
        },
        // SAFETY: generation validated the positive, range-checked pid.
        || unsafe { libc::getpgid(anchor.pid as i32) },
        r.pgid,
    )
}

pub(super) fn verified(r: &ProcessReceipt, anchor: &ProcessIdentity) -> bool {
    verification(r, anchor).is_ok()
}

#[derive(Clone, Copy)]
pub(super) struct ClosureDetail {
    pub path: &'static str,
    pub result: &'static str,
    pub role: &'static str,
}

pub(super) fn closed<'a>(
    r: &ProcessReceipt,
    anchor: Option<&'a ProcessIdentity>,
    leader: Option<&'a ProcessIdentity>,
) -> Observed<Closure> {
    closed_detail(r, anchor, leader).0
}

fn closed_detail(
    r: &ProcessReceipt,
    anchor: Option<&ProcessIdentity>,
    leader: Option<&ProcessIdentity>,
) -> (Observed<Closure>, ClosureDetail) {
    let verified_anchor = anchor.filter(|a| verified(r, a));
    let mut accounted: Vec<&ProcessIdentity> = verified_anchor.into_iter().collect();
    accounted.extend(leader);
    platform::closure_detail(
        r.pgid,
        &accounted,
        anchor,
        leader.or(Some(&r.identity)),
        verified_anchor.is_some(),
    )
}

/// Each pidfd opens before the generation observation; on macOS the documented
/// identity-check/numeric-signal window remains. An open pidfd never justifies
/// a numeric group signal. Membership and anchor evidence are checked afresh.
fn member_signal(
    r: &ProcessReceipt,
    anchor: &ProcessIdentity,
    member: &ProcessIdentity,
    signal: i32,
) -> Observed<SignalDelivered> {
    platform::signal_checked(member, signal, || {
        #[cfg(test)]
        if let Some(decision) = seam::authority(anchor.pid) {
            return decision;
        }
        // SAFETY: signal_checked has verified the positive member generation.
        (unsafe { libc::getpgid(member.pid as i32) }) == r.pgid as i32 && verified(r, anchor)
    })
}

/// Whether an anchor's observed state means it has retired: it is gone, or it
/// exited and waits to be reaped. Its group's closure is then judged from the
/// group itself, never from the anchor.
pub(super) fn retired(state: &Observed<ProcessState>) -> bool {
    matches!(
        state,
        Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _)
    )
}

/// Test seam for the one window the anchor can retire inside a kill: after its
/// generation read succeeds and before the authority check re-reads it. Only
/// the next authority check for the armed anchor is affected.
#[cfg(test)]
pub(crate) mod seam {
    use super::*;
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Authority {
        /// SIGKILL the anchor at the crossing, then let the real check observe it.
        RetireAnchor,
        /// Refuse the signal while the anchor is still live.
        RefuseLive,
    }

    thread_local! {
        static ARMED: Cell<Option<(u32, Authority)>> = const { Cell::new(None) };
    }

    pub(crate) fn arm(pid: u32, action: Authority) {
        ARMED.with(|armed| armed.set(Some((pid, action))));
    }

    pub(crate) fn disarm() {
        ARMED.with(|armed| armed.set(None));
    }

    /// The decision forced for this crossing, or `None` for the real check.
    pub(super) fn authority(pid: u32) -> Option<bool> {
        let (armed, action) = ARMED.with(Cell::get)?;
        if armed != pid {
            return None;
        }
        disarm();
        match action {
            Authority::RefuseLive => Some(false),
            Authority::RetireAnchor => {
                // SAFETY: the pid is the anchor this test launched.
                unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                let deadline = Instant::now() + Duration::from_secs(5);
                while !retired(&ident::process_state(pid)) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                None
            }
        }
    }
}

pub(super) fn signal(
    r: &ProcessReceipt,
    anchor: &ProcessIdentity,
    signal: i32,
) -> Observed<SignalDelivered> {
    signal_detail(r, anchor, signal).0
}

#[derive(Clone, Copy)]
pub(super) struct SignalFailure {
    pub verification: Option<VerificationFailure>,
    pub stage: &'static str,
}

fn signal_detail(
    r: &ProcessReceipt,
    anchor: &ProcessIdentity,
    signal: i32,
) -> (Observed<SignalDelivered>, SignalFailure) {
    let mut failure_detail = SignalFailure {
        verification: None,
        stage: "signal",
    };
    let observed = (|| {
        if ![libc::SIGTERM, libc::SIGCONT, libc::SIGKILL, libc::SIGINT].contains(&signal) {
            return Observed::Unknown(unknown("invalid custody signal"));
        }
        failure_detail.stage = "anchor-verification";
        if let Err(failure) = verification(r, anchor) {
            failure_detail.verification = Some(failure);
            // The anchor is the group's continuity evidence. Without it the group
            // cannot be proved closed, so the refusal carries the named state.
            return Observed::Unknown(unknown(format!(
                "{}; anchor={}:{}",
                crate::custody::UNPROVEN_GROUP_CLOSURE,
                failure.stage,
                failure.category
            )));
        }
        failure_detail.stage = "inventory";
        let pids = match platform::live_pids(r.pgid) {
            Ok(pids) => pids,
            Err(e) => return Observed::Unknown(unknown(e)),
        };
        let mut delivered = None;
        for pid in pids.into_iter().filter(|pid| *pid != anchor.pid) {
            failure_detail.stage = "member-identity";
            let member = match ident::process_identity(pid) {
                Observed::Verified(member, _) => member,
                Observed::Absent(_) => continue,
                Observed::Unknown(reason) => return Observed::Unknown(reason),
            };
            failure_detail.stage = "member-signal";
            let sent = member_signal(r, anchor, &member, signal);
            if let Observed::Verified(sent, _) = &sent {
                delivered = Some(sent.clone());
            }
            if let Observed::Unknown(reason) = sent {
                // A process can retire between inventory and its evidence read.
                if matches!(
                    ident::process_state(pid),
                    Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _)
                ) {
                    continue;
                }
                return Observed::Unknown(reason);
            }
        }
        delivered
            .map(|sent| Observed::Verified(sent, evidence()))
            .unwrap_or_else(|| Observed::Unknown(unknown(NO_LIVE_MEMBER)))
    })();
    (observed, failure_detail)
}

pub(super) fn stop(
    r: &ProcessReceipt,
    anchor: Option<&ProcessIdentity>,
    cx: &CleanupContext,
) -> Observed<Closure> {
    let Some(anchor) = anchor else {
        return platform::group_closed(r.pgid, &[]);
    };
    if verified(r, anchor) {
        if let Err(e) = crate::fault_point!(AnchorStop) {
            return Observed::Unknown(unknown(e));
        }
        if let Observed::Unknown(reason) = member_signal(r, anchor, anchor, libc::SIGKILL) {
            // The anchor retires itself once its group has no live member, so it
            // can exit between the liveness check above and this kill. A refused
            // kill of a retired anchor has nothing left to kill. Closure is still
            // proven below from the group, never from the anchor's exit.
            if !retired(&ident::process_state(anchor.pid)) {
                return Observed::Unknown(reason);
            }
        }
        if let Err(e) = crate::fault_point!(AnchorStopped) {
            return Observed::Unknown(unknown(e));
        }
    }
    loop {
        let state = platform::group_closed(r.pgid, &[]);
        if matches!(state, Observed::Verified(..) | Observed::Absent(_)) {
            return state;
        }
        if let Err(e) = cx.operation().check("anchor stop") {
            return Observed::Unknown(unknown(e));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn recover(
    r: &ProcessReceipt,
    anchor: &ProcessIdentity,
    cx: &CleanupContext,
    escalate: &mut impl FnMut() -> bool,
    refusal: &mut Option<String>,
) -> Observed<Closure> {
    let grace = Instant::now()
        .checked_add(Duration::from_millis(r.grace_millis))
        .unwrap_or_else(Instant::now);
    loop {
        // An already empty group needs no authority. Never ignore a stale
        // anchor pid: exclusion itself requires its full generation evidence.
        if matches!(
            closed(r, Some(anchor), None),
            Observed::Verified(..) | Observed::Absent(_)
        ) {
            return stop(r, Some(anchor), cx);
        }
        if let Err(e) = cx.operation().check("anchored recovery") {
            return Observed::Unknown(unknown(e));
        }
        let sig = if Instant::now() >= grace || crate::signal::escalated() || escalate() {
            libc::SIGKILL
        } else {
            libc::SIGTERM
        };
        let selected = sig;
        for sig in [selected, libc::SIGCONT] {
            let (sent, failure) = signal_detail(r, anchor, sig);
            if let Observed::Unknown(reason) = sent {
                let (closure, detail) = closed_detail(r, Some(anchor), None);
                if matches!(closure, Observed::Verified(..) | Observed::Absent(_)) {
                    return stop(r, Some(anchor), cx);
                }
                if matches!(&reason, crate::observe::UnknownReason::Inconsistent(message) if message == NO_LIVE_MEMBER)
                {
                    break;
                }
                *refusal = Some(recovery_refusal(&reason, failure, selected, sig, detail));
                return Observed::Unknown(reason);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn signal_name(signal: i32) -> &'static str {
    match signal {
        libc::SIGTERM => "TERM",
        libc::SIGKILL => "KILL",
        libc::SIGCONT => "CONT",
        _ => "other",
    }
}

// The separate journal detail keeps even payload-free UnknownReason variants
// intact. Only fixed categories enter the detail, never the reason's payload.
pub(super) fn recovery_refusal(
    reason: &UnknownReason,
    failure: SignalFailure,
    selected: i32,
    attempt: i32,
    closure: ClosureDetail,
) -> String {
    let category = match reason {
        UnknownReason::TimedOut { .. } => "timed-out",
        UnknownReason::Busy { .. } => "busy",
        UnknownReason::Cancelled(_) => "cancelled",
        UnknownReason::PermissionDenied => "permission-denied",
        UnknownReason::ToolUnavailable(_) => "tool-unavailable",
        UnknownReason::Unparsable(_) => "unparsable",
        UnknownReason::Inconsistent(_) => "inconsistent",
        UnknownReason::GuardUnavailable => "guard-unavailable",
        UnknownReason::Injected => "injected",
        UnknownReason::Untrusted(_) => "untrusted",
        UnknownReason::HostAuthorityRequired => "host-authority-required",
        UnknownReason::StoreUnreadable(_) => "store-unreadable",
    };
    let anchor = failure.verification.map_or_else(
        || "verified-at-entry".to_owned(),
        |failed| format!("{}:{}", failed.stage, failed.category),
    );
    format!(
        "{}; anchor={anchor}; refusal={}:{category}; stop={} attempt={} closure={}:{} row={}",
        crate::custody::UNPROVEN_GROUP_CLOSURE,
        failure.stage,
        signal_name(selected),
        signal_name(attempt),
        closure.path,
        closure.result,
        closure.role
    )
}
