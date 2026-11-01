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
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_workspace_capacity_reservation_has_one_owner() {
        let capacity = Capacity::default();
        let start = Arc::new(Barrier::new(3));
        let held = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let capacity = capacity.clone();
            let start = start.clone();
            let held = held.clone();
            threads.push(std::thread::spawn(move || {
                start.wait();
                let reservation = capacity.reserve();
                let accepted = reservation.is_ok();
                held.wait();
                drop(reservation);
                accepted
            }));
        }
        start.wait();
        held.wait();
        let accepted = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|accepted| *accepted)
            .count();
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
}
