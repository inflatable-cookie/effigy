use std::thread;
use std::time::{Duration, Instant};

use super::{signal, ProcessSupervisor, ShutdownProgress};

impl ProcessSupervisor {
    pub fn terminate_all_graceful(&self, timeout: Duration) {
        self.terminate_all_graceful_with_progress(timeout, |_| {});
    }

    pub fn terminate_all_graceful_with_progress<F>(&self, timeout: Duration, mut on_progress: F)
    where
        F: FnMut(ShutdownProgress),
    {
        on_progress(ShutdownProgress::SendingTerm);
        let children = self.all_child_handles();
        for child in &children {
            let mut child = crate::locks::lock_tolerant(child);
            signal::send_terminate(&mut child);
        }

        on_progress(ShutdownProgress::Waiting);
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let all_exited = children.iter().all(|child| {
                crate::locks::lock_tolerant(child)
                    .try_wait()
                    .ok()
                    .flatten()
                    .is_some()
            });
            if all_exited {
                on_progress(ShutdownProgress::Complete {
                    total: children.len(),
                    forced: 0,
                });
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }

        on_progress(ShutdownProgress::ForceKilling);
        let mut forced = 0usize;
        for child in &children {
            let mut child = crate::locks::lock_tolerant(child);
            let still_running = child.try_wait().ok().flatten().is_none();
            if !still_running {
                continue;
            }
            signal::send_kill(&mut child);
            forced += 1;
        }
        on_progress(ShutdownProgress::Complete {
            total: children.len(),
            forced,
        });
    }
}
