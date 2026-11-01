//! OS observations and generation-evidenced signals. Numeric recovery on macOS
//! and pre-6.9 Linux has the contract's documented check/signal window; a pidfd
//! is never used as permission for a numeric group kill.
use super::*;
use crate::ident::{self, ProcessStartIdentity, ProcessState};
#[cfg(target_os = "linux")]
use std::io::Read;
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;

#[derive(Debug, Clone)]
pub(crate) struct Member {
    pub identity: ProcessIdentity,
    pub nonce: Observed<String>,
}
pub(crate) trait ProcessTable {
    fn leader(&mut self, receipt: &ProcessReceipt) -> Observed<ProcessIdentity>;
    fn members(&mut self, pgid: u32) -> Observed<Vec<Member>>;
    /// Whether `identity` is still a live process. A member can leave the
    /// group between the inventory and its evidence read; only a member that
    /// is still present can be foreign, never one that is provably gone.
    fn present(&mut self, identity: &ProcessIdentity) -> Observed<bool>;
    fn signal_leader(&mut self, receipt: &ProcessReceipt, signal: i32)
        -> Observed<SignalDelivered>;
    fn signal_member(
        &mut self,
        member: &Member,
        receipt: &ProcessReceipt,
        signal: i32,
    ) -> Observed<SignalDelivered>;
}
#[derive(Debug)]
struct Host;
fn gone(pgid: u32) -> Observed<Closure> {
    Observed::Absent(AbsenceProof::GroupEmpty { pgid })
}
fn group_exists(pgid: u32) -> Observed<()> {
    if pgid == 0 || pgid > i32::MAX as u32 {
        return Observed::Unknown(unknown("invalid process group"));
    }
    // SAFETY: validated positive group, signal zero only observes existence.
    if unsafe { libc::kill(-(pgid as i32), 0) } == 0 {
        return Observed::Verified((), evidence());
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::ESRCH) {
        Observed::Absent(AbsenceProof::GroupEmpty { pgid })
    } else {
        Observed::Unknown(unknown(e))
    }
}
pub(crate) fn owner_signal(receipt: &ProcessReceipt, signal: i32) -> Observed<SignalDelivered> {
    // Caller holds its unreaped direct leader. The group number remains pinned.
    numeric(receipt.pgid, true, signal)
}
pub(crate) fn numeric(pid: u32, group: bool, signal: i32) -> Observed<SignalDelivered> {
    if pid == 0
        || pid > i32::MAX as u32
        || ![libc::SIGTERM, libc::SIGCONT, libc::SIGKILL, libc::SIGINT].contains(&signal)
    {
        return Observed::Unknown(unknown("invalid signal target"));
    }
    // SAFETY: positive, range-checked target and allowed signal; authority is
    // established by the caller's unreaped child or immediate identity check.
    let result = unsafe { libc::kill(if group { -(pid as i32) } else { pid as i32 }, signal) };
    if result == 0 {
        Observed::Verified(SignalDelivered { pid, group, signal }, evidence())
    } else {
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::ESRCH) => Observed::Absent(AbsenceProof::NoSuchProcess { pid }),
            // A refused signal is an authority fact, not an inconsistent
            // reading. Keep it distinguishable so an owner can re-verify the
            // target instead of reading the refusal itself as failure.
            Some(libc::EPERM | libc::EACCES) => Observed::Unknown(UnknownReason::PermissionDenied),
            _ => Observed::Unknown(unknown(e)),
        }
    }
}
pub(crate) fn not_earlier(member: &ProcessIdentity, receipt: &ProcessReceipt) -> bool {
    if member.boot != receipt.identity.boot {
        return false;
    }
    match (&member.start, &receipt.identity.start) {
        (ProcessStartIdentity::LinuxTicks(a), ProcessStartIdentity::LinuxTicks(b)) => a >= b,
        (
            ProcessStartIdentity::MacOs {
                seconds: a,
                microseconds: aa,
            },
            ProcessStartIdentity::MacOs {
                seconds: b,
                microseconds: bb,
            },
        ) => (a, aa) >= (b, bb),
        _ => false,
    }
}
/// Closed means no live member; an unreaped leader/zombie supplies no workload
/// and is reaped only by its owner. A vanished group has an ESRCH proof.
/// Owner closure. The owner holds the leader unreaped and drives the kill, so a
/// row is only evidence when it was positively resolved: no positively live member,
/// no row that stayed ambiguous. An ambiguous row reports
/// [`crate::custody::UNPROVEN_GROUP_CLOSURE`], and the caller re-inventories rather
/// than treating it as absence.
pub(crate) fn group_closed(pgid: u32, accounted: &[&ProcessIdentity]) -> Observed<Closure> {
    if let Observed::Absent(_) = group_exists(pgid) {
        return gone(pgid);
    }
    match group_membership(pgid).map(|rows| classify_round(&rows, accounted)) {
        Ok(RoundVerdict::Empty) => Observed::Verified(Closure, evidence()),
        Ok(RoundVerdict::Live) => Observed::Unknown(unknown("group still has live members")),
        Ok(RoundVerdict::Unproven) => {
            Observed::Unknown(unknown(crate::custody::UNPROVEN_GROUP_CLOSURE))
        }
        Err(e) => Observed::Unknown(unknown(e)),
    }
}
pub(super) fn recover(
    receipt: &ProcessReceipt,
    cx: &CleanupContext,
    escalate: &mut impl FnMut() -> bool,
) -> Observed<Closure> {
    recover_with(&mut Host, receipt, cx, escalate)
}
pub(crate) fn recover_with(
    table: &mut impl ProcessTable,
    receipt: &ProcessReceipt,
    cx: &CleanupContext,
    escalate: &mut impl FnMut() -> bool,
) -> Observed<Closure> {
    let grace = Instant::now()
        .checked_add(Duration::from_millis(receipt.grace_millis))
        .unwrap_or_else(Instant::now);
    let mut killed = false;
    loop {
        if let Err(e) = cx.operation().check("custody recovery") {
            return Observed::Unknown(unknown(format!("{e}")));
        }
        let signal = if Instant::now() >= grace || crate::signal::escalated() || escalate() {
            killed = true;
            libc::SIGKILL
        } else {
            libc::SIGTERM
        };
        // Every turn rechecks evidence, including after TERM/CONT and at KILL.
        let mut group_authority = false;
        let leader = table.leader(receipt);
        // O8 (§12.1): only a positive identity match may precede a signal.
        #[cfg(all(test, feature = "fault-injection"))]
        crate::fault::record(crate::fault::Event::Observed {
            subject: format!("custody-leader:{}", receipt.nonce),
            unknown: !matches!(&leader, Observed::Verified(current, _) if *current == receipt.identity),
        });
        match leader {
            Observed::Verified(current, _) if current == receipt.identity => {
                group_authority = true;
                let mut sent = false;
                for sig in [signal, libc::SIGCONT] {
                    #[cfg(all(test, feature = "fault-injection"))]
                    crate::fault::record(crate::fault::Event::Destructive {
                        subject: format!("custody-leader:{}", receipt.nonce),
                    });
                    match table.signal_leader(receipt, sig) {
                        Observed::Verified(..) => sent = true,
                        // TERM/KILL may have terminated the leader before CONT.
                        // An uncertain CONT grants no signal authority: observe
                        // membership again, using nonce evidence if needed.
                        Observed::Unknown(_) if sent => {
                            group_authority = false;
                            break;
                        }
                        Observed::Absent(_) => {
                            group_authority = false;
                            break;
                        }
                        Observed::Unknown(reason) => return Observed::Unknown(reason),
                    }
                }
            }
            Observed::Unknown(reason) => return Observed::Unknown(reason),
            _ => {}
        }
        match table.members(receipt.pgid) {
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
            Observed::Verified(members, _) if members.is_empty() => {
                return Observed::Verified(Closure, evidence())
            }
            Observed::Verified(_, _) if group_authority => {}
            Observed::Verified(members, _) => {
                let mut foreign = Vec::new();
                for member in members {
                    let evidenced = member
                        .nonce
                        .verified()
                        .is_some_and(|nonce| nonce == &receipt.nonce)
                        && not_earlier(&member.identity, receipt);
                    #[cfg(all(test, feature = "fault-injection"))]
                    crate::fault::record(crate::fault::Event::Observed {
                        subject: format!("custody-member:{}", member.identity.pid),
                        unknown: !evidenced,
                    });
                    if !evidenced {
                        // Its environment is gone (or cleared) while its pid
                        // was still live: the member can have exited between
                        // the inventory and this read. A vanished member is
                        // not residue, so re-inventory instead of reporting
                        // it; a present one without the nonce still is.
                        match table.present(&member.identity) {
                            Observed::Verified(false, _) => continue,
                            Observed::Verified(true, _) => {}
                            Observed::Unknown(reason) => return Observed::Unknown(reason),
                            Observed::Absent(_) => continue,
                        }
                        foreign.push(format!(
                            "pid {} nonce={:?} age_verified={}",
                            member.identity.pid,
                            member.nonce,
                            not_earlier(&member.identity, receipt)
                        ));
                        continue;
                    }
                    let mut sent = false;
                    for sig in [signal, libc::SIGCONT] {
                        #[cfg(all(test, feature = "fault-injection"))]
                        crate::fault::record(crate::fault::Event::Destructive {
                            subject: format!("custody-member:{}", member.identity.pid),
                        });
                        match table.signal_member(&member, receipt, sig) {
                            Observed::Verified(..) => sent = true,
                            Observed::Absent(_) => break,
                            // The previous verified signal can close the process
                            // between identity and environment observations. Do
                            // not send CONT; reobserve the group on the next turn.
                            Observed::Unknown(_) if sent => break,
                            Observed::Unknown(reason) => return Observed::Unknown(reason),
                        }
                    }
                }
                if !foreign.is_empty() {
                    return Observed::Unknown(unknown(format!(
                        "group contains members without generation evidence: {foreign:?}"
                    )));
                }
            }
        }
        if killed {
            std::thread::sleep(Duration::from_millis(2));
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl ProcessTable for Host {
    fn leader(&mut self, r: &ProcessReceipt) -> Observed<ProcessIdentity> {
        match ident::process_state(r.identity.pid) {
            Observed::Verified(ProcessState::Zombie, _) => {
                Observed::Absent(AbsenceProof::NoSuchProcess {
                    pid: r.identity.pid,
                })
            }
            Observed::Unknown(reason) => Observed::Unknown(reason),
            _ => ident::process_identity(r.identity.pid),
        }
    }
    fn present(&mut self, identity: &ProcessIdentity) -> Observed<bool> {
        match ident::process_state(identity.pid) {
            Observed::Verified(ProcessState::Zombie, _) | Observed::Absent(_) => {
                Observed::Verified(false, evidence())
            }
            Observed::Verified(_, _) => Observed::Verified(true, evidence()),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }
    fn members(&mut self, pgid: u32) -> Observed<Vec<Member>> {
        if let Observed::Absent(proof) = group_exists(pgid) {
            return Observed::Absent(proof);
        }
        let pids = match live_pids(pgid) {
            Ok(p) => p,
            Err(e) => return Observed::Unknown(unknown(e)),
        };
        let mut members = Vec::new();
        for pid in pids {
            match ident::process_identity(pid) {
                Observed::Verified(identity, _) => members.push(Member {
                    identity,
                    nonce: read_nonce(pid),
                }),
                Observed::Absent(_) => {}
                Observed::Unknown(reason) => return Observed::Unknown(reason),
            }
        }
        Observed::Verified(members, evidence())
    }
    fn signal_leader(&mut self, r: &ProcessReceipt, signal: i32) -> Observed<SignalDelivered> {
        #[cfg(target_os = "linux")]
        {
            let fd = match pidfd(r.identity.pid) {
                Ok(fd) => fd,
                Err(e) if e.raw_os_error() == Some(libc::ESRCH) => {
                    return Observed::Absent(AbsenceProof::NoSuchProcess {
                        pid: r.identity.pid,
                    })
                }
                Err(e) => return Observed::Unknown(unknown(e)),
            };
            match generation(&r.identity) {
                Observed::Verified(..) => {}
                Observed::Absent(proof) => return Observed::Absent(proof),
                Observed::Unknown(reason) => return Observed::Unknown(reason),
            }
            // SAFETY: live pidfd, supported signal, null siginfo; flag 4 selects
            // the original process group's kernel identity on Linux >=6.9.
            let sent = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    signal,
                    std::ptr::null::<libc::siginfo_t>(),
                    4u32,
                )
            };
            if sent == 0 {
                return Observed::Verified(
                    SignalDelivered {
                        pid: r.pgid,
                        group: true,
                        signal,
                    },
                    evidence(),
                );
            }
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EINVAL) {
                return if e.raw_os_error() == Some(libc::ESRCH) {
                    Observed::Absent(AbsenceProof::NoSuchProcess {
                        pid: r.identity.pid,
                    })
                } else {
                    Observed::Unknown(unknown(e))
                };
            }
            // EINVAL is the capability probe for old kernels. Re-check once
            // more immediately before the documented numeric-signal window.
        }
        match generation(&r.identity) {
            Observed::Verified(..) => {}
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        }
        numeric(r.pgid, true, signal)
    }
    fn signal_member(
        &mut self,
        m: &Member,
        r: &ProcessReceipt,
        signal: i32,
    ) -> Observed<SignalDelivered> {
        signal_checked(&m.identity, signal, || {
            read_nonce(m.identity.pid).verified() == Some(&r.nonce) && not_earlier(&m.identity, r)
        })
    }
}
/// Open a pidfd before observing generation and the caller's positive authority.
pub(super) fn signal_checked(
    identity: &ProcessIdentity,
    signal: i32,
    authority: impl FnOnce() -> bool,
) -> Observed<SignalDelivered> {
    #[cfg(target_os = "linux")]
    let fd = match pidfd(identity.pid) {
        Ok(fd) => fd,
        Err(e) if e.raw_os_error() == Some(libc::ESRCH) => {
            return Observed::Absent(AbsenceProof::NoSuchProcess { pid: identity.pid })
        }
        Err(e) => return Observed::Unknown(unknown(e)),
    };
    // Open the descriptor before observing identity and environment. Neither
    // a stale process table entry nor a bare numeric pid grants authority.
    match generation(identity) {
        Observed::Verified(..) => {}
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    }
    if !authority() {
        return Observed::Unknown(unknown("member generation unverified"));
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: live per-process pidfd opened before evidence checks;
        // flags zero targets the referenced process, never a numeric pid.
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
            return Observed::Verified(
                SignalDelivered {
                    pid: identity.pid,
                    group: false,
                    signal,
                },
                evidence(),
            );
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ESRCH) {
            Observed::Absent(AbsenceProof::NoSuchProcess { pid: identity.pid })
        } else {
            Observed::Unknown(unknown(e))
        }
    }
    #[cfg(target_os = "macos")]
    numeric(identity.pid, false, signal)
}
#[cfg(target_os = "linux")]
pub(crate) fn pidfd(pid: u32) -> io::Result<OwnedFd> {
    // SAFETY: positive process id with flags zero; result is a fresh fd.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0u32) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful syscall returned a fresh owned fd.
    Ok(unsafe { OwnedFd::from_raw_fd(raw as i32) })
}
/// How one listed candidate resolved in a group inventory round.
///
/// Only a positively resolved row is evidence. A row that was live and then could
/// not be read — or that the kernel lists in the group without a readable
/// identity — is *unproven*: it may have forked a member this inventory cannot see,
/// so it supports neither closure nor an anchor's retirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MembershipRow {
    /// Positively observed live in this group, with its kernel generation.
    Live(ProcessIdentity),
    /// Positively not running: the kernel reports it as a zombie, which cannot fork.
    NotLive(u32),
    /// Listed in the group but unreadable at its state or identity read: it may
    /// have left inside the round after forking a member this inventory cannot see.
    /// Ambiguous, never absence.
    Unproven(u32),
}

/// One inventory of the group, keeping each row's outcome. A failed read that is
/// not simply "no such process" is an error: an unreadable member is never absent.
pub(crate) fn group_membership(pgid: u32) -> io::Result<Vec<MembershipRow>> {
    let mut rows = Vec::new();
    for pid in group_pids(pgid)? {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: pid came from the OS inventory; getpgid only observes it.
            let group = unsafe { libc::getpgid(pid as i32) };
            if group < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::ESRCH) {
                    // Gone before we could ask whether it was ours. Linux lists
                    // every pid, so this is usually an unrelated short-lived
                    // process; it is neither a member nor closure evidence.
                    continue;
                }
                return Err(e);
            }
            if group as u32 != pgid {
                continue;
            }
        }
        match ident::process_state(pid) {
            Observed::Verified(ProcessState::Zombie, _) => rows.push(MembershipRow::NotLive(pid)),
            Observed::Verified(_, _) => match ident::process_identity(pid) {
                // The generation travels with a live row so evidence is
                // incarnation-exact rather than a bare number.
                Observed::Verified(identity, _) => rows.push(MembershipRow::Live(identity)),
                // Live at the state read, gone at the identity read: it may have
                // forked into this group first.
                Observed::Absent(_) => rows.push(MembershipRow::Unproven(pid)),
                Observed::Unknown(r) => return Err(io::Error::other(format!("{r}"))),
            },
            // A listed member may have forked before disappearing at this read.
            Observed::Absent(_) => rows.push(MembershipRow::Unproven(pid)),
            Observed::Unknown(r) => return Err(io::Error::other(format!("{r}"))),
        }
    }
    Ok(rows)
}

/// Verdict of one inventory round, given the incarnations the caller has evidence
/// for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoundVerdict {
    /// No live member other than the accounted ones, and every other row was
    /// positively resolved as not live.
    Empty,
    /// A member other than the accounted ones is positively live.
    Live,
    /// A row is ambiguous: it may hide a member this inventory cannot see.
    Unproven,
}

/// Classify one round. Pure, so the fork-race shape is testable without timing.
///
/// `accounted` holds the exact recorded incarnations the caller has evidence for
/// (an anchor it verified, or a leader it holds unreaped or has observed exit).
/// Live rows are compared by generation, so a pid recycled into a new process is a
/// member again; an unproven row is skipped only for an accounted pid, whose own
/// evidence covers that incarnation.
pub(crate) fn classify_round(
    rows: &[MembershipRow],
    accounted: &[&ProcessIdentity],
) -> RoundVerdict {
    let mut verdict = RoundVerdict::Empty;
    for row in rows {
        match row {
            MembershipRow::Live(identity) => {
                if accounted.contains(&identity) {
                    continue;
                }
                return RoundVerdict::Live;
            }
            MembershipRow::NotLive(_) => {}
            MembershipRow::Unproven(pid) => {
                if accounted.iter().any(|known| known.pid == *pid) {
                    continue;
                }
                verdict = RoundVerdict::Unproven;
            }
        }
    }
    verdict
}

pub(crate) fn live_pids(pgid: u32) -> io::Result<Vec<u32>> {
    Ok(group_membership(pgid)?
        .into_iter()
        .filter_map(|row| match row {
            MembershipRow::Live(identity) => Some(identity.pid),
            MembershipRow::NotLive(_) | MembershipRow::Unproven(_) => None,
        })
        .collect())
}
#[cfg(target_os = "linux")]
pub(crate) fn group_pids(_pgid: u32) -> io::Result<Vec<u32>> {
    let dir = crate::fs::DirFd::walk(std::path::Path::new("/proc"), crate::fs::Trust::Ancestor)
        .map_err(io::Error::other)?;
    Ok(dir
        .list(1 << 20)
        .map_err(io::Error::other)?
        .iter()
        .filter_map(|n| n.to_str()?.parse().ok())
        .collect())
}
#[cfg(target_os = "macos")]
pub(crate) fn group_pids(pgid: u32) -> io::Result<Vec<u32>> {
    let mut pids = vec![0i32; 1 << 17];
    // SAFETY: writable aligned allocation for the passed byte size, bounded
    // below i32::MAX; proc_listallpids returns a count of initialized entries.
    // Clear errno to distinguish a successful empty inventory from the
    // libproc wrapper's zero-on-error return value.
    // SAFETY: __error returns this thread's live errno slot.
    unsafe {
        *libc::__error() = 0;
    }
    let count = unsafe {
        if pgid == 0 {
            libc::proc_listallpids(
                pids.as_mut_ptr().cast(),
                (pids.len() * std::mem::size_of::<i32>()) as i32,
            )
        } else {
            libc::proc_listpgrppids(
                pgid as i32,
                pids.as_mut_ptr().cast(),
                (pids.len() * std::mem::size_of::<i32>()) as i32,
            )
        }
    };
    if count < 0 || count as usize >= pids.len() {
        return Err(io::Error::other(
            "process inventory unavailable or truncated",
        ));
    }
    if count == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(0) {
            return Err(error);
        }
    }
    pids.truncate(count as usize);
    Ok(pids
        .into_iter()
        .filter(|p| *p > 0)
        .map(|p| p as u32)
        .collect())
}
pub(super) fn read_nonce(pid: u32) -> Observed<String> {
    read_nonce_key(pid, b"EFFIGY_CUSTODY_NONCE=")
}
pub(crate) fn read_nonce_key(pid: u32, key: &[u8]) -> Observed<String> {
    match environment(pid) {
        Ok(bytes) => {
            match bytes
                .split(|b| *b == 0)
                .find_map(|entry| entry.strip_prefix(key))
            {
                Some(nonce) => match String::from_utf8(nonce.to_vec()) {
                    Ok(nonce) => Observed::Verified(nonce, evidence()),
                    Err(_) => Observed::Unknown(unknown("unparsable custody nonce")),
                },
                None => Observed::Unknown(unknown("custody nonce missing")),
            }
        }
        Err(e) => Observed::Unknown(unknown(e)),
    }
}
#[cfg(target_os = "linux")]
fn environment(pid: u32) -> io::Result<Vec<u8>> {
    use crate::fs::{DirFd, FileName, Trust};
    let proc =
        DirFd::walk(std::path::Path::new("/proc"), Trust::Ancestor).map_err(io::Error::other)?;
    let dir = proc
        .child(&pid.to_string(), Trust::Ancestor, false)
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::from_raw_os_error(libc::ESRCH))?;
    let (mut file, _) = dir
        .open_file(
            &FileName::new("environ").map_err(io::Error::other)?,
            libc::O_RDONLY,
            None,
            Trust::Ancestor,
        )
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::from_raw_os_error(libc::ESRCH))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take((1 << 20) + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1 << 20 {
        return Err(io::Error::other("environment too large"));
    }
    Ok(bytes)
}
#[cfg(target_os = "macos")]
fn environment(pid: u32) -> io::Result<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut bytes = vec![0u8; 1 << 20];
    let mut size = bytes.len();
    // SAFETY: three initialized MIB integers and a writable, initialized byte
    // allocation of size bytes. Null newp/zero newlen make this read-only.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    if size > bytes.len() || size < 4 {
        return Err(io::Error::other("invalid process environment size"));
    }
    bytes.truncate(size);
    let argc = i32::from_ne_bytes(bytes[..4].try_into().map_err(io::Error::other)?);
    if argc < 0 {
        return Err(io::Error::other("invalid process argv count"));
    }
    let mut offset = 4;
    while offset < bytes.len() && bytes[offset] != 0 {
        offset += 1;
    }
    while offset < bytes.len() && bytes[offset] == 0 {
        offset += 1;
    }
    for _ in 0..argc {
        while offset < bytes.len() && bytes[offset] != 0 {
            offset += 1;
        }
        offset = offset.saturating_add(1);
    }
    Ok(bytes
        .get(offset..)
        .ok_or_else(|| io::Error::other("truncated process environment"))?
        .to_vec())
}

// A process exiting between TERM and CONT is positive absence, not an unknown
// generation. Only Verified permits a signal; changed generations are reobserved
// through the nonce path on the next recovery turn.
pub(crate) fn generation(expected: &ProcessIdentity) -> Observed<()> {
    match ident::process_identity(expected.pid) {
        Observed::Verified(current, _) if current == *expected => {
            Observed::Verified((), evidence())
        }
        Observed::Verified(_, _) => {
            Observed::Absent(AbsenceProof::StartIdentityDiffers { pid: expected.pid })
        }
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}
