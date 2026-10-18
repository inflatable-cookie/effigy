//! The only post-fork path: async-signal-safe syscalls over prepared buffers.
use super::*;
use crate::fs::{DirFd, Trust};
use std::ffi::CString;
use std::os::fd::{FromRawFd, RawFd};
use std::sync::Mutex;
static FORK: Mutex<()> = Mutex::new(());

pub(super) fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
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
fn high(fd: OwnedFd) -> io::Result<OwnedFd> {
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
#[derive(Debug)]
pub(super) struct TerminalLease {
    fd: RawFd,
    previous: libc::pid_t,
}
impl TerminalLease {
    fn take(pgid: u32) -> io::Result<Self> {
        // SAFETY: stdin is only borrowed, never closed by this guard.
        let previous = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
        if previous < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: positive group of a direct child in the current session.
        if unsafe { libc::tcsetpgrp(libc::STDIN_FILENO, pgid as i32) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            fd: libc::STDIN_FILENO,
            previous,
        })
    }
}
impl Drop for TerminalLease {
    fn drop(&mut self) {
        // SAFETY: restores the group saved by tcgetpgrp on our borrowed terminal.
        // Block TTOU on this thread only, so restoration from the background works.
        unsafe {
            let mut mask: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut mask);
            libc::sigaddset(&mut mask, libc::SIGTTOU);
            let mut old = std::mem::zeroed();
            libc::pthread_sigmask(libc::SIG_BLOCK, &mask, &mut old);
            libc::tcsetpgrp(self.fd, self.previous);
            libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        }
    }
}
#[derive(Debug)]
pub(super) struct Blocked {
    pub pid: u32,
    barrier: Option<OwnedFd>,
    status: OwnedFd,
    streams: Option<(OwnedFd, OwnedFd, Option<TerminalLease>)>,
    released: bool,
}
impl Blocked {
    pub fn fork(spec: &ProcessSpec, nonce: &str, cx: &OperationContext) -> io::Result<Self> {
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
            if key.as_bytes().contains(&b'=') || key.as_bytes() == b"EFFIGY_CUSTODY_NONCE" {
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
        env.push(
            CString::new(format!("EFFIGY_CUSTODY_NONCE={nonce}"))
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid nonce"))?,
        );
        let mut args: Vec<_> = argv.iter().map(|a| a.as_ptr()).collect();
        args.push(std::ptr::null());
        let mut vars: Vec<_> = env.iter().map(|a| a.as_ptr()).collect();
        vars.push(std::ptr::null());
        let cwd = DirFd::walk(&spec.cwd, Trust::Operator).map_err(io::Error::other)?;
        let _fork = FORK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (barrier_read, barrier_write) = pipe()?;
        let barrier_read = high(barrier_read)?;
        let (status_read, status_write) = pipe()?;
        let status_write = high(status_write)?;
        let (out_read, out_write) = pipe()?;
        let out_write = high(out_write)?;
        let (err_read, err_write) = pipe()?;
        let err_write = high(err_write)?;
        let (input_read, input_write) = pipe()?;
        let input_read = high(input_read)?;
        drop(input_write);
        // SAFETY: no Rust code, allocation or locks run in the child. All
        // buffers and pointers are prepared above and remain initialized and
        // immutable in its private address space until execve or _exit.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            // SAFETY: every fd below is live in the inherited descriptor table;
            // source fds are >=10, so dup2 targets cannot overwrite a source.
            // After closefrom/range only stdio, barrier and status remain. In
            // particular no sibling's barrier or sentinel writer is inherited.
            unsafe {
                if libc::fchdir(cwd.raw()) != 0 || libc::setpgid(0, 0) != 0 {
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
                #[cfg(target_os = "linux")]
                {
                    if libc::syscall(libc::SYS_close_range, 5u32, u32::MAX, 0u32) != 0 {
                        // Linux before close_range: a finite descriptor-table bound
                        // captured by the OS. sysconf itself is async-signal-safe.
                        let max = libc::sysconf(libc::_SC_OPEN_MAX);
                        let mut fd = 5;
                        while i64::from(fd) < max {
                            libc::close(fd);
                            fd += 1;
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    unsafe extern "C" {
                        fn closefrom(lowfd: libc::c_int);
                    }
                    closefrom(5);
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
                let ready = [1u8];
                if libc::write(4, ready.as_ptr().cast(), 1) != 1 {
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
        nonblock(&out_read)?;
        nonblock(&err_read)?;
        nonblock(&status_read)?;
        let mut child = Self {
            pid: pid as u32,
            barrier: Some(barrier_write),
            status: status_read,
            streams: Some((out_read, err_read, None)),
            released: false,
        };
        child.await_status(cx, false)?;
        if matches!(spec.terminal, Terminal::Foreground) {
            child.streams.as_mut().expect("streams present").2 =
                Some(TerminalLease::take(pid as u32)?);
        }
        Ok(child)
    }
    fn await_status(&mut self, cx: &OperationContext, exec: bool) -> io::Result<()> {
        loop {
            cx.check("spawn barrier")
                .map_err(|e| io::Error::other(format!("{e:?}")))?;
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
    pub fn take_streams(&mut self) -> (OwnedFd, OwnedFd, Option<TerminalLease>) {
        self.streams.take().expect("streams transferred once")
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
        if !self.released {
            self.barrier.take();
            // SAFETY: this is our unreaped direct child, still behind the barrier.
            // It cannot have execed, changed credentials, or reused its pid.
            unsafe { libc::kill(self.pid as i32, libc::SIGKILL) };
            // SAFETY: SIGKILL of a pre-exec direct child guarantees termination.
            unsafe { libc::waitpid(self.pid as i32, std::ptr::null_mut(), 0) };
        }
    }
}
pub(super) fn peek(pid: u32) -> io::Result<Option<Exit>> {
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
pub(super) fn reap(pid: u32) -> io::Result<Option<Exit>> {
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
