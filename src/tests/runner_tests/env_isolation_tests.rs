//! Deterministic regressions for process-environment isolation between
//! parallel runner tests.
//!
//! Under plain `cargo test -p effigy --lib`, tests share one process on
//! parallel threads, so every remaining global env writer must go through
//! the shared reentrant test boundary — `EnvGuard`, which holds the same
//! `lock_test()` every runner env test uses. Product-side readers that only
//! needed an env fact (container handoff, deferred depth, build identity)
//! take injected values instead, so no global writers remain outside this
//! boundary. These tests prove the boundary actually excludes concurrent
//! writers.

use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME;
use crate::runner::tests::prelude::EnvGuard;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

/// Parallel writers cycling contested env keys through the shared boundary
/// must each observe their own values: the boundary cannot let one writer's
/// window overlap another's.
#[test]
fn env_isolation_parallel_writers_do_not_overwrite_one_another() {
    const WRITERS: usize = 4;
    const ROUNDS: usize = 50;
    let barrier = Arc::new(Barrier::new(WRITERS));

    let writers: Vec<_> = (0..WRITERS)
        .map(|writer| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                for round in 0..ROUNDS {
                    let handoff = format!("writer-{writer}-round-{round}");
                    let cargo_home = format!("/env-isolation/{writer}/{round}/home");
                    let cargo_target = format!("/env-isolation/{writer}/{round}/target");
                    barrier.wait();
                    let guard = EnvGuard::set_many(&[
                        (CONTAINER_HANDOFF_ENV_NAME, Some(handoff.clone())),
                        ("CARGO_HOME", Some(cargo_home.clone())),
                        ("CARGO_TARGET_DIR", Some(cargo_target.clone())),
                    ]);
                    assert_eq!(
                        env::var(CONTAINER_HANDOFF_ENV_NAME).ok().as_deref(),
                        Some(handoff.as_str()),
                        "another writer's environment overlapped this writer's window"
                    );
                    assert_eq!(
                        env::var("CARGO_HOME").ok().as_deref(),
                        Some(cargo_home.as_str())
                    );
                    assert_eq!(
                        env::var("CARGO_TARGET_DIR").ok().as_deref(),
                        Some(cargo_target.as_str())
                    );
                    drop(guard);
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().expect("writer thread");
    }
}

/// A reader that requires a key to be absent — as the container-handoff
/// probe did before it took an injected value — keeps that guarantee while
/// parallel writers cycle the same key, because absence windows and writes
/// take the same boundary.
#[test]
fn env_isolation_absence_reader_survives_parallel_handoff_writers() {
    const ROUNDS: usize = 200;
    let stop = Arc::new(AtomicBool::new(false));

    let writers: Vec<_> = (0..2)
        .map(|writer| {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    let guard = EnvGuard::set_many(&[(
                        CONTAINER_HANDOFF_ENV_NAME,
                        Some(format!("writer-{writer}")),
                    )]);
                    drop(guard);
                }
            })
        })
        .collect();

    for _ in 0..ROUNDS {
        let guard = EnvGuard::set_many(&[(CONTAINER_HANDOFF_ENV_NAME, None)]);
        assert!(
            env::var_os(CONTAINER_HANDOFF_ENV_NAME).is_none(),
            "a parallel writer overwrote an absence window held under the shared boundary"
        );
        drop(guard);
    }

    stop.store(true, Ordering::SeqCst);
    for writer in writers {
        writer.join().expect("writer thread");
    }

    // Leave the key absent for whatever runs next in this process.
    let _cleanup = EnvGuard::set_many(&[(CONTAINER_HANDOFF_ENV_NAME, None)]);
    assert!(env::var_os(CONTAINER_HANDOFF_ENV_NAME).is_none());
}
