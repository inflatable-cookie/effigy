use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::super::signal;

pub(super) fn terminate_child_graceful(child: &Arc<Mutex<Child>>, timeout: Duration) {
    // The direct child's exit is proved by `try_wait` below.
    let _tree = signal::send_terminate(&mut crate::locks::lock_tolerant(child));

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let exited = crate::locks::lock_tolerant(child)
            .try_wait()
            .ok()
            .flatten()
            .is_some();
        if exited {
            return;
        }
        thread::sleep(Duration::from_millis(30));
    }

    let _tree = signal::send_kill(&mut crate::locks::lock_tolerant(child));
}
