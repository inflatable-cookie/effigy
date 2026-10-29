//! Same-image group anchor. It announces its ready PID before go, and exits on
//! SIGTERM or when settlement retires it.
use super::*;
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
/// custody nonce in its environment. Its image is the run's own executable,
/// and an image that carries a file capability (the installed `effigy` carries
/// `cap_net_bind_service` for the low-port gateway) is a secure exec: the
/// kernel clears the dumpable attribute, which makes `/proc/<pid>/environ`
/// refuse the operator's own reads. Custody then cannot verify the anchor it
/// holds, refuses to signal it, and the group never closes (effigy#476).
///
/// The anchor runs no workload code, inherits no workload stream and holds no
/// secret beyond the nonce the contract requires it to expose, so restoring the
/// readable state gives nothing away and keeps the identity proof exactly as
/// strict as it was: the recorded generation, group and nonce are all still
/// required before any signal.
#[cfg(target_os = "linux")]
fn restore_evidence_readability() {
    // SAFETY: PR_GET_DUMPABLE and PR_SET_DUMPABLE take one integer argument and
    // only read or set this process's own dumpable attribute.
    let state = unsafe { libc::prctl(libc::PR_GET_DUMPABLE) };
    if state != 0 {
        // 1 is the ordinary state, and a failed query is not proof that the
        // environment is unreadable, so neither one changes anything.
        return;
    }
    // SAFETY: as above; 1 selects the ordinary dumpable state.
    // A refused request leaves the anchor exactly as it was: custody's identity
    // requirements do not move, and settlement stays fail-closed.
    unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 1) };
}

pub(super) fn entry() -> Result<(), RunError> {
    if std::env::var("EFFIGY_CUSTODY_NONCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err(RunError::broken("anchor nonce missing"));
    }
    #[cfg(target_os = "linux")]
    restore_evidence_readability();
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
    loop {
        // SAFETY: pause has no arguments. INT and HUP stay ignored, and TERM's
        // default action ends the process; no fds remain either way.
        unsafe {
            libc::pause();
        }
    }
}
