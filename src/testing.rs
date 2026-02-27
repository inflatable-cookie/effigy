use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const VITEST_CONFIG_FILES: &[&str] = &[
    "vitest.config.ts",
    "vitest.config.mts",
    "vitest.config.js",
    "vitest.config.mjs",
    "vitest.config.cjs",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRunner {
    Vitest,
    CargoNextest,
    CargoTest,
}

impl TestRunner {
    pub fn label(self) -> &'static str {
        match self {
            TestRunner::Vitest => "vitest",
            TestRunner::CargoNextest => "cargo-nextest",
            TestRunner::CargoTest => "cargo-test",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRunnerPlan {
    pub runner: TestRunner,
    pub command: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRunnerCandidate {
    pub runner: TestRunner,
    pub command: String,
    pub available: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRunnerDetection {
    pub selected: Option<TestRunnerPlan>,
    pub candidates: Vec<TestRunnerCandidate>,
}

pub fn detect_test_runner(repo_root: &Path) -> Option<TestRunnerPlan> {
    detect_test_runner_detailed(repo_root).selected
}

pub fn detect_test_runner_detailed(repo_root: &Path) -> TestRunnerDetection {
    let (vitest_plan, vitest_candidate) = detect_vitest(repo_root);
    let (nextest_plan, nextest_candidate, fallback_plan, fallback_candidate) =
        detect_rust(repo_root);
    let selected = vitest_plan.or(nextest_plan).or(fallback_plan);
    TestRunnerDetection {
        selected,
        candidates: vec![vitest_candidate, nextest_candidate, fallback_candidate],
    }
}

fn detect_vitest(repo_root: &Path) -> (Option<TestRunnerPlan>, TestRunnerCandidate) {
    let mut evidence = Vec::<String>::new();
    let package_json = repo_root.join("package.json");
    if let Ok(raw) = fs::read_to_string(&package_json) {
        if package_json_mentions_vitest(&raw) {
            evidence.push("package.json includes vitest dependency/script evidence".to_owned());
        }
    }

    for filename in VITEST_CONFIG_FILES {
        if repo_root.join(filename).is_file() {
            evidence.push(format!("found `{filename}`"));
        }
    }

    if repo_root.join("node_modules/.bin/vitest").is_file() {
        evidence.push("found local `node_modules/.bin/vitest` executable".to_owned());
    }

    if evidence.is_empty() {
        return (
            None,
            TestRunnerCandidate {
                runner: TestRunner::Vitest,
                command: "vitest".to_owned(),
                available: false,
                reason: "no package/config/bin vitest markers found".to_owned(),
            },
        );
    }

    (
        Some(TestRunnerPlan {
            runner: TestRunner::Vitest,
            command: "vitest".to_owned(),
            evidence: evidence.clone(),
        }),
        TestRunnerCandidate {
            runner: TestRunner::Vitest,
            command: "vitest".to_owned(),
            available: true,
            reason: evidence.join("; "),
        },
    )
}

fn detect_rust(
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

fn package_json_mentions_vitest(raw: &str) -> bool {
    let Ok(json) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    dependency_contains(&json, "dependencies", "vitest")
        || dependency_contains(&json, "devDependencies", "vitest")
        || scripts_contain_vitest(&json)
}

fn dependency_contains(json: &Value, field: &str, name: &str) -> bool {
    json.get(field)
        .and_then(Value::as_object)
        .is_some_and(|deps| deps.contains_key(name))
}

fn scripts_contain_vitest(json: &Value) -> bool {
    json.get("scripts")
        .and_then(Value::as_object)
        .is_some_and(|scripts| {
            scripts
                .values()
                .filter_map(Value::as_str)
                .any(|script| script.contains("vitest"))
        })
}

fn command_on_path(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<PathBuf>>())
        .unwrap_or_default()
        .into_iter()
        .any(|dir| dir.join(command).is_file())
}

#[cfg(test)]
#[path = "tests/testing_tests.rs"]
mod tests;
