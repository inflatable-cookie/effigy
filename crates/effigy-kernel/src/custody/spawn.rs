//! The production post-fork path: async-signal-safe syscalls over prepared buffers.
use super::*;
use crate::fs::{DirFd, Trust};
use std::ffi::CString;
use std::os::fd::{FromRawFd, RawFd};
use std::sync::Mutex;
static FORK: Mutex<()> = Mutex::new(());

pub(crate) fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    // SAFETY: writable storage for exactly two fds. Ownership transfers below.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful pipe initialized two fresh, distinct owned fds.
    let pair = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    for fd in [&pair.0, &pair.1] {
        // SAFETY: live fd and a valid descriptor flag.
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(pair)
}
pub(crate) fn high(fd: OwnedFd) -> io::Result<OwnedFd> {
    // SAFETY: duplicates a live fd above the child's reserved descriptors.
    let copy = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
    if copy < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl returned a new fd, transferred once.
    Ok(unsafe { OwnedFd::from_raw_fd(copy) })
}
fn nonblock(fd: &OwnedFd) -> io::Result<()> {
    // SAFETY: live fd and supported file status flag.
    if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
/// The process groups this owner has handed the controlling terminal to. A
/// handover only displaces the owner's own group or a group already leased to a
/// foreground child, so a foreign foreground program keeps the terminal.
static LEASES: Mutex<Vec<libc::pid_t>> = Mutex::new(Vec::new());

fn leases() -> std::sync::MutexGuard<'static, Vec<libc::pid_t>> {
    LEASES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `SIGTTOU` is blocked on this thread while the terminal changes hands.
/// `tcsetpgrp` and terminal writes from a background group would otherwise stop
/// the owner: handing the terminal over is routine, never a reason to stop.
struct Ttou(libc::sigset_t);
impl Ttou {
    fn block() -> Self {
        // SAFETY: sigemptyset initializes the set the mask is built from, and
        // only this thread's previous mask is read.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGTTOU);
            let mut old: libc::sigset_t = std::mem::zeroed();
            libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
            Self(old)
        }
    }
}
impl Drop for Ttou {
    fn drop(&mut self) {
        // SAFETY: restores the mask this guard saved on the same thread.
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut()) };
    }
}

/// A group that still exists. A workload lease outlives neither the workload nor
/// its anchor, so restoration never names a reaped group.
fn group_live(pgid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only observes the group; EPERM still proves it exists.
    unsafe { libc::kill(-pgid, 0) == 0 || last_errno() == libc::EPERM }
}

#[derive(Debug)]
pub(crate) struct TerminalLease {
    fd: RawFd,
    pgid: libc::pid_t,
    owner: libc::pid_t,
}
impl TerminalLease {
    /// Hands the controlling terminal to the workload's own process group.
    ///
    /// `None` means this owner holds no terminal to give: stdin is not a
    /// terminal, or another program is the terminal's foreground group. A
    /// foreground child then runs exactly as an `Inherit` child would.
    fn take(pid: u32) -> io::Result<Option<Self>> {
        if pid == 0 || pid > i32::MAX as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid workload group",
            ));
        }
        // SAFETY: observes this process, its unreaped direct child, and the
        // borrowed terminal only.
        let (is_tty, owner) = unsafe { (libc::isatty(libc::STDIN_FILENO), libc::getpgrp()) };
        if is_tty != 1 {
            return Ok(None);
        }
        // The workload became its own group leader before exec, and the group
        // anchor joins that group; the leader is never the anchor.
        // SAFETY: getpgid only observes our own direct child.
        let pgid = unsafe { libc::getpgid(pid as i32) };
        if pgid <= 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: stdin is only borrowed, never closed by this guard.
        let previous = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
        if previous < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut leases = leases();
        if previous != owner && !leases.contains(&previous) {
            // A foreign foreground program owns the terminal. Taking it would
            // be theft rather than custody.
            return Ok(None);
        }
        let _mask = Ttou::block();
        // SAFETY: pgid is the live group of our own child in this session.
        if unsafe { libc::tcsetpgrp(libc::STDIN_FILENO, pgid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        leases.push(pgid);
        Ok(Some(Self {
            fd: libc::STDIN_FILENO,
            pgid,
            owner,
        }))
    }
}
impl Drop for TerminalLease {
    fn drop(&mut self) {
        // Restoration usually happens from the background, once the workload
        // has exited. Block TTOU on this thread, as taking does.
        let _mask = Ttou::block();
        let mut leases = leases();
        leases.retain(|pgid| *pgid != self.pgid);
        // A newer lease or the owner already holds the terminal: leave it.
        // SAFETY: only observes the borrowed terminal.
        let current = unsafe { libc::tcgetpgrp(self.fd) };
        if current != self.pgid {
            return;
        }
        // A lease we still own is the next holder, else the owner itself.
        let next = leases
            .last()
            .copied()
            .filter(|pgid| group_live(*pgid))
            .unwrap_or(self.owner);
        // SAFETY: hands the terminal to the owner's own group or to a group
        // this owner still leases to a running foreground child.
        unsafe { libc::tcsetpgrp(self.fd, next) };
    }
}
/// Whether the blocked leader puts the whole eligible workload group in a
/// session of its own, by calling `setsid` instead of `setpgid`.
///
/// The group moves together: the blocked leader creates both the process group
/// and the session before the anchor joins either, so the anchor still shares
/// the recorded PGID and no identity rule changes. Isolation is what keeps a
/// run's custody helpers out of the caller's session, which is what a nested
/// task session or a harness census of its own session would otherwise have to
/// account for as unexplained rows.
///
/// A route keeps the caller's session when it owes the caller's terminal
/// something. `Foreground` owes it the terminal itself: `tcsetpgrp` can hand a
/// terminal only to a group in the terminal's own session. `Inherit` owes it an
/// inherited terminal's foreground lease, and that lease is the same rule, so it
/// keeps the caller's session while stdin is that terminal. With stdin off a
/// terminal there is no lease to take, nothing to hand over and no reader that
/// could stop on its own stdin, so the group is eligible. Every remaining route
/// reads and writes only Effigy's own pipes, and a PTY already creates its own
/// session.
pub(crate) fn isolates_session(terminal: &Terminal, stdin_is_terminal: bool) -> bool {
    match terminal {
        Terminal::Foreground => false,
        Terminal::Inherit { .. } => !stdin_is_terminal,
        Terminal::None | Terminal::Discard | Terminal::Pipe | Terminal::Styled { .. } | Terminal::Pty => true,
    }
}

#[derive(Debug)]
pub(crate) struct Blocked {
    pub pid: u32,
    pub anchor: Option<u32>,
    /// The owner's write end of a `Terminal::Pipe` child's stdin.
    input: Option<OwnedFd>,
    barrier: Option<OwnedFd>,
    status: OwnedFd,
    streams: Option<(OwnedFd, OwnedFd, Option<TerminalLease>)>,
    released: bool,
}
impl Blocked {
    pub fn fork(
        spec: &ProcessSpec,
        nonce: &str,
        cx: &OperationContext,
        image: &crate::supervise::anchor::Image,
        closure: Option<&crate::closure::ClosurePin>,
        handoff: Option<&OwnedFd>,
    ) -> io::Result<Self> {
        Self::fork_inner(
            spec,
            "EFFIGY_CUSTODY_NONCE",
            nonce,
            cx,
            Some(image),
            closure,
            handoff,
        )
    }
    pub(crate) fn fork_with_nonce(
        spec: &ProcessSpec,
        key: &str,
        nonce: &str,
        cx: &OperationContext,
    ) -> io::Result<Self> {
        Self::fork_inner(spec, key, nonce, cx, None, None, None)
    }
    #[allow(clippy::too_many_arguments)]
    fn fork_inner(
        spec: &ProcessSpec,
        key: &str,
        nonce: &str,
        cx: &OperationContext,
        image: Option<&crate::supervise::anchor::Image>,
        closure: Option<&crate::closure::ClosurePin>,
        handoff: Option<&OwnedFd>,
    ) -> io::Result<Self> {
        if !["EFFIGY_CUSTODY_NONCE", "EFFIGY_PROBE_NONCE"].contains(&key) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid nonce key",
            ));
        }
        let argv = spec
            .argv
            .iter()
            .map(|s| CString::new(s.as_bytes()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in argv"))?;
        if argv.first().is_none_or(|a| !a.as_bytes().starts_with(b"/")) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "executable must be absolute",
            ));
        }
        let mut env = Vec::new();
        for (key, value) in &spec.env {
            if key.as_bytes().contains(&b'=')
                || [
                    b"EFFIGY_CUSTODY_NONCE".as_slice(),
                    b"EFFIGY_PROBE_NONCE".as_slice(),
                ]
                .contains(&key.as_bytes())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "reserved or invalid environment key",
                ));
            }
            let mut entry = key.as_bytes().to_vec();
            entry.push(b'=');
            entry.extend_from_slice(match value {
                EnvValue::Plain(s) => s.as_bytes(),
                EnvValue::Secret(s) => &s.0,
            });
            env.push(
                CString::new(entry).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "NUL in environment")
                })?,
            );
        }
        let closure_fd = closure
            .map(|capability| capability.custody_fd())
            .transpose()?;
        let closure_fd_raw = closure_fd.as_ref().map_or(-1, AsRawFd::as_raw_fd);
        // The parent's one-shot descriptor, handed over at its reserved slot.
        let handoff_fd = handoff.map_or(-1, AsRawFd::as_raw_fd);
        env.push(
            CString::new(format!("{key}={nonce}"))
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid nonce"))?,
        );
        let mut args: Vec<_> = argv.iter().map(|a| a.as_ptr()).collect();
        args.push(std::ptr::null());
        let mut vars: Vec<_> = env.iter().map(|a| a.as_ptr()).collect();
        vars.push(std::ptr::null());
        let anchor_args = [
            image.map_or(c"".as_ptr(), |i| i.path.as_ptr()),
            c"__kernel-anchor".as_ptr(),
            std::ptr::null(),
        ];
        let anchor_env = [
            env.last().expect("nonce present").as_ptr(),
            std::ptr::null(),
        ];
        let anchor_fd = image
            .and_then(|i| i.fd.as_ref())
            .map_or(-1, AsRawFd::as_raw_fd);
        let anchored = image.is_some();
        let cwd = DirFd::walk(&spec.cwd, Trust::Ancestor).map_err(io::Error::other)?;
        let _fork = FORK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (barrier_read, barrier_write) = pipe()?;
        let barrier_read = high(barrier_read)?;
        let (status_read, status_write) = pipe()?;
        let status_write = high(status_write)?;
        let (mut out_read, mut out_write) = pipe()?;
        out_write = high(out_write)?;
        let (err_read, err_write) = pipe()?;
        let mut err_write = high(err_write)?;
        let (input_read, input_write) = pipe()?;
        let mut input_read = high(input_read)?;
        // `Pipe` keeps the owner's write end; every other terminal closes it
        // before the fork so the child reads EOF (or its own terminal).
        let mut input_keep = None;
        match spec.terminal {
            Terminal::None | Terminal::Styled { pty: false } => drop(input_write),
            Terminal::Discard => {
                drop(input_write);
                let null = std::fs::OpenOptions::new().write(true).open("/dev/null")?;
                out_write = high(null.into())?;
                err_write = duplicate(out_write.as_raw_fd())?;
            }
            Terminal::Pipe => {
                // High, so the child's descriptor sweep closes it: a child
                // holding its own stdin write end would never see EOF.
                input_keep = Some(high(input_write)?);
            }
            Terminal::Inherit { capture } => {
                // A closed owner descriptor keeps the EOF pipe or capture pipe.
                if let Ok(fd) = duplicate(libc::STDIN_FILENO) {
                    input_read = fd;
                }
                if !capture {
                    if let Ok(fd) = duplicate(libc::STDOUT_FILENO) {
                        out_write = fd;
                    }
                    if let Ok(fd) = duplicate(libc::STDERR_FILENO) {
                        err_write = fd;
                    }
                }
            }
            Terminal::Foreground => {
                // A foreground child reads the real terminal, not an EOF pipe.
                input_read = duplicate(libc::STDIN_FILENO)?;
            }
            Terminal::Pty | Terminal::Styled { pty: true } => {
                let (master, slave) = pty()?;
                input_read = high(slave)?;
                out_write = duplicate(input_read.as_raw_fd())?;
                err_write = duplicate(input_read.as_raw_fd())?;
                out_read = master;
            }
        }
        let pty = matches!(
            spec.terminal,
            Terminal::Pty | Terminal::Styled { pty: true }
        );
        // A workload group that owns no terminal obligation gets a session of its
        // own, so no custody helper of this run lives in the caller's session.
        // A harness that censuses its own session (a nested task session, for
        // instance) then sees the run's payload group and its anchor as another
        // session rather than as members it cannot account for. The whole
        // eligible group moves: the blocked leader calls `setsid` before the
        // anchor joins it, so the anchor still shares the recorded group and
        // every identity rule below is unchanged.
        //
        // SAFETY: isatty only observes this process's own stdin.
        let stdin_is_terminal = unsafe { libc::isatty(libc::STDIN_FILENO) } == 1;
        let isolate_session = isolates_session(&spec.terminal, stdin_is_terminal);
        let fd_limit = descriptor_limit()?;
        // SAFETY: no Rust code, allocation or locks run in the child. All
        // buffers and pointers are prepared above and remain initialized and
        // immutable in its private address space until execve or _exit.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            // SAFETY: every fd below is live in the inherited descriptor table;
            // stdio/barrier sources are >=10 and mapped before the optional
            // cleanup-pin target 10. Their sources are >=128, so neither can
            // overwrite the other. After closure only explicit slots remain. In
            // particular no sibling's barrier or sentinel writer is inherited.
            unsafe {
                if libc::fchdir(cwd.raw()) != 0
                    || (if isolate_session {
                        libc::setsid()
                    } else {
                        libc::setpgid(0, 0)
                    }) < 0
                    || (pty && libc::ioctl(input_read.as_raw_fd(), libc::TIOCSCTTY as _, 0) < 0)
                {
                    libc::_exit(125);
                }
                if libc::dup2(input_read.as_raw_fd(), 0) < 0
                    || libc::dup2(out_write.as_raw_fd(), 1) < 0
                    || libc::dup2(err_write.as_raw_fd(), 2) < 0
                    || libc::dup2(barrier_read.as_raw_fd(), 3) < 0
                    || libc::dup2(status_write.as_raw_fd(), 4) < 0
                {
                    libc::_exit(125);
                }
                libc::fcntl(3, libc::F_SETFD, libc::FD_CLOEXEC);
                libc::fcntl(4, libc::F_SETFD, libc::FD_CLOEXEC);
                if anchor_fd >= 0 && libc::dup2(anchor_fd, 5) < 0 {
                    libc::_exit(125);
                }
                if closure_fd_raw >= 0 && libc::dup2(closure_fd_raw, crate::closure::CLOSURE_FD) < 0
                {
                    libc::_exit(125);
                }
                // A source that is already the target slot makes `dup2` a
                // no-op, so the descriptor keeps CLOEXEC and the payload would
                // never survive exec. Clear the flag on the slot itself.
                if handoff_fd >= 0
                    && (libc::dup2(handoff_fd, HANDOFF_FD) < 0
                        || libc::fcntl(HANDOFF_FD, libc::F_SETFD, 0) < 0)
                {
                    libc::_exit(125);
                }
                let close_from = if anchor_fd >= 0 { 6 } else { 5 };
                // Keep only the write-only cleanup pin and the parent's
                // one-shot handoff in slots 10/11. Sibling barriers always
                // close; the group anchor closes these workload slots too.
                #[cfg(target_os = "linux")]
                let closed = {
                    let mut ok =
                        libc::syscall(libc::SYS_close_range, close_from as u32, 9u32, 0u32) == 0;
                    if ok && closure_fd_raw < 0 {
                        ok = libc::syscall(libc::SYS_close_range, 10u32, 10u32, 0u32) == 0;
                    }
                    ok && libc::syscall(
                        libc::SYS_close_range,
                        if handoff_fd >= 0 { 12u32 } else { 11u32 },
                        u32::MAX,
                        0u32,
                    ) == 0
                };
                #[cfg(target_os = "macos")]
                let closed = false;
                if !closed {
                    let mut fd = close_from;
                    while fd < fd_limit {
                        if !(closure_fd_raw >= 0 && fd == crate::closure::CLOSURE_FD
                            || handoff_fd >= 0 && fd == HANDOFF_FD)
                        {
                            libc::close(fd);
                        }
                        fd += 1;
                    }
                }
                for signal in [
                    libc::SIGHUP,
                    libc::SIGINT,
                    libc::SIGTERM,
                    libc::SIGPIPE,
                    libc::SIGTTOU,
                ] {
                    libc::signal(signal, libc::SIG_DFL);
                }
                let mut mask: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut mask);
                libc::sigprocmask(libc::SIG_SETMASK, &mask, std::ptr::null_mut());
                // Fork inside the blocked leader's session, including PTY
                // setsid. Its group already exists; neither parent setpgid nor
                // a join across sessions can race workload execution. An
                // isolated group is its own session, and the anchor joins the
                // group inside it the same way.
                let mut anchor_pid = [0u8; 4];
                let mut control = [-1; 2];
                if anchored {
                    let mut ready = [-1; 2];
                    if libc::pipe(control.as_mut_ptr()) != 0 || libc::pipe(ready.as_mut_ptr()) != 0
                    {
                        libc::_exit(125);
                    }
                    let middle = libc::fork();
                    if middle < 0 {
                        libc::_exit(125);
                    }
                    if middle == 0 {
                        let leaf = libc::fork();
                        if leaf < 0 {
                            libc::_exit(125);
                        }
                        if leaf > 0 {
                            libc::_exit(0);
                        }
                        // Reparent before workload exec: waitpid(-1) in the
                        // workload must never see the kernel's anchor child.
                        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
                        if null < 0
                            || libc::dup2(null, 0) < 0
                            || libc::dup2(null, 1) < 0
                            || libc::dup2(null, 2) < 0
                            || libc::dup2(control[0], 3) < 0
                            || libc::dup2(ready[1], 4) < 0
                        {
                            libc::_exit(125);
                        }
                        // The blocked leader already closed every older fd.
                        // These pipes and /dev/null are its only new ones.
                        let end = null.max(control[1]).max(ready[1]);
                        let mut fd = close_from;
                        while fd <= end {
                            libc::close(fd);
                            fd += 1;
                        }
                        if handoff_fd >= 0 {
                            // The anchor is not the reader: only the workload
                            // keeps the parent's handoff.
                            libc::close(HANDOFF_FD);
                        }
                        #[cfg(target_os = "linux")]
                        if anchor_fd >= 0 {
                            libc::syscall(
                                libc::SYS_execveat,
                                5,
                                c"".as_ptr(),
                                anchor_args.as_ptr(),
                                anchor_env.as_ptr(),
                                libc::AT_EMPTY_PATH,
                            );
                            libc::_exit(125);
                        }
                        libc::execve(anchor_args[0], anchor_args.as_ptr(), anchor_env.as_ptr());
                        libc::_exit(125);
                    }
                    if anchor_fd >= 0 {
                        libc::close(5);
                    }
                    libc::close(control[0]);
                    libc::close(ready[1]);
                    let mut status = 0;
                    loop {
                        if libc::waitpid(middle, &mut status, 0) >= 0 {
                            break;
                        }
                        if last_errno() != libc::EINTR {
                            libc::_exit(125);
                        }
                    }
                    if status != 0 {
                        libc::_exit(125);
                    }
                    let mut offset = 0;
                    while offset < anchor_pid.len() {
                        let n = libc::read(
                            ready[0],
                            anchor_pid[offset..].as_mut_ptr().cast(),
                            anchor_pid.len() - offset,
                        );
                        if n > 0 {
                            offset += n as usize;
                        } else if n == 0 || last_errno() != libc::EINTR {
                            libc::_exit(125);
                        }
                    }
                    libc::close(ready[0]);
                }
                let ready = [1u8];
                if libc::write(4, ready.as_ptr().cast(), 1) != 1 {
                    libc::_exit(125);
                }
                if anchored
                    && libc::write(4, anchor_pid.as_ptr().cast(), anchor_pid.len())
                        != anchor_pid.len() as isize
                {
                    libc::_exit(125);
                }
                let mut go = [0u8];
                loop {
                    let n = libc::read(3, go.as_mut_ptr().cast(), 1);
                    if n == 1 {
                        break;
                    }
                    if n == 0 {
                        libc::_exit(125);
                    }
                    if last_errno() != libc::EINTR {
                        libc::_exit(125);
                    }
                }
                libc::close(3);
                if go[0] != 1 {
                    libc::_exit(125);
                }
                if anchored {
                    let release = [1u8];
                    if libc::write(control[1], release.as_ptr().cast(), release.len())
                        != release.len() as isize
                    {
                        libc::_exit(125);
                    }
                    libc::close(control[1]);
                }
                libc::execve(argv[0].as_ptr(), args.as_ptr(), vars.as_ptr());
                let failed = [2u8];
                libc::write(4, failed.as_ptr().cast(), 1);
                libc::_exit(127);
            }
        }
        drop(barrier_read);
        drop(status_write);
        drop(out_write);
        drop(err_write);
        drop(input_read);
        let mut child = Self {
            pid: pid as u32,
            anchor: None,
            input: input_keep,
            barrier: Some(barrier_write),
            status: status_read,
            streams: Some((out_read, err_read, None)),
            released: false,
        };
        if let Some(input) = &child.input {
            nonblock(input)?;
        }
        nonblock(&child.status)?;
        if let Some((stdout, stderr, _)) = &child.streams {
            nonblock(stdout)?;
            nonblock(stderr)?;
        }
        child.await_status(cx, false)?;
        if anchored {
            let mut bytes = [0u8; 4];
            let mut offset = 0;
            while offset < bytes.len() {
                cx.check("anchor identity frame")
                    .map_err(|e| io::Error::other(format!("{e}")))?;
                // SAFETY: nonblocking live status pipe and remaining buffer.
                let n = unsafe {
                    libc::read(
                        child.status.as_raw_fd(),
                        bytes[offset..].as_mut_ptr().cast(),
                        bytes.len() - offset,
                    )
                };
                if n > 0 {
                    offset += n as usize;
                } else if n == 0 {
                    return Err(io::Error::other("anchor frame EOF"));
                } else if !matches!(
                    io::Error::last_os_error().kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    return Err(io::Error::last_os_error());
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            child.anchor = Some(u32::from_ne_bytes(bytes));
            #[cfg(all(test, feature = "fault-injection"))]
            crate::fault::note_anchor(u32::from_ne_bytes(bytes));
        }
        if matches!(spec.terminal, Terminal::Foreground) {
            child.streams.as_mut().expect("streams present").2 = TerminalLease::take(pid as u32)?;
        }
        if matches!(spec.terminal, Terminal::Inherit { .. }) {
            // Best effort: without the terminal an interactive child stops on
            // its first read, but a non-interactive one is unaffected. A
            // foreign foreground program or a non-terminal stdin takes no lease.
            child.streams.as_mut().expect("streams present").2 =
                TerminalLease::take(pid as u32).ok().flatten();
        }
        Ok(child)
    }
    fn await_status(&mut self, cx: &OperationContext, exec: bool) -> io::Result<()> {
        loop {
            cx.check("spawn barrier")
                .map_err(|e| io::Error::other(format!("{e}")))?;
            let mut byte = [0u8];
            // SAFETY: live nonblocking pipe and initialized writable byte.
            let count = unsafe { libc::read(self.status.as_raw_fd(), byte.as_mut_ptr().cast(), 1) };
            if count == 0 {
                return if exec {
                    Ok(())
                } else {
                    Err(io::Error::other("child failed before barrier"))
                };
            }
            if count == 1 {
                return if !exec && byte[0] == 1 {
                    Ok(())
                } else {
                    Err(io::Error::other("exec failed"))
                };
            }
            let error = io::Error::last_os_error();
            if !matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                return Err(error);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pub fn transfer(&mut self) {
        self.released = true;
    }
    pub fn executed(&mut self, cx: &OperationContext) -> io::Result<()> {
        self.await_status(cx, true)
    }
    pub fn take_streams(&mut self) -> (OwnedFd, OwnedFd, Option<TerminalLease>) {
        self.streams.take().expect("streams transferred once")
    }
    pub fn take_input(&mut self) -> Option<OwnedFd> {
        self.input.take()
    }
    pub fn go(&mut self) -> io::Result<()> {
        let mut pipe = std::fs::File::from(self.barrier.take().expect("barrier released once"));
        pipe.write_all(&[1])?;
        self.released = true;
        Ok(())
    }
}
impl Drop for Blocked {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.barrier.take();
        // The child is still behind the barrier: it cannot have execed,
        // changed credentials, or reused its pid, and its group holds only
        // this child and the anchor it forked before acknowledging readiness.
        // Take the whole group, so a refused spawn leaves no live member and
        // its recorded receipt can settle as released rather than residue.
        // SAFETY: positive unreaped direct child; its group is its own pid.
        unsafe { libc::kill(-(self.pid as i32), libc::SIGKILL) };
        // SAFETY: this is our unreaped direct child, still behind the barrier.
        unsafe { libc::kill(self.pid as i32, libc::SIGKILL) };
        // SAFETY: SIGKILL of a pre-exec direct child guarantees termination.
        unsafe { libc::waitpid(self.pid as i32, std::ptr::null_mut(), 0) };
        // A refused spawn is settled by the run's receipt, which needs an
        // empty group. The anchor exits on its own when its control pipe
        // closes; bound the wait and leave any survivor to the settle's
        // residue path rather than claiming closure here.
        let deadline = Instant::now() + Duration::from_secs(1);
        while !matches!(
            super::platform::group_closed(self.pid, &[]),
            Observed::Verified(..) | Observed::Absent(_)
        ) && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
pub(crate) fn peek(pid: u32) -> io::Result<Option<Exit>> {
    // SAFETY: zeroed storage is valid for siginfo_t; waitid initializes it.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    // SAFETY: positive direct child pid; WNOWAIT deliberately retains it.
    if unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful waitid initializes the union, including pid/status.
    if unsafe { info.si_pid() } == 0 {
        return Ok(None);
    }
    // SAFETY: si_status belongs to the initialized child-exit variant.
    let status = unsafe { info.si_status() };
    Ok(Some(Exit {
        code: if info.si_code == libc::CLD_EXITED {
            status
        } else {
            128 + status
        },
    }))
}
pub(crate) fn reap(pid: u32) -> io::Result<Option<Exit>> {
    let mut status = 0;
    // SAFETY: direct child pid and initialized writable status.
    let result = unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if result == 0 {
        return Ok(None);
    }
    Ok(Some(Exit {
        code: if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else {
            128 + libc::WTERMSIG(status)
        },
    }))
}

fn last_errno() -> i32 {
    #[cfg(target_os = "linux")]
    // SAFETY: libc provides a live thread-local errno slot.
    unsafe {
        *libc::__errno_location()
    }
    #[cfg(target_os = "macos")]
    // SAFETY: libc provides a live thread-local errno slot.
    unsafe {
        *libc::__error()
    }
}

pub(crate) fn descriptor_limit() -> io::Result<i32> {
    // SAFETY: sysconf only observes the calling process's descriptor limit.
    let limit = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
    if !(5..=1 << 20).contains(&limit) {
        return Err(io::Error::other("descriptor table cannot be bounded"));
    }
    #[cfg(target_os = "linux")]
    let limit = limit as i32;
    #[cfg(target_os = "macos")]
    let limit = {
        let mut limit = limit as i32;
        // Include descriptors that existed before a caller lowered RLIMIT_NOFILE.
        // Future allocations are below the current limit; this snapshot covers
        // the older high descriptors. No allocator is used in the fork child.
        let mut fds = vec![std::mem::MaybeUninit::<libc::proc_fdinfo>::uninit(); 1 << 16];
        let size = fds.len() * std::mem::size_of::<libc::proc_fdinfo>();
        // SAFETY: writable aligned allocation of size bytes, current pid, and
        // PROC_PIDLISTFDS's documented output layout. Only full entries are read.
        let count = unsafe {
            libc::proc_pidinfo(
                std::process::id() as i32,
                libc::PROC_PIDLISTFDS,
                0,
                fds.as_mut_ptr().cast(),
                size as i32,
            )
        };
        if count <= 0
            || count as usize >= size
            || !(count as usize).is_multiple_of(std::mem::size_of::<libc::proc_fdinfo>())
        {
            return Err(io::Error::other("descriptor inventory unavailable"));
        }
        for item in fds
            .iter()
            .take(count as usize / std::mem::size_of::<libc::proc_fdinfo>())
        {
            // SAFETY: proc_pidinfo initialized this full entry above.
            let fd = unsafe { item.assume_init_ref() }.proc_fd;
            if !(0..1 << 20).contains(&fd) {
                return Err(io::Error::other("descriptor inventory exceeds bound"));
            }
            limit = limit.max(fd + 1);
        }
        limit
    };
    Ok(limit)
}

fn duplicate(fd: RawFd) -> io::Result<OwnedFd> {
    // SAFETY: borrows the live source; the returned descriptor has sole ownership.
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) };
    if copy < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl returned a newly owned descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(copy) })
}
fn pty() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: two writable descriptor slots; null settings request OS defaults.
    if unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful openpty returns two fresh, distinct descriptors.
    let pair = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    Ok((high(pair.0)?, high(pair.1)?))
}
