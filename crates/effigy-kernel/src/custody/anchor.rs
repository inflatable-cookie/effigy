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
        // SAFETY: signal_checked has verified the positive member generation.
        (unsafe { libc::getpgid(member.pid as i32) }) == r.pgid as i32 && verified(r, anchor)
    })
}

pub(super) fn signal(
    r: &ProcessReceipt,
    anchor: &ProcessIdentity,
    signal: i32,
) -> Observed<SignalDelivered> {
    signal_detail(r, anchor, signal).0
}

fn signal_detail(
    r: &ProcessReceipt,
    anchor: &ProcessIdentity,
    signal: i32,
) -> (Observed<SignalDelivered>, Option<VerificationFailure>) {
    let mut failure_detail = None;
    let observed = (|| {
        if ![libc::SIGTERM, libc::SIGCONT, libc::SIGKILL, libc::SIGINT].contains(&signal) {
            return Observed::Unknown(unknown("invalid custody signal"));
        }
        if let Err(failure) = verification(r, anchor) {
            failure_detail = Some(failure);
            // The anchor is the group's continuity evidence. Without it the group
            // cannot be proved closed, so the refusal carries the named state.
            return Observed::Unknown(unknown(format!(
                "{}; anchor={}:{}",
                crate::custody::UNPROVEN_GROUP_CLOSURE,
                failure.stage,
                failure.category
            )));
        }
        let pids = match platform::live_pids(r.pgid) {
            Ok(pids) => pids,
            Err(e) => return Observed::Unknown(unknown(e)),
        };
        let mut delivered = None;
        for pid in pids.into_iter().filter(|pid| *pid != anchor.pid) {
            let member = match ident::process_identity(pid) {
                Observed::Verified(member, _) => member,
                Observed::Absent(_) => continue,
                Observed::Unknown(reason) => return Observed::Unknown(reason),
            };
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
            return Observed::Unknown(reason);
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
                return Observed::Unknown(recovery_refusal(reason, failure, selected, sig, detail));
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

pub(super) fn recovery_refusal(
    reason: UnknownReason,
    failure: Option<VerificationFailure>,
    selected: i32,
    attempt: i32,
    closure: ClosureDetail,
) -> UnknownReason {
    // Preserve variant and original reason. Only our bounded anchor refusal
    // gains context; arbitrary external error strings are never copied here.
    match reason {
        UnknownReason::Inconsistent(message) if failure.is_some() => {
            UnknownReason::Inconsistent(format!(
                "{message}; stop={} attempt={} closure={}:{} row={}",
                signal_name(selected),
                signal_name(attempt),
                closure.path,
                closure.result,
                closure.role
            ))
        }
        other => other,
    }
}
