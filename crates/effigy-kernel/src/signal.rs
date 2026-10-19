//! One process-wide router, installed explicitly by the binary before spawning.
//! The async handler only publishes bits. Normal code polls the router and
//! cancels/forwards under custody; libraries never install handlers themselves.
use crate::ctx::{CancelToken, StopCause};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};

static INSTALLED: AtomicBool = AtomicBool::new(false);
static FIRST: AtomicI32 = AtomicI32::new(0);
static POLICY: AtomicUsize = AtomicUsize::new(0);
static LAST_INTERRUPT: AtomicU64 = AtomicU64::new(0);
static ESCALATE: AtomicBool = AtomicBool::new(false);
fn root_cause() -> Option<StopCause> {
    let signal = FIRST.load(Ordering::Acquire);
    (signal != 0).then_some(StopCause::Signal(signal))
}
pub(crate) fn escalated() -> bool {
    ESCALATE.load(Ordering::Acquire)
}
static CURSOR: AtomicUsize = AtomicUsize::new(0);
static PENDING: [AtomicU64; 32] = [const { AtomicU64::new(0) }; 32];
extern "C" fn handler(signal: i32) {
    let policy = POLICY.load(Ordering::Acquire);
    if policy == 3 || (policy == 2 && signal == libc::SIGHUP) {
        return;
    }
    let _ = FIRST.compare_exchange(0, signal, Ordering::AcqRel, Ordering::Acquire);
    let kind = match signal {
        libc::SIGHUP => 1,
        libc::SIGINT => 2,
        libc::SIGTERM => 3,
        _ => return,
    };
    // POSIX clock_gettime is async-signal-safe. The bounded atomic mailbox
    // preserves repeated deliveries, including two INTs before the next poll.
    // errno is preserved so the handler cannot corrupt an interrupted syscall.
    unsafe {
        #[cfg(target_os = "macos")]
        let errno = libc::__error();
        #[cfg(target_os = "linux")]
        let errno = libc::__errno_location();
        let saved = *errno;
        let mut time: libc::timespec = std::mem::zeroed();
        let tick = if libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) == 0 {
            (time.tv_sec as u64)
                .saturating_mul(1000)
                .saturating_add(time.tv_nsec as u64 / 1_000_000)
        } else {
            1
        };
        if signal != libc::SIGHUP {
            let previous = LAST_INTERRUPT.swap(tick, Ordering::AcqRel);
            if previous != 0 && tick.saturating_sub(previous) <= 2000 {
                ESCALATE.store(true, Ordering::Release);
            }
        }
        PENDING[CURSOR.fetch_add(1, Ordering::Relaxed) % PENDING.len()]
            .store((tick << 2) | kind, Ordering::Release);
        *errno = saved;
    }
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
    last: Option<u64>,
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
        POLICY.store(
            match policy {
                RouterPolicy::Inline => 0,
                RouterPolicy::Client => 1,
                RouterPolicy::Supervisor => 2,
                RouterPolicy::Sentinel => 3,
            },
            Ordering::Release,
        );
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
            cancel: CancelToken::from_signal_source(root_cause),
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
        let mut pending: Vec<_> = PENDING
            .iter()
            .map(|slot| slot.swap(0, Ordering::Acquire))
            .filter(|event| *event != 0)
            .collect();
        pending.sort_unstable();
        let mut events = Vec::new();
        for event in pending {
            let now = event >> 2;
            let signal = match event & 3 {
                1 => libc::SIGHUP,
                2 => libc::SIGINT,
                _ => libc::SIGTERM,
            };
            if self.policy == RouterPolicy::Sentinel
                || (signal == libc::SIGHUP && self.policy == RouterPolicy::Supervisor)
            {
                continue;
            }
            let escalate = signal != libc::SIGHUP
                && self
                    .last
                    .is_some_and(|last| now.saturating_sub(last) <= 2000);
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

#[cfg(all(test, feature = "fault-injection"))]
mod tests {
    use super::*;
    #[test]
    fn library_token_observes_int_without_router_poll_and_escalates() {
        let mut router = SignalRouter::install(RouterPolicy::Inline).unwrap();
        assert!(SignalRouter::install(RouterPolicy::Inline).is_err());
        let token = router.root_cancel();
        handler(libc::SIGINT);
        // An in-process interpreter's only responsibility is polling its token.
        assert_eq!(token.cause(), Some(&StopCause::Signal(libc::SIGINT)));
        handler(libc::SIGINT);
        let routed = router.poll();
        assert_eq!(routed.len(), 2);
        assert!(!routed[0].escalate);
        assert!(routed[1].escalate);
        assert!(escalated());
        let cleanup = crate::ctx::OperationContext::new(
            crate::ctx::Deadline::Open(crate::ctx::OpenReason::Shell),
            token,
            crate::ctx::Limits {
                capture_tail: 1,
                log_rotate: 1,
                log_keep: 1,
                line_max: 1,
                queue_bytes: 1,
                probe_concurrency: 1,
            },
            crate::ctx::EffectClass::Mutate,
            crate::ctx::Placement::Host,
        )
        .cleanup(std::time::Duration::from_secs(1))
        .unwrap();
        assert!(cleanup.operation().cancel_token().cause().is_none());
    }
    #[test]
    fn detached_router_ignores_hup_but_routes_term() {
        let mut router = SignalRouter::install(RouterPolicy::Supervisor).unwrap();
        handler(libc::SIGHUP);
        assert!(router.poll().is_empty());
        assert!(router.root_cancel().cause().is_none());
        handler(libc::SIGTERM);
        assert_eq!(router.poll()[0].forward, libc::SIGTERM);
    }
    #[test]
    fn rhai_try_catch_cannot_swallow_signal_sourced_stop() {
        let router = SignalRouter::install(RouterPolicy::Inline).unwrap();
        let cancel = router.root_cancel();
        let observed = cancel.clone();
        let sent = std::sync::atomic::AtomicBool::new(false);
        let mut engine = rhai::Engine::new();
        engine.on_progress(move |operations| {
            if operations >= 10 && !sent.swap(true, Ordering::SeqCst) {
                // SAFETY: own process, with its one router installed above.
                assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
            }
            observed.cause().map(|_| rhai::Dynamic::UNIT)
        });
        let error = engine
            .eval::<rhai::Dynamic>("loop { try { loop {} } catch (error) {} }")
            .unwrap_err();
        assert!(matches!(*error, rhai::EvalAltResult::ErrorTerminated(..)));
        assert_eq!(cancel.cause(), Some(&StopCause::Signal(libc::SIGINT)));
    }
}
