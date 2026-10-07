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
//! writers and protects the PATH and graph-budget readers used by release-
//! preparation proofs.

use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME;
use crate::runner::tests::prelude::EnvGuard;
use std::env;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Barrier};
use std::thread;
use std::time::Duration;

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

/// PATH based child spawning and graph-budget reads wait for an active scoped
/// writer, then observe the restored process environment. This covers the two
/// release-preparation failures without changing production behavior.
#[test]
fn env_isolation_path_and_graph_budget_readers_wait_for_scoped_writer() {
    const PATH: &str = "PATH";
    const GRAPH_TIMEOUT: &str = "EFFIGY_GRAPH_TIMEOUT_MS";

    let (original_path, original_timeout, original_budget) = {
        let _lock = crate::contract_test_support::lock_test();
        (
            env::var_os(PATH),
            env::var_os(GRAPH_TIMEOUT),
            crate::runner::graph_time_budget::graph_time_budget(),
        )
    };
    assert!(original_path.is_some(), "the shell fixture requires PATH");

    let fixture = tempfile::tempdir().expect("tempdir");
    let empty_bin = fixture.path().join("empty-bin");
    std::fs::create_dir(&empty_bin).expect("create empty PATH directory");
    let empty_path = empty_bin.display().to_string();
    let (writer_active_tx, writer_active_rx) = mpsc::channel();
    let (reader_attempting_tx, reader_attempting_rx) = mpsc::channel();
    let (reader_entered_tx, reader_entered_rx) = mpsc::channel();

    let writer_path = empty_path.clone();
    let writer = thread::spawn(move || {
        let _env = EnvGuard::set_many(&[
            (PATH, Some(writer_path.clone())),
            (GRAPH_TIMEOUT, Some("1".to_owned())),
        ]);
        assert_eq!(
            env::var_os(PATH).as_deref(),
            Some(std::ffi::OsStr::new(writer_path.as_str()))
        );
        assert_eq!(
            crate::runner::graph_time_budget::graph_time_budget(),
            Some(Duration::from_millis(1))
        );
        writer_active_tx.send(()).expect("notify active writer");
        reader_attempting_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reader attempted to acquire the shared boundary");
        assert!(
            reader_entered_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err(),
            "a reader entered while the scoped environment writer held the boundary"
        );
        drop(_env);
        reader_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reader proceeded after the writer restored the environment");
    });

    let reader_original_path = original_path.clone();
    let reader = thread::spawn(move || {
        writer_active_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("writer became active");
        reader_attempting_tx
            .send(())
            .expect("notify reader attempting");
        let _lock = crate::contract_test_support::lock_test();
        reader_entered_tx.send(()).expect("notify reader entered");
        assert_eq!(env::var_os(PATH), reader_original_path);
        assert_eq!(
            crate::runner::graph_time_budget::graph_time_budget(),
            original_budget
        );
        let status = Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .status()
            .expect("PATH resolves the shell after the writer restores it");
        assert!(status.success(), "shell fixture exited with {status}");
    });

    writer.join().expect("writer thread");
    reader.join().expect("reader thread");

    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _env = EnvGuard::set_many(&[
            (PATH, Some(empty_path)),
            (GRAPH_TIMEOUT, Some("1".to_owned())),
        ]);
        std::panic::resume_unwind(Box::new("injected fixture unwind"));
    }));
    assert!(unwind.is_err(), "fixture unwind was not observed");
    let _lock = crate::contract_test_support::lock_test();
    assert_eq!(env::var_os(PATH), original_path);
    assert_eq!(env::var_os(GRAPH_TIMEOUT), original_timeout);
}
