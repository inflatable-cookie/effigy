use std::path::Path;

use effigy_core::fs_probe::PathPresenceCache;
use effigy_core::path_probe::command_available_in_path;

use super::{TestRunner, TestRunnerCandidate, TestRunnerPlan};

pub(super) fn detect_rust(
    repo_root: &Path,
) -> (
    Option<TestRunnerPlan>,
    TestRunnerCandidate,
    Option<TestRunnerPlan>,
    TestRunnerCandidate,
) {
    let mut probe = PathPresenceCache::new();
    if !probe.child_is_file(repo_root, "Cargo.toml") {
        return (
            None,
            TestRunnerCandidate {
                runner: TestRunner::CargoNextest,
                command: "cargo nextest run".to_owned(),
                available: false,
                reason: "Cargo.toml not found".to_owned(),
            },
            None,
            TestRunnerCandidate {
                runner: TestRunner::CargoTest,
                command: "cargo test".to_owned(),
                available: false,
                reason: "Cargo.toml not found".to_owned(),
            },
        );
    }
    let mut evidence = vec!["found `Cargo.toml`".to_owned()];
    if command_available_in_path("cargo-nextest") {
        evidence.push("found `cargo-nextest` on PATH".to_owned());
        return (
            Some(TestRunnerPlan {
                runner: TestRunner::CargoNextest,
                command: "cargo nextest run".to_owned(),
                evidence: evidence.clone(),
            }),
            TestRunnerCandidate {
                runner: TestRunner::CargoNextest,
                command: "cargo nextest run".to_owned(),
                available: true,
                reason: evidence.join("; "),
            },
            Some(TestRunnerPlan {
                runner: TestRunner::CargoTest,
                command: "cargo test".to_owned(),
                evidence: vec![
                    "found `Cargo.toml`".to_owned(),
                    "fallback if `cargo nextest run` is unavailable".to_owned(),
                ],
            }),
            TestRunnerCandidate {
                runner: TestRunner::CargoTest,
                command: "cargo test".to_owned(),
                available: true,
                reason: "fallback Rust runner".to_owned(),
            },
        );
    }
    evidence.push("`cargo-nextest` not found on PATH; falling back to `cargo test`".to_owned());
    (
        None,
        TestRunnerCandidate {
            runner: TestRunner::CargoNextest,
            command: "cargo nextest run".to_owned(),
            available: false,
            reason: "Cargo.toml present but `cargo-nextest` is not on PATH".to_owned(),
        },
        Some(TestRunnerPlan {
            runner: TestRunner::CargoTest,
            command: "cargo test".to_owned(),
            evidence: evidence.clone(),
        }),
        TestRunnerCandidate {
            runner: TestRunner::CargoTest,
            command: "cargo test".to_owned(),
            available: true,
            reason: evidence.join("; "),
        },
    )
}
