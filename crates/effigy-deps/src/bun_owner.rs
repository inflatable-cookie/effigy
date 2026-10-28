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
use std::sync::{Mutex, OnceLock};

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
}

/// Every Bun link owner this process still has in flight.
static OWNERS: OnceLock<Mutex<Vec<Owner>>> = OnceLock::new();
static NEXT_OWNER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// An armed link owner. Dropping it removes this owner from the interrupt
/// watcher, so a signal after the command finished retires nothing.
pub(crate) struct BunLinkOwner {
    id: u64,
}

impl BunLinkOwner {
    pub(crate) fn arm(
        home: &Path,
        consumer_repo: &Path,
        library_path: &Path,
        registrations: Vec<(String, PathBuf)>,
    ) -> Self {
        if registrations.is_empty() {
            return Self { id: 0 };
        }
        let owners = OWNERS.get_or_init(|| {
            watch_interrupts();
            Mutex::new(Vec::new())
        });
        let id = NEXT_OWNER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        owners
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(Owner {
                id,
                home: home.to_path_buf(),
                consumer_repo: consumer_repo.to_path_buf(),
                library_path: library_path.to_path_buf(),
                registrations,
            });
        Self { id }
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
        owners
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .retain(|owner| owner.id != self.id);
    }
}

fn watch_interrupts() {
    use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};

    std::thread::Builder::new()
        .name("effigy-bun-link-interrupt".to_owned())
        .spawn(|| {
            let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGINT, SIGTERM, SIGHUP])
            else {
                return;
            };
            for signal in signals.forever() {
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
                // anything: left running, it would recreate the registration
                // after the owner died. Then retire, and repeat once because
                // the stopped child may have been mid-write when it died.
                for _ in 0..2 {
                    crate::process::terminate_owned_children();
                    for owner in &owners {
                        let retirement = retire_owned_bun_registrations(
                            &owner.home,
                            &owner.consumer_repo,
                            &owner.library_path,
                            &owner.registrations,
                            &StdReadOnlyProcess,
                            &FsBunPlanObserver,
                        );
                        failures.extend(retirement.failures);
                    }
                }
                for failure in &failures {
                    eprintln!("effigy: interrupted Bun link could not retire: {failure}");
                }
                let _ = signal_hook::low_level::emulate_default_handler(signal);
                return;
            }
        })
        .ok();
}
