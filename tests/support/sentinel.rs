//! Record the exact foreground sentinel so a failed test can stop its own run.

use effigy_kernel::{
    ident::{self, ProcessIdentity, ProcessState},
    observe::Observed,
};
use effigy_test_support::process::FixtureCommandExt;
use std::{
    process::Command,
    time::{Duration, Instant},
};

pub(super) struct Sentinels {
    identities: Vec<ProcessIdentity>,
}

impl Sentinels {
    /// Capture the sentinel while its owner is still alive, using the OS
    /// parent relationship and then pinning its PID to the kernel start ID.
    ///
    /// The wait is for an event — the owner's sentinel appearing — and the
    /// timeout only stops a sentinel that never starts, so it keeps its floor
    /// on a quiet host and stretches with measured load: a spawned helper
    /// waits for the scheduler, and that is not the behaviour under test.
    pub(super) fn for_owner(owner_pid: u32, timeout: Duration) -> Self {
        let timeout = effigy_test_support::load_scaled_guard(timeout);
        let started = Instant::now();
        loop {
            let output = Command::new("ps")
                .args(["-axo", "pid=,ppid=,args="])
                .output_owned()
                .expect("list processes to find this run's sentinel");
            assert!(output.status.success(), "ps failed: {output:?}");
            let text = String::from_utf8_lossy(&output.stdout);
            let pids = text
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split_whitespace();
                    let pid = fields.next()?.parse::<u32>().ok()?;
                    let parent = fields.next()?.parse::<u32>().ok()?;
                    (parent == owner_pid && line.contains("__kernel-sentinel")).then_some(pid)
                })
                .collect::<Vec<_>>();
            if !pids.is_empty() {
                let identities = pids
                    .into_iter()
                    .map(|pid| match ident::process_identity(pid) {
                        Observed::Verified(identity, _) => identity,
                        observation => panic!(
                            "sentinel PID {pid} has no verified start identity: {observation:?}"
                        ),
                    })
                    .collect();
                return Self { identities };
            }
            assert!(
                started.elapsed() < timeout,
                "owner {owner_pid} did not start its kernel sentinel"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Sentinels {
    fn drop(&mut self) {
        // A timed-out assertion is the failure path. Stop only the exact
        // process recorded by this fixture; never signal a reused PID.
        if !std::thread::panicking() {
            return;
        }
        for identity in &self.identities {
            if matches!(ident::process_identity(identity.pid), Observed::Verified(ref current, _) if *current == *identity)
            {
                // SAFETY: the exact PID and start identity still match this fixture.
                unsafe {
                    libc::kill(identity.pid as i32, libc::SIGKILL);
                }
                let deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < deadline {
                    match ident::process_identity(identity.pid) {
                        Observed::Absent(_) => break,
                        Observed::Verified(current, _) if current != *identity => break,
                        Observed::Verified(..)
                            if matches!(
                                ident::process_state(identity.pid),
                                Observed::Verified(ProcessState::Zombie, _)
                            ) =>
                        {
                            break;
                        }
                        _ => std::thread::sleep(Duration::from_millis(20)),
                    }
                }
            }
        }
    }
}
