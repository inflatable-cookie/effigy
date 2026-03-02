use serde_json::Value;
use std::fs;
use std::path::Path;

use super::{TestRunner, TestRunnerCandidate, TestRunnerPlan};

const VITEST_CONFIG_FILES: &[&str] = &[
    "vitest.config.ts",
    "vitest.config.mts",
    "vitest.config.js",
    "vitest.config.mjs",
    "vitest.config.cjs",
];

pub(super) fn detect_vitest(repo_root: &Path) -> (Option<TestRunnerPlan>, TestRunnerCandidate) {
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
                command: "vitest run".to_owned(),
                available: false,
                reason: "no package/config/bin vitest markers found".to_owned(),
            },
        );
    }

    (
        Some(TestRunnerPlan {
            runner: TestRunner::Vitest,
            command: "vitest run".to_owned(),
            evidence: evidence.clone(),
        }),
        TestRunnerCandidate {
            runner: TestRunner::Vitest,
            command: "vitest run".to_owned(),
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
