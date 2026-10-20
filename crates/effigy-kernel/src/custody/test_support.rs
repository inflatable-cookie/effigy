//! Test-only native Child adapter. Production effects still require journalled
//! Custody::spawn; fixtures reuse its generation observations and recovery.
use super::{platform, Closure, ProcessReceipt};
use crate::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, Limits, OperationContext, Placement,
};
use crate::ident;
use crate::observe::Observed;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub fn spawn(command: &mut Command, nonce: &str) -> io::Result<(Child, ProcessReceipt)> {
    spawn_registered(command, nonce, register)
}

fn spawn_registered(
    command: &mut Command,
    nonce: &str,
    registration: impl FnOnce(u32, &str) -> io::Result<ProcessReceipt>,
) -> io::Result<(Child, ProcessReceipt)> {
    command.process_group(0).env("EFFIGY_CUSTODY_NONCE", nonce);
    let mut child = command.spawn()?;
    match registration(child.id(), nonce) {
        Ok(receipt) => Ok((child, receipt)),
        Err(error) => {
            // The unreaped direct child pins this PID; partial startup rolls back.
            let _ = platform::numeric(child.id(), true, libc::SIGKILL);
            let _ = child.wait();
            Err(error)
        }
    }
}

/// A fresh fixture's launch receipt supplies the PID; observe its generation
/// before retaining authority. Only a dedicated group leader can be adopted.
pub fn register(pid: u32, nonce: &str) -> io::Result<ProcessReceipt> {
    let identity = match ident::process_identity(pid) {
        Observed::Verified(identity, _) => identity,
        other => {
            return Err(io::Error::other(format!(
                "fixture process identity: {other:?}"
            )))
        }
    };
    // SAFETY: process_identity range-checks PID; getpgid only observes it.
    let group = unsafe { libc::getpgid(pid as i32) };
    if group < 0 {
        return Err(io::Error::last_os_error());
    }
    if group as u32 != pid {
        return Err(io::Error::other("fixture process must lead its own group"));
    }
    let nonce = if nonce.is_empty() {
        platform::read_nonce(pid)
            .verified()
            .cloned()
            .unwrap_or_default()
    } else {
        nonce.into()
    };
    Ok(ProcessReceipt {
        identity,
        pgid: pid,
        nonce,
        grace_millis: 250,
    })
}

/// Observe a direct child's exit without releasing its PID/group generation.
pub fn poll(child: &Child) -> io::Result<Option<std::process::ExitStatus>> {
    use std::os::unix::process::ExitStatusExt;
    // SAFETY: zero is a valid initial siginfo; waitid initializes its fields.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    // SAFETY: valid writable siginfo, positive owned Child PID and wait flags.
    if unsafe {
        libc::waitid(
            libc::P_PID,
            child.id(),
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    #[cfg(target_os = "linux")]
    // SAFETY: successful waitid initialized the child-status siginfo variant.
    let (pid, status) = unsafe { (info.si_pid(), info.si_status()) };
    #[cfg(target_os = "macos")]
    let (pid, status) = (info.si_pid, info.si_status);
    if pid == 0 {
        return Ok(None);
    }
    let raw = if info.si_code == libc::CLD_EXITED {
        status << 8
    } else {
        status
            | if info.si_code == libc::CLD_DUMPED {
                0x80
            } else {
                0
            }
    };
    Ok(Some(std::process::ExitStatus::from_raw(raw)))
}

/// Retain the direct leader until the group is closed, including when it is a
/// zombie. This avoids depending on readable argv of protected macOS children.
pub fn stop_child(child: &mut Child, receipt: &ProcessReceipt) -> io::Result<()> {
    if child.id() != receipt.identity.pid || receipt.pgid != child.id() {
        return Err(io::Error::other("invalid direct fixture receipt"));
    }
    let exited = poll(child)?.is_some(); // ECHILD refuses a released direct-child handle.
                                         // macOS may stop exposing the start identity of a zombie. WNOWAIT still
                                         // proves the owned child is unreaped, so its original group remains pinned.
    if !exited
        && !matches!(
            platform::generation(&receipt.identity),
            Observed::Verified(..)
        )
    {
        return Err(io::Error::other("direct fixture generation changed"));
    }
    let cleanup = cleanup_context()?;
    let grace = Instant::now() + Duration::from_millis(receipt.grace_millis);
    loop {
        if matches!(
            closed(receipt),
            Observed::Verified(..) | Observed::Absent(_)
        ) {
            return Ok(());
        }
        cleanup
            .operation()
            .check("fixture group closure")
            .map_err(|error| io::Error::other(format!("fixture group closure: {error:?}")))?;
        let signal = if Instant::now() < grace {
            libc::SIGTERM
        } else {
            libc::SIGKILL
        };
        // The unreaped Child pins its PID and group throughout this loop.
        if let Observed::Unknown(reason) = platform::owner_signal(receipt, signal) {
            return Err(io::Error::other(format!(
                "fixture group signal: {reason:?}"
            )));
        }
        let _ = platform::owner_signal(receipt, libc::SIGCONT);
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn closed(receipt: &ProcessReceipt) -> Observed<Closure> {
    platform::group_closed(receipt.pgid, None)
}

/// Same bounded, generation-checked recovery as journalled custody, including
/// nonce-evidenced descendants after their leader was reaped.
pub fn stop(receipt: &ProcessReceipt) -> io::Result<()> {
    if receipt.identity.pid != receipt.pgid {
        return Err(io::Error::other("invalid fixture group receipt"));
    }
    let cleanup = cleanup_context()?;
    match platform::recover(receipt, &cleanup, &mut || false) {
        Observed::Verified(..) | Observed::Absent(_) => Ok(()),
        Observed::Unknown(reason) => Err(io::Error::other(format!(
            "fixture process residue: {reason:?}"
        ))),
    }
}

fn cleanup_context() -> io::Result<CleanupContext> {
    let operation = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(15)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 1024,
            log_keep: 1,
            line_max: 1024,
            queue_bytes: 1024,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        Placement::Host,
    );
    operation
        .cleanup(Duration::from_secs(15))
        .map_err(|_| io::Error::other("fixture cleanup budget overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Proof(Child, ProcessReceipt);
    impl Drop for Proof {
        fn drop(&mut self) {
            let _ = stop_child(&mut self.0, &self.1);
            let _ = self.0.wait();
        }
    }
    fn proof(command: &mut Command, nonce: &str) -> Proof {
        let (child, receipt) = spawn(command, nonce).unwrap();
        Proof(child, receipt)
    }

    #[test]
    fn registration_failure_rolls_back_the_owned_child() {
        let mut pid = 0;
        let error = spawn_registered(
            Command::new("sleep").arg("60"),
            "fixture-rollback",
            |child, _| {
                pid = child;
                Err(io::Error::other("injected registration failure"))
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("injected registration failure"));
        assert!(matches!(ident::process_identity(pid), Observed::Absent(_)));
    }

    #[test]
    fn direct_child_pins_group_until_background_members_are_closed() {
        let mut child = proof(
            Command::new("sh").args(["-c", "sleep 60 &"]),
            "fixture-kernel-background-proof",
        );
        let receipt = &child.1;
        let deadline = Instant::now() + Duration::from_secs(15);
        while poll(&child.0).unwrap().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!matches!(
            closed(receipt),
            Observed::Verified(..) | Observed::Absent(_)
        ));
        stop_child(&mut child.0, receipt).unwrap();
        assert!(matches!(
            closed(receipt),
            Observed::Verified(..) | Observed::Absent(_)
        ));
    }

    #[test]
    fn inconsistent_receipt_does_not_signal_foreign_group() {
        let mut child = proof(Command::new("sleep").arg("60"), "fixture-foreign-proof");
        let mut other = proof(Command::new("sleep").arg("60"), "fixture-stale-proof");
        let mut stale = other.1.clone();
        stale.identity = child.1.identity.clone();
        assert!(stop(&stale).is_err());
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(other.0.try_wait().unwrap().is_none());
    }
}
