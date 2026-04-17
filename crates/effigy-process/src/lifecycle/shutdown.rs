use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::super::signal;

pub(super) fn terminate_child_graceful(child: &Arc<Mutex<Child>>, timeout: Duration) {
    signal::send_terminate(&mut child.lock().expect("child lock"));

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let exited = child
            .lock()
            .expect("child lock")
            .try_wait()
            .ok()
            .flatten()
            .is_some();
        if exited {
            return;
        }
        thread::sleep(Duration::from_millis(30));
    }

    signal::send_kill(&mut child.lock().expect("child lock"));
}
