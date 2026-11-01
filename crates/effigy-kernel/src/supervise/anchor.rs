//! Same-image group anchor. It announces its ready PID before go, and exits on
//! SIGTERM, when settlement retires it, or once its group has no live member.
use super::*;
use crate::custody::platform;
use crate::ident::ProcessState;
use std::ffi::CString;
use std::os::fd::OwnedFd;

pub(crate) struct Image {
    pub(crate) path: CString,
    pub(crate) fd: Option<OwnedFd>,
}
impl Image {
    pub(crate) fn prepare(executable: &Executable) -> Result<Self, RunError> {
        #[cfg(target_os = "linux")]
        let fd = Some(crate::custody::spawn::high(
            std::fs::File::open(&executable.path)?.into(),
        )?);
        #[cfg(target_os = "macos")]
        let fd = None;
        let path =
            CString::new(executable.path.as_os_str().as_bytes()).map_err(io::Error::other)?;
        Ok(Self { path, fd })
    }
}

/// The anchor's whole substance is the evidence custody reads about it: its
/// recorded `{pid, boot, start}` generation, its group membership, and the
/// custody nonce in its environment. Its image is the run's own executable, so
/// when that image carries a file capability (the installed `effigy` carries
/// `cap_net_bind_service` for the low-port gateway) the anchor inherits it and
/// two independent kernel rules then hide `/proc/<pid>/environ` from the
/// unprivileged run owner:
///
/// * the exec is a secure exec, so the kernel clears the dumpable attribute;
/// * `cap_ptrace_access_check` grants a read only when the reader's permitted
///   capabilities cover the target's, and the owner holds none.
///
/// Custody reads that file for the nonce, so it could not verify the anchor it
/// holds, refused to signal it, and the group never closed (effigy#476). This
/// was the whole native failure: leaders gone, anchors live and unverifiable.
///
/// The anchor runs no workload code, inherits no workload stream and needs no
/// capability of its own, so it drops every capability and restores the
/// ordinary dumpable state before it announces its pid. Both are verified, and
/// the identity proof is exactly as strict as before: the recorded generation,
/// group membership and nonce are still required before any signal. Nothing
/// else is given up: a workload keeps its own capabilities and the gateway can
/// still bind its low ports.
#[cfg(target_os = "linux")]
fn make_evidence_readable() -> Result<(), RunError> {
    drop_capabilities()?;
    restore_dumpable()?;
    Ok(())
}

/// Drop this process's effective, permitted and inheritable capability sets,
/// which also empties the ambient set: an ambient capability must be in both
/// the permitted and inheritable sets, so removing it from either removes it
/// from ambient too. The bounding set needs no change — it only limits future
/// execs, and the anchor execs nothing — but a caller that starts with a wider
/// permitted set is handled by the same empty `capset`.
/// The kernel's own capability ABI (`linux/capability.h`): the header names the
/// version and target pid, and version 3 writes the 64-bit sets as two words.
/// `libc` does not expose these on every target Rust builds for, so the layout
/// is stated here, unchanged since Linux 2.6.26.
#[cfg(target_os = "linux")]
#[repr(C)]
struct CapabilityHeader {
    version: u32,
    pid: i32,
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CapabilityData {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

/// `_LINUX_CAPABILITY_VERSION_3`, the 64-bit-set version.
#[cfg(target_os = "linux")]
const CAPABILITY_VERSION_3: u32 = 0x2008_0522;

#[cfg(target_os = "linux")]
fn capability_sets() -> [CapabilityData; 2] {
    [
        CapabilityData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
        CapabilityData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
    ]
}

/// Drop this process's effective, permitted and inheritable capability sets,
/// which also empties the ambient set: an ambient capability must be in both
/// the permitted and inheritable sets, so removing it from either removes it
/// from ambient too. The bounding set needs no change — it only limits future
/// execs, and the anchor execs nothing — but a caller that starts with a wider
/// permitted set is handled by the same empty `capset`.
#[cfg(target_os = "linux")]
fn drop_capabilities() -> Result<(), RunError> {
    // pid 0 is this process; version 3 covers the 64-bit sets in two words.
    let mut header = CapabilityHeader {
        version: CAPABILITY_VERSION_3,
        pid: 0,
    };
    let mut current = capability_sets();
    // SAFETY: the header names this process and the kernel ABI version, and
    // `current` provides the two writable words capget fills in.
    if unsafe { libc::syscall(libc::SYS_capget, &mut header, current.as_mut_ptr()) } != 0 {
        return Err(RunError::broken(
            "the custody anchor cannot read its own capabilities",
        ));
    }
    let dropping = capability_sets();
    // SAFETY: as above; capset replaces only this process's own sets.
    if unsafe { libc::syscall(libc::SYS_capset, &header, dropping.as_ptr()) } != 0 {
        return Err(RunError::broken(
            "the custody anchor cannot drop its capabilities",
        ));
    }
    let mut remaining = capability_sets();
    // SAFETY: as above; the read-back proves the drop took effect.
    if unsafe { libc::syscall(libc::SYS_capget, &mut header, remaining.as_mut_ptr()) } != 0 {
        return Err(RunError::broken(
            "the custody anchor cannot confirm its capability drop",
        ));
    }
    if remaining
        .iter()
        .any(|set| set.effective != 0 || set.permitted != 0 || set.inheritable != 0)
    {
        // Without an empty set the owner may still be refused `/proc` reads, so
        // the anchor would be evidence nobody can check. Refuse to be an anchor
        // rather than let a settle time out over it.
        return Err(RunError::broken(
            "the custody anchor kept a capability and cannot prove its evidence",
        ));
    }
    Ok(())
}

/// Restore the ordinary dumpable state the secure exec cleared, and prove it.
#[cfg(target_os = "linux")]
fn restore_dumpable() -> Result<(), RunError> {
    // SAFETY: PR_GET_DUMPABLE and PR_SET_DUMPABLE take one integer argument and
    // only read or set this process's own dumpable attribute.
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 1) } != 0 {
        return Err(RunError::broken(
            "the custody anchor cannot restore its readable identity evidence",
        ));
    }
    // SAFETY: as above; the read-back proves the state the owner's read needs.
    if unsafe { libc::prctl(libc::PR_GET_DUMPABLE) } != 1 {
        return Err(RunError::broken(
            "the custody anchor did not become readable identity evidence",
        ));
    }
    Ok(())
}

pub(super) fn entry() -> Result<(), RunError> {
    if std::env::var("EFFIGY_CUSTODY_NONCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err(RunError::broken("anchor nonce missing"));
    }
    #[cfg(target_os = "linux")]
    make_evidence_readable()?;
    // SAFETY: supported signals and SIG_IGN/SIG_DFL; this single-threaded helper
    // never forwards or handles workload signals. INT and HUP stay ignored
    // because a terminal delivers them to the whole foreground group, where the
    // anchor is only the group's evidence. TERM is the cooperative exit, so its
    // default action is restored even when the binary installed a signal router
    // before dispatching here; settlement still stops it last.
    unsafe {
        for signal in [libc::SIGHUP, libc::SIGINT] {
            libc::signal(signal, libc::SIG_IGN);
        }
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        // A blocked TERM inherited across exec would otherwise survive the
        // default action, so the anchor unblocks it here.
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
        // spawn::Blocked supplied only 0..=4 (plus Linux image fd 5).
        // The main-only helper dispatch and signal router open no descriptors.
        for fd in [0, 1, 2, 5] {
            libc::close(fd);
        }
    }
    let mut go = std::fs::File::from(private_dup(3)?);
    let mut ready = std::fs::File::from(private_dup(4)?);
    // SAFETY: the protocol's original descriptors are no longer needed; only
    // the two validated, freshly owned duplicates above remain in use.
    unsafe {
        libc::close(3);
        libc::close(4);
    }
    ready.write_all(&std::process::id().to_ne_bytes())?;
    drop(ready);
    let mut byte = [0u8];
    go.read_exact(&mut byte)?; // pre-go owner death closes this pipe
    drop(go);
    if byte != [1] {
        return Err(RunError::broken("invalid anchor go"));
    }
    // SAFETY: getpgrp only observes this process's own process group, which the
    // blocked leader created before the anchor joined it.
    let group = unsafe { libc::getpgrp() };
    if group <= 0 {
        return Err(RunError::broken("the anchor cannot read its own group"));
    }
    let group = group as u32;
    let me = std::process::id();
    let mut wait = POLL_MIN;
    loop {
        // A live leader pins the group, so no inventory is needed while it is
        // there; only an emptied leader turns this into a closure question. A
        // signal still ends the process: INT and HUP stay ignored, TERM keeps
        // its default action, and settlement may still retire the anchor last.
        if !leader_live(group) && observe_group(group, me) == ClosureRound::Empty {
            // No live member remains, so the anchor is no longer anyone's
            // evidence. It retires itself here instead of waiting to be killed:
            // that is what keeps no anchor beyond its group, including after its
            // owner, its checkout or its volume is gone. A round with an unproven
            // row or without the anchor's own positive row keeps waiting instead,
            // so an anchor that cannot establish closure stays alive and is
            // reported as `unproven group closure` (effigy#520).
            return Ok(());
        }
        std::thread::sleep(wait);
        wait = wait.saturating_mul(2).min(POLL_MAX);
    }
}

/// How long the anchor waits between group observations, and the ceiling that
/// wait grows to. Both phases stay bounded, so an anchor whose group closes is
/// gone within [`POLL_MAX`] of that closure whatever happened to its owner.
const POLL_MIN: Duration = Duration::from_millis(20);
const POLL_MAX: Duration = Duration::from_millis(250);

/// Whether the group's leader, which the blocked leader became before the anchor
/// joined it, is still a live process. One observation, never a signal.
fn leader_live(group: u32) -> bool {
    matches!(
        ident::process_state(group),
        Observed::Verified(ProcessState::Active | ProcessState::Stopped, _)
    )
}

/// Outcome of one best-effort closure round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClosureRound {
    /// Every listed member other than the anchor was positively not live, and the
    /// anchor's own row was positively observed. The inventory is not atomic,
    /// so an unlisted descendant may still have been missed.
    Empty,
    /// A member other than the anchor is positively live.
    Live,
    /// A row could not be resolved: it was live and then vanished, or the kernel
    /// lists it in the group with no readable identity. It may hide a member this
    /// inventory cannot see, so nothing is proved.
    Unproven,
    /// An observation failed, or the anchor's own row was not positively seen.
    Unknown,
}

/// Classify one inventory round. Pure, so the shape is testable without timing.
///
/// Only positively resolved rows are evidence: a zombie (`NotLive`) cannot fork,
/// while an unreadable row is [`ClosureRound::Unproven`] and keeps the anchor
/// alive. The anchor's own positive row keeps an empty inventory from satisfying
/// this vacuously (effigy#520).
pub(crate) fn closure_round(rows: &[platform::MembershipRow], me: u32) -> ClosureRound {
    let mut anchor_live = false;
    let mut unproven = false;
    for row in rows {
        match row {
            platform::MembershipRow::Live(identity) if identity.pid == me => anchor_live = true,
            platform::MembershipRow::Live(_) => return ClosureRound::Live,
            platform::MembershipRow::NotLive(_) => {}
            platform::MembershipRow::Unproven(_) => unproven = true,
        }
    }
    if !anchor_live {
        return ClosureRound::Unknown;
    }
    if unproven {
        return ClosureRound::Unproven;
    }
    ClosureRound::Empty
}

/// This round's outcome against the live group.
///
/// Best effort, never a proof: an inventory is not atomic with process creation,
/// so a descendant forked and orphaned inside the window can be missed. Only a
/// round with no live or unproven member and a positively observed anchor row
/// retires the anchor. An unproven row or failed read keeps it alive as evidence.
fn observe_group(group: u32, me: u32) -> ClosureRound {
    match platform::group_membership(group) {
        Ok(rows) => closure_round(&rows, me),
        Err(_) => ClosureRound::Unknown,
    }
}
