use std::path::Path;

mod rust;
mod vitest;

use rust::detect_rust;
use vitest::detect_vitest;

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

pub fn detect_test_runner_plans(repo_root: &Path) -> Vec<TestRunnerPlan> {
    let (vitest_plan, _) = detect_vitest(repo_root);
    let (nextest_plan, _, fallback_plan, _) = detect_rust(repo_root);
    let mut plans = Vec::<TestRunnerPlan>::new();
    if let Some(plan) = vitest_plan {
        plans.push(plan);
    }
    if let Some(plan) = nextest_plan.or(fallback_plan) {
        plans.push(plan);
    }
    plans
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

#[cfg(test)]
#[allow(dead_code)]
#[path = "tests/contract_test_support.rs"]
mod contract_test_support;

#[cfg(test)]
#[path = "tests/testing_tests.rs"]
mod tests;
