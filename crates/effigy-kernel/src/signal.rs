//! One process-wide router, installed explicitly by the binary before spawning.
//! The async handler only publishes bits. Normal code polls the router and
//! cancels/forwards under custody; libraries never install handlers themselves.
use crate::ctx::{CancelToken, StopCause};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

static INSTALLED: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicU32 = AtomicU32::new(0);
extern "C" fn handler(signal: i32) {
    // All supported targets have lock-free u32 atomics. No allocation, lock or
    // non-async-signal-safe runtime operation is called from the handler.
    let bit = match signal {
        libc::SIGHUP => 1,
        libc::SIGINT => 2,
        libc::SIGTERM => 4,
        _ => 0,
    };
    PENDING.fetch_or(bit, Ordering::Relaxed);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterPolicy {
    Inline,
    Client,
    Supervisor,
    Sentinel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutedSignal {
    pub received: i32,
    pub forward: i32,
    pub escalate: bool,
}
#[derive(Debug)]
pub struct RouterHandle {
    policy: RouterPolicy,
    cancel: CancelToken,
    last: Option<Instant>,
}
#[derive(Debug)]
pub struct SignalRouter;
impl SignalRouter {
    /// Call once from main, before any thread or child is started. A second
    /// installation is refused even if the first handle has been dropped.
    pub fn install(policy: RouterPolicy) -> io::Result<RouterHandle> {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "signal router already installed",
            ));
        }
        for signal in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM] {
            // SAFETY: zero is a valid empty sigaction; sigemptyset initializes
            // the mask, and sigaction copies the complete initialized structure.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = handler as *const () as usize;
            action.sa_flags = libc::SA_RESTART;
            // SAFETY: mask is writable storage inside a live sigaction.
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            // SAFETY: supported signal number, initialized action, no old action.
            if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(RouterHandle {
            policy,
            cancel: CancelToken::default(),
            last: None,
        })
    }
}
impl RouterHandle {
    pub fn root_cancel(&self) -> CancelToken {
        self.cancel.clone()
    }
    /// Poll at every bounded wait boundary. HUP is ignored only by detached
    /// supervisors/sentinels. Two interrupt/terminate deliveries within 2 s
    /// request immediate custody KILL, while settle still owns cleanup.
    pub fn poll(&mut self) -> Vec<RoutedSignal> {
        let pending = PENDING.swap(0, Ordering::Relaxed);
        let mut events = Vec::new();
        for (bit, signal) in [(1, libc::SIGHUP), (2, libc::SIGINT), (4, libc::SIGTERM)] {
            if pending & bit == 0
                || self.policy == RouterPolicy::Sentinel
                || (signal == libc::SIGHUP && self.policy == RouterPolicy::Supervisor)
            {
                continue;
            }
            let now = Instant::now();
            let escalate = signal != libc::SIGHUP
                && self
                    .last
                    .is_some_and(|last| now.duration_since(last) <= Duration::from_secs(2));
            if signal != libc::SIGHUP {
                self.last = Some(now);
            }
            self.cancel.cancel(StopCause::Signal(signal));
            events.push(RoutedSignal {
                received: signal,
                forward: if escalate {
                    libc::SIGKILL
                } else if signal == libc::SIGHUP {
                    libc::SIGTERM
                } else {
                    signal
                },
                escalate,
            });
        }
        events
    }
}
