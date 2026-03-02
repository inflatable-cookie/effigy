use std::path::{Path, PathBuf};

use super::{TestRunner, TestRunnerCandidate, TestRunnerPlan};

pub(super) fn detect_rust(
    repo_root: &Path,
) -> (
    Option<TestRunnerPlan>,
    TestRunnerCandidate,
    Option<TestRunnerPlan>,
    TestRunnerCandidate,
) {
    let cargo_toml = repo_root.join("Cargo.toml");
    if !cargo_toml.is_file() {
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
    if command_on_path("cargo-nextest") {
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

fn command_on_path(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<PathBuf>>())
        .unwrap_or_default()
        .into_iter()
        .any(|dir| dir.join(command).is_file())
}
