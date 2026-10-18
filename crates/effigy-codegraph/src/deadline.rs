//! One cooperative deadline, inherited by graph work on the calling thread.
//! Checks happen before walks, lock polls and file transactions. A transaction
//! already in progress commits or rolls back before control returns.
use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::CodeGraphError;

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

struct RestoreDeadline(Option<Instant>);

impl Drop for RestoreDeadline {
    fn drop(&mut self) {
        DEADLINE.set(self.0);
    }
}

/// Run synchronously under a deadline. Nested calls may only shorten it.
pub fn with_deadline<T>(budget: Duration, operation: impl FnOnce() -> T) -> T {
    let requested = Instant::now().checked_add(budget);
    let previous = DEADLINE.get();
    let deadline = match (previous, requested) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    DEADLINE.set(deadline);
    let _restore = RestoreDeadline(previous);
    operation()
}

pub(crate) fn check() -> Result<(), CodeGraphError> {
    if DEADLINE
        .get()
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        Err(CodeGraphError::DeadlineExceeded)
    } else {
        Ok(())
    }
}

pub(crate) fn remaining() -> Option<Duration> {
    DEADLINE
        .get()
        .map(|deadline| deadline.saturating_duration_since(Instant::now()))
}

#[cfg(test)]
pub(crate) fn expire() {
    DEADLINE.set(Some(Instant::now()));
}
