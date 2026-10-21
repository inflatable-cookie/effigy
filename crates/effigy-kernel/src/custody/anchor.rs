//! An executable anchor stays in its birth group until workload closure. It
//! ignores cooperative signals; escalation targets workload identities only.
use super::*;
use crate::ident::{self, ProcessState};

fn verified(r: &ProcessReceipt, anchor: &ProcessIdentity) -> bool {
    platform::generation(anchor).verified().is_some()
        && matches!(ident::process_state(anchor.pid), Observed::Verified(ProcessState::Active | ProcessState::Stopped, _))
        && platform::read_nonce(anchor.pid).verified() == Some(&r.nonce)
        && platform::not_earlier(anchor, r)
        // SAFETY: generation validated the positive, range-checked pid.
        && unsafe { libc::getpgid(anchor.pid as i32) } == r.pgid as i32
}

pub(super) fn closed(
    r: &ProcessReceipt,
    anchor: Option<&ProcessIdentity>,
    leader: Option<u32>,
) -> Observed<Closure> {
    let Some(anchor) = anchor.filter(|a| verified(r, a)) else {
        return platform::group_closed(r.pgid, leader);
    };
    match platform::live_pids(r.pgid) {
        Ok(pids)
            if pids
                .iter()
                .all(|pid| *pid == anchor.pid || Some(*pid) == leader) =>
        {
            Observed::Verified(Closure, evidence())
        }
        Ok(_) => Observed::Unknown(unknown("workload still has live members")),
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
    #[cfg(target_os = "linux")]
    let fd = match platform::pidfd(member.pid) {
        Ok(fd) => fd,
        Err(e) if e.raw_os_error() == Some(libc::ESRCH) => {
            return Observed::Absent(AbsenceProof::NoSuchProcess { pid: member.pid })
        }
        Err(e) => return Observed::Unknown(unknown(e)),
    };
    match platform::generation(member) {
        Observed::Verified(..) => {}
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    }
    // SAFETY: identity observation validated the positive member pid.
    if unsafe { libc::getpgid(member.pid as i32) } != r.pgid as i32 || !verified(r, anchor) {
        return Observed::Unknown(unknown("group anchor unavailable"));
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: live pidfd, null siginfo and flags zero target this identity.
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0u32,
            )
        } == 0
        {
            Observed::Verified(
                SignalDelivered {
                    pid: member.pid,
                    group: false,
                    signal,
                },
                evidence(),
            )
        } else {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::ESRCH) {
                Observed::Absent(AbsenceProof::NoSuchProcess { pid: member.pid })
            } else {
                Observed::Unknown(unknown(e))
            }
        }
    }
    #[cfg(target_os = "macos")]
    platform::numeric(member.pid, false, signal)
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
        return Observed::Unknown(unknown("group anchor unavailable"));
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
        .unwrap_or_else(|| Observed::Unknown(unknown("no live workload member to signal")))
}

pub(super) fn stop(
    r: &ProcessReceipt,
    anchor: Option<&ProcessIdentity>,
    cx: &CleanupContext,
) -> Observed<Closure> {
    let Some(anchor) = anchor else {
        return platform::group_closed(r.pgid, None);
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
        let state = platform::group_closed(r.pgid, None);
        if matches!(state, Observed::Verified(..) | Observed::Absent(_)) {
            return state;
        }
        if let Err(e) = cx.operation().check("anchor stop") {
            return Observed::Unknown(unknown(format!("{e:?}")));
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
            return Observed::Unknown(unknown(format!("{e:?}")));
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
                return Observed::Unknown(reason);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
