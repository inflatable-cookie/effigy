//! An executable anchor stays in its birth group until workload closure. It
//! ignores cooperative signals; escalation targets workload identities only.
use super::*;
use crate::ident::{self, ProcessState};

const NO_LIVE_MEMBER: &str = "no live workload member to signal";

pub(super) fn verified(r: &ProcessReceipt, anchor: &ProcessIdentity) -> bool {
    platform::generation(anchor).verified().is_some()
        && matches!(ident::process_state(anchor.pid), Observed::Verified(ProcessState::Active | ProcessState::Stopped, _))
        && platform::read_nonce(anchor.pid).verified() == Some(&r.nonce)
        && platform::not_earlier(anchor, r)
        // SAFETY: generation validated the positive, range-checked pid.
        && unsafe { libc::getpgid(anchor.pid as i32) } == r.pgid as i32
}

pub(super) fn closed<'a>(
    r: &ProcessReceipt,
    anchor: Option<&'a ProcessIdentity>,
    leader: Option<&'a ProcessIdentity>,
) -> Observed<Closure> {
    let Some(anchor) = anchor.filter(|a| verified(r, a)) else {
        let accounted: Vec<&ProcessIdentity> = leader.into_iter().collect();
        return platform::group_closed(r.pgid, &accounted);
    };
    let mut accounted: Vec<&ProcessIdentity> = vec![anchor];
    accounted.extend(leader);
    match platform::group_membership(r.pgid).map(|rows| platform::classify_round(&rows, &accounted))
    {
        Ok(platform::RoundVerdict::Empty) => Observed::Verified(Closure, evidence()),
        Ok(platform::RoundVerdict::Live) => {
            Observed::Unknown(unknown("workload still has live members"))
        }
        // A row that could not be resolved may hide a member this inventory
        // cannot see: never closure, and the caller re-inventories.
        Ok(platform::RoundVerdict::Unproven) => {
            Observed::Unknown(unknown(crate::custody::UNPROVEN_GROUP_CLOSURE))
        }
        Err(e) => Observed::Unknown(unknown(e)),
    }
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
    if ![libc::SIGTERM, libc::SIGCONT, libc::SIGKILL, libc::SIGINT].contains(&signal) {
        return Observed::Unknown(unknown("invalid custody signal"));
    }
    if !verified(r, anchor) {
        // The anchor is the group's continuity evidence. Without it the group
        // cannot be proved closed, so the refusal carries the named state.
        return Observed::Unknown(unknown(crate::custody::UNPROVEN_GROUP_CLOSURE));
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
        for sig in [sig, libc::SIGCONT] {
            if let Observed::Unknown(reason) = signal(r, anchor, sig) {
                if matches!(
                    closed(r, Some(anchor), None),
                    Observed::Verified(..) | Observed::Absent(_)
                ) {
                    return stop(r, Some(anchor), cx);
                }
                if matches!(&reason, crate::observe::UnknownReason::Inconsistent(message) if message == NO_LIVE_MEMBER)
                {
                    break;
                }
                return Observed::Unknown(reason);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
