//! Bounded self-hash scaffolding. This does not grant a helper capability.
//!
//! A stalled filesystem read can retain at most one worker thread per process.
//! A timed-out or cancelled started attempt permanently refuses later attempts;
//! a late worker result can never authorize them. An expired context rejected
//! before starting does not use the worker slot. Thread creation itself is not
//! proven bounded; this scaffold cannot be used as a deadline guarantee.
use crate::ctx::{Deadline, OperationContext};
use crate::id::Digest;
use sha2::{Digest as _, Sha256};
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

#[derive(Debug)]
enum State {
    Fresh,
    Running,
    Ready(Digest, Instant),
    Refused,
}

#[derive(Debug)]
struct Cache {
    state: Mutex<State>,
    claimed: AtomicBool,
}

impl Cache {
    fn new() -> Self {
        Self {
            state: Mutex::new(State::Fresh),
            claimed: AtomicBool::new(false),
        }
    }

    fn get(
        self: &Arc<Self>,
        cx: &OperationContext,
        read: impl FnOnce() -> io::Result<Digest> + Send + 'static,
    ) -> Result<Digest, &'static str> {
        let deadline = match cx.deadline() {
            Deadline::At(deadline) => deadline,
            Deadline::Open(_) => return Err("self-hash needs a finite deadline"),
        };
        cx.check("helper self-hash")
            .map_err(|_| "helper self-hash unavailable")?;
        if self.claimed.swap(true, Ordering::AcqRel) {
            return Err("helper self-hash unavailable");
        }
        {
            let mut state = self.lock_until(cx, deadline)?;
            *state = State::Running;
        }
        let cache = Arc::clone(self);
        if std::thread::Builder::new()
            .name("effigy-helper-self-hash".into())
            .spawn(move || {
                let result = read();
                let finished = Instant::now();
                let mut state = cache
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if matches!(*state, State::Running) {
                    *state = match result {
                        Ok(digest) => State::Ready(digest, finished),
                        Err(_) => State::Refused,
                    };
                }
            })
            .is_err()
        {
            return Err("helper self-hash unavailable");
        }
        loop {
            let mut state = self.lock_until(cx, deadline)?;
            match &*state {
                State::Ready(digest, finished) if *finished < deadline => {
                    let digest = digest.clone();
                    // A hash alone is not a qualified birth receipt. Until
                    // receipt integration exists, do not cache it for reuse.
                    *state = State::Refused;
                    return Ok(digest);
                }
                State::Ready(..) | State::Refused => {
                    *state = State::Refused;
                    return Err("helper self-hash unavailable");
                }
                State::Running => drop(state),
                State::Fresh => return Err("helper self-hash unavailable"),
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn lock_until(
        &self,
        cx: &OperationContext,
        deadline: Instant,
    ) -> Result<std::sync::MutexGuard<'_, State>, &'static str> {
        loop {
            if cx.check("helper self-hash").is_err() || Instant::now() >= deadline {
                return Err("helper self-hash unavailable");
            }
            match self.state.try_lock() {
                Ok(state) => return Ok(state),
                Err(TryLockError::Poisoned(_)) => return Err("helper self-hash unavailable"),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }
}

static OWNER: AtomicU32 = AtomicU32::new(0);
static CACHE: OnceLock<Arc<Cache>> = OnceLock::new();

/// Hash the current process image once, with an operation-bound result.
/// A post-fork child refuses inherited state before touching the cache lock.
pub(crate) fn self_hash(cx: &OperationContext) -> Result<Digest, &'static str> {
    let pid = std::process::id();
    let owner = OWNER
        .compare_exchange(0, pid, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|owner| owner);
    if owner != 0 && owner != pid {
        return Err("helper self-hash unavailable after fork");
    }
    CACHE.get_or_init(|| Arc::new(Cache::new())).get(cx, || {
        #[cfg(target_os = "linux")]
        let path = std::path::PathBuf::from("/proc/self/exe");
        #[cfg(not(target_os = "linux"))]
        let path = std::env::current_exe()?;
        let mut file = std::fs::File::open(path)?;
        let mut sha = Sha256::new();
        let mut bytes = [0; 8192];
        loop {
            let count = file.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            sha.update(&bytes[..count]);
        }
        Ok(Digest(sha.finalize().into()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::{CancelToken, EffectClass, Limits, Placement};

    fn context(budget: Duration) -> OperationContext {
        OperationContext::new(
            Deadline::At(Instant::now() + budget),
            CancelToken::default(),
            Limits {
                capture_tail: 4096,
                log_rotate: 128,
                log_keep: 2,
                line_max: 128,
                queue_bytes: 4096,
                probe_concurrency: 1,
            },
            EffectClass::Mutate,
            Placement::Host,
        )
    }

    #[test]
    fn successful_hash_is_not_reused_without_a_receipt() {
        let cache = Arc::new(Cache::new());
        let cx = context(Duration::from_secs(2));
        let expected = Digest::of(b"image");
        assert_eq!(
            cache.get(&cx, || Ok(Digest::of(b"image"))),
            Ok(expected.clone())
        );
        assert!(cache
            .get(&cx, || panic!("must not spawn another worker"))
            .is_err());
    }

    #[test]
    fn a_stalled_state_lock_refuses_within_the_deadline() {
        let cache = Arc::new(Cache::new());
        let lock = cache.state.lock().unwrap();
        let started = Instant::now();
        assert!(cache
            .get(&context(Duration::from_millis(50)), || {
                panic!("a locked cache must not start a worker")
            })
            .is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(lock);
        assert!(cache
            .get(&context(Duration::from_secs(1)), || {
                panic!("a spent slot must not start a worker")
            })
            .is_err());
    }

    #[test]
    fn late_worker_cannot_authorize_a_later_attempt() {
        let cache = Arc::new(Cache::new());
        let (release, blocked) = std::sync::mpsc::channel();
        let (finished, completion) = std::sync::mpsc::channel();
        let cx = context(Duration::from_millis(50));
        assert!(cache
            .get(&cx, move || {
                blocked.recv().unwrap();
                finished.send(()).unwrap();
                Ok(Digest::of(b"late"))
            })
            .is_err());
        release.send(()).unwrap();
        completion.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(cache
            .get(&context(Duration::from_secs(2)), || {
                panic!("timeout must not start another worker")
            })
            .is_err());
    }
}
