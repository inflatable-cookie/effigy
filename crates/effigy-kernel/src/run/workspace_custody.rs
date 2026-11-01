//! Unwired, pre-effect capacity scaffold. A reservation is not a Run, gate,
//! process, or permission to perform an effect. It must not be used to hold
//! a live or uncertain owner: dropping it releases only this empty slot.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CapacityUnavailable;

impl std::fmt::Display for CapacityUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("workspace custody capacity unavailable")
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub(super) struct Capacity {
    busy: Arc<AtomicBool>,
}

#[allow(dead_code)]
#[must_use]
#[derive(Debug)]
pub(super) struct Reservation {
    busy: Arc<AtomicBool>,
}

impl Capacity {
    #[allow(dead_code)]
    pub(super) fn reserve(&self) -> Result<Reservation, CapacityUnavailable> {
        self.busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| CapacityUnavailable)?;
        Ok(Reservation {
            busy: self.busy.clone(),
        })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::Capacity;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn concurrent_capacity_contenders_hold_reservations_until_observed() {
        let capacity = Capacity::default();
        let (results, observations) = mpsc::channel();
        let mut threads = Vec::new();
        let mut releases = Vec::new();
        for _ in 0..2 {
            let capacity = capacity.clone();
            let results = results.clone();
            let (release, wait) = mpsc::channel::<()>();
            releases.push(release);
            threads.push(std::thread::spawn(move || {
                let reservation = capacity.reserve();
                let accepted = reservation.is_ok();
                results.send(accepted).unwrap();
                let _ = wait.recv_timeout(Duration::from_secs(5));
                drop(reservation);
            }));
        }
        let outcomes: Vec<_> = (0..2)
            .map(|_| observations.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect();
        for release in releases {
            release.send(()).unwrap();
        }
        for thread in threads {
            thread.join().unwrap();
        }
        let accepted = outcomes.into_iter().filter(|accepted| *accepted).count();
        assert_eq!(accepted, 1, "capacity must have exactly one owner");
    }

    #[test]
    fn empty_workspace_reservation_releases_after_drop_and_unwind() {
        let capacity = Capacity::default();
        let reservation = capacity.reserve().unwrap();
        assert_eq!(
            capacity.reserve().unwrap_err().to_string(),
            "workspace custody capacity unavailable"
        );
        drop(reservation);
        let unwind = std::panic::catch_unwind(|| {
            let _empty = capacity.reserve().unwrap();
            panic!("pre-effect cancellation");
        });
        assert!(unwind.is_err());
        assert!(capacity.reserve().is_ok());
    }

    #[test]
    fn busy_capacity_precedes_state_root_creation_in_the_fixture() {
        let fixture = tempfile::tempdir().unwrap();
        let capacity = Capacity::default();
        let _reserved = capacity.reserve().unwrap();
        let attempt = || -> Result<(), super::CapacityUnavailable> {
            let _empty_slot = capacity.reserve()?;
            let cx = crate::run::tests::context();
            let _root = crate::root::StateRoot::effects(&cx, fixture.path()).unwrap();
            Ok(())
        };
        assert!(attempt().is_err());
        assert!(
            !fixture.path().join(".effigy").exists(),
            "the refused fixture attempt must not create state"
        );
    }

    #[test]
    fn free_capacity_allows_the_fixture_to_create_and_remove_state() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().to_owned();
        let capacity = Capacity::default();
        let reservation = capacity.reserve().unwrap();
        let cx = crate::run::tests::context();
        let root = crate::root::StateRoot::effects(&cx, &path).unwrap();
        assert!(path.join(".effigy").is_dir());
        drop(root);
        drop(reservation);
        fixture.close().unwrap();
        assert!(!path.exists(), "the fixture must be removed");
    }
}
