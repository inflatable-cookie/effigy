//! The owner that retires the Bun registrations a link creates.
//!
//! `effigy deps link bun` registers each package in the machine-wide Bun
//! install so the consumer's save-less `bun link` can resolve it. The link
//! itself is the consumer's `node_modules` symlink plus the ledger, so the
//! registration is transient: the command that created it retires it when it
//! completes, and an interrupt retires the same registrations before the signal
//! takes its default course. Keeping the registration is what left
//! registrations pointing at deleted worktrees.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::{
    retire_owned_bun_registrations, BunDependencyPlan, BunRegistrationDisposition,
    FsBunPlanObserver, StdReadOnlyProcess,
};

/// The registrations a plan creates or keeps for its consumer.
///
/// A registration this plan created (`Absent`, `StaleOwned`) or already owned
/// with no other consumer (`MatchingOwned`) belongs to this owner. A shared or
/// foreign registration does not, so an interrupt leaves it alone.
pub(crate) fn link_registrations(plan: &BunDependencyPlan) -> Vec<(String, PathBuf)> {
    plan.packages
        .iter()
        .filter(|package| {
            matches!(
                package.registration,
                BunRegistrationDisposition::Absent
                    | BunRegistrationDisposition::StaleOwned
                    | BunRegistrationDisposition::MatchingOwned
            )
        })
        .map(|package| (package.name.clone(), package.local_path.clone()))
        .collect()
}

#[derive(Debug, Clone)]
struct Owner {
    id: u64,
    home: PathBuf,
    consumer_repo: PathBuf,
    library_path: PathBuf,
    registrations: Vec<(String, PathBuf)>,
    /// Set by the transaction once it can create a registration.
    started: Arc<AtomicBool>,
}

/// Every Bun link owner this process still has in flight.
static OWNERS: OnceLock<Mutex<Vec<Owner>>> = OnceLock::new();
static NEXT_OWNER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// An interrupt cleanup that is running or has run since the signal arrived.
///
/// The main thread must not outrun it: stopping the in-flight `bun link` lets
/// the apply fail and return, and if the process left before the watcher
/// retired the registration, nothing would retire it at all.
static INTERRUPT_CLEANUP: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();
static INTERRUPT_SEEN: AtomicBool = AtomicBool::new(false);

fn interrupt_cleanup_state() -> &'static (Mutex<bool>, Condvar) {
    INTERRUPT_CLEANUP.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

/// An armed link owner. Dropping it waits out a pending interrupt cleanup, then
/// removes this owner from the interrupt watcher.
pub(crate) struct BunLinkOwner {
    id: u64,
    started: Arc<AtomicBool>,
}

impl BunLinkOwner {
    /// The marker the transaction sets once it can create a registration.
    pub(crate) fn started(&self) -> &AtomicBool {
        &self.started
    }
}

impl BunLinkOwner {
    pub(crate) fn arm(
        home: &Path,
        consumer_repo: &Path,
        library_path: &Path,
        registrations: Vec<(String, PathBuf)>,
    ) -> Self {
        if registrations.is_empty() {
            return Self {
                id: 0,
                started: Arc::new(AtomicBool::new(false)),
            };
        }
        // The registry has to exist before the watcher can read it, and the
        // watcher has to be live before this owner can create anything: an
        // interrupt in either gap would kill the process and leave a started
        // `bun link` behind with no owner to retire it.
        let owners = OWNERS.get_or_init(|| Mutex::new(Vec::new()));
        if !ensure_interrupt_watcher() {
            // Completion still retires the registration; only an interrupt
            // cannot be cleaned up, and that is reported once above.
            return Self {
                id: 0,
                started: Arc::new(AtomicBool::new(false)),
            };
        }
        let id = NEXT_OWNER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let started = Arc::new(AtomicBool::new(false));
        owners
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(Owner {
                id,
                home: home.to_path_buf(),
                consumer_repo: consumer_repo.to_path_buf(),
                library_path: library_path.to_path_buf(),
                registrations,
                started: Arc::clone(&started),
            });
        Self { id, started }
    }
}

impl Drop for BunLinkOwner {
    fn drop(&mut self) {
        if self.id == 0 {
            return;
        }
        let Some(owners) = OWNERS.get() else {
            return;
        };
        if INTERRUPT_SEEN.load(Ordering::SeqCst) {
            // Do not return from the command (and let the process exit) while
            // the watcher is still retiring this owner's registration.
            let (done, ready) = interrupt_cleanup_state();
            let mut done = done.lock().unwrap_or_else(|poison| poison.into_inner());
            let deadline = Instant::now() + Duration::from_secs(20);
            while !*done {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    break;
                };
                let (next, timeout) = ready
                    .wait_timeout(done, remaining)
                    .unwrap_or_else(|poison| poison.into_inner());
                done = next;
                if timeout.timed_out() {
                    break;
                }
            }
        }
        owners
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .retain(|owner| owner.id != self.id);
    }
}

/// Whether the interrupt watcher is installed, so a signal from now on is
/// handled instead of killing the process outright.
static WATCHER: OnceLock<bool> = OnceLock::new();

fn ensure_interrupt_watcher() -> bool {
    *WATCHER.get_or_init(install_interrupt_watcher)
}

#[cfg(test)]
fn interrupt_watcher_installed() -> bool {
    WATCHER.get().copied().unwrap_or(false)
}

/// Install the interrupt watcher synchronously, before any registration exists.
///
/// `Signals::new` registers the signal handlers in the calling thread, so once
/// this returns a signal is buffered for the watcher instead of taking its
/// default action. That ordering matters: an owner that armed asynchronously
/// could have a `bun link` child running while a signal still killed the
/// process outright, leaving the child to create a registration nobody owns.
fn install_interrupt_watcher() -> bool {
    use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};

    let signals = match signal_hook::iterator::Signals::new([SIGINT, SIGTERM, SIGHUP]) {
        Ok(signals) => signals,
        Err(error) => {
            eprintln!("effigy: cannot watch for interrupts while linking: {error}");
            return false;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("effigy-bun-link-interrupt".to_owned())
        .spawn(move || watch_interrupts(signals))
        .is_ok();
    if !spawned {
        eprintln!("effigy: cannot start the interrupt watcher while linking");
        return false;
    }
    true
}

fn watch_interrupts(mut signals: signal_hook::iterator::Signals) {
    // One interrupt is all this watcher handles: it publishes the interrupt,
    // retires what this process owns, then lets the signal take its default
    // action. `forever()` blocks the same way `next()` does, so an interrupt
    // that never arrives keeps the watcher parked instead of returning.
    if let Some(signal) = signals.forever().next() {
        // Publish the interrupt before touching anything: the main thread waits
        // for this cleanup instead of returning from the command, so the
        // process cannot exit with the registration still owned.
        INTERRUPT_SEEN.store(true, Ordering::SeqCst);
        let owners = OWNERS
            .get()
            .map(|owners| {
                owners
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .clone()
            })
            .unwrap_or_default();
        let mut failures = Vec::new();
        // Stop the `bun link` this owner is waiting on before retiring
        // anything: left running, it would recreate the registration after the
        // owner died. A second pass covers a child that was mid-write or
        // mid-spawn when the first one ran.
        for _ in 0..2 {
            let pass = crate::process::interrupt_cleanup(|| {
                let mut pass = Vec::new();
                for owner in &owners {
                    let retirement = retire_owned_bun_registrations(
                        &owner.home,
                        &owner.consumer_repo,
                        &owner.library_path,
                        &owner.registrations,
                        owner.started.load(Ordering::SeqCst),
                        &StdReadOnlyProcess,
                        &FsBunPlanObserver,
                    );
                    pass.extend(retirement.failures);
                }
                pass
            });
            failures.extend(pass);
        }
        for failure in &failures {
            eprintln!("effigy: interrupted Bun link could not retire: {failure}");
        }
        // Release the main thread: it may be waiting to return from the command.
        let (done, ready) = interrupt_cleanup_state();
        *done.lock().unwrap_or_else(|poison| poison.into_inner()) = true;
        ready.notify_all();
        let _ = signal_hook::low_level::emulate_default_handler(signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn arming_installs_the_interrupt_watcher_before_it_returns() {
        let home = TempDir::new().unwrap();
        let consumer = TempDir::new().unwrap();
        let library = TempDir::new().unwrap();
        assert!(!interrupt_watcher_installed());

        let owner = BunLinkOwner::arm(
            home.path(),
            consumer.path(),
            library.path(),
            vec![("underlay".to_owned(), library.path().to_path_buf())],
        );

        // A signal now is handled instead of killing the process outright, so a
        // `bun link` this owner starts can never outlive the cleanup.
        assert!(interrupt_watcher_installed());
        drop(owner);
    }

    #[test]
    fn dropping_an_owner_waits_for_an_interrupt_cleanup() {
        let home = TempDir::new().unwrap();
        let consumer = TempDir::new().unwrap();
        let library = TempDir::new().unwrap();
        let owner = BunLinkOwner::arm(
            home.path(),
            consumer.path(),
            library.path(),
            vec![("underlay".to_owned(), library.path().to_path_buf())],
        );
        let (done, ready) = interrupt_cleanup_state();
        *done.lock().unwrap_or_else(|poison| poison.into_inner()) = false;
        INTERRUPT_SEEN.store(true, Ordering::SeqCst);

        // The command must not return (and the process must not exit) while the
        // watcher still has the registration to retire.
        let waiting = std::thread::spawn(move || {
            drop(owner);
        });
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !waiting.is_finished(),
            "the owner returned while the interrupt cleanup was still running"
        );

        *done.lock().unwrap_or_else(|poison| poison.into_inner()) = true;
        ready.notify_all();
        waiting.join().unwrap();
        INTERRUPT_SEEN.store(false, Ordering::SeqCst);
    }
}
