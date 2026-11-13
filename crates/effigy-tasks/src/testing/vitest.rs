use serde_json::Value;
use std::fs;
use std::path::Path;

use effigy_core::fs_probe::PathPresenceCache;
use effigy_core::shell::shell_quote;

use super::js_config::{scan_config, ConfigScan};
use super::{TestRunner, TestRunnerCandidate, TestRunnerPlan};

const VITEST_CONFIG_FILES: &[&str] = &[
    "vitest.config.ts",
    "vitest.config.mts",
    "vitest.config.cts",
    "vitest.config.js",
    "vitest.config.mjs",
    "vitest.config.cjs",
];

const VITE_CONFIG_FILES: &[&str] = &[
    "vite.config.ts",
    "vite.config.mts",
    "vite.config.cts",
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.cjs",
];

const DEFAULT_COMMAND: &str = "vitest run";

/// A config file and what it says about the suite root.
struct ConfigDeclaration {
    filename: String,
    scan: ConfigScan,
}

pub(super) fn detect_vitest(repo_root: &Path) -> (Option<TestRunnerPlan>, TestRunnerCandidate) {
    let mut evidence = Vec::<String>::new();
    let mut probe = PathPresenceCache::new();
    let package_json = repo_root.join("package.json");
    if let Ok(raw) = fs::read_to_string(&package_json) {
        if package_json_mentions_vitest(&raw) {
            evidence.push("package.json includes vitest dependency/script evidence".to_owned());
        }
    }

    let mut preferred_config: Option<ConfigDeclaration> = None;
    for filename in VITEST_CONFIG_FILES {
        if probe.child_is_file(repo_root, filename) {
            evidence.push(format!("found `{filename}`"));
            if preferred_config.is_none() {
                preferred_config = read_config(repo_root, filename, &mut evidence);
            }
        }
    }

    if preferred_config.is_none() {
        for filename in VITE_CONFIG_FILES {
            if !probe.child_is_file(repo_root, filename) {
                continue;
            }
            let Ok(raw) = fs::read_to_string(repo_root.join(filename)) else {
                continue;
            };
            // A Vite config only counts when a readable `test` block makes it
            // Vitest intent; an unreadable one is not evidence either way.
            let scan = scan_config(&raw);
            if matches!(scan, ConfigScan::NoTestBlock | ConfigScan::Unreadable(_)) {
                continue;
            }
            evidence.push(format!("found `{filename}` with a Vitest `test` block"));
            preferred_config = Some(ConfigDeclaration {
                filename: (*filename).to_owned(),
                scan,
            });
            break;
        }
    }

    let has_bin = probe.child_is_file(repo_root, "node_modules/.bin/vitest");
    if evidence.is_empty() {
        let reason = if has_bin {
            "no package-owned vitest declaration or config; installed `node_modules/.bin/vitest` is not package intent"
        } else {
            "no package-owned vitest declaration or config"
        };
        return (
            None,
            TestRunnerCandidate {
                runner: TestRunner::Vitest,
                command: DEFAULT_COMMAND.to_owned(),
                available: false,
                reason: reason.to_owned(),
            },
        );
    }

    let mut command = DEFAULT_COMMAND.to_owned();
    if let Some(declaration) = &preferred_config {
        match &declaration.scan {
            ConfigScan::Dir(dir) => {
                evidence.push(format!("`{}` test.dir is `{dir}`", declaration.filename));
                command = format!("{DEFAULT_COMMAND} --dir {}", shell_quote(dir));
            }
            ConfigScan::NoDir | ConfigScan::NoTestBlock => {}
            ConfigScan::Unreadable(detail) => {
                // The suite still runs, but the operator has to know the run is
                // not narrowed: the declared dir could not be read.
                evidence.push(format!(
                    "`{}` could not be read as JavaScript ({detail}); the run is not narrowed to a `test.dir`",
                    declaration.filename
                ));
            }
        }
    }

    (
        Some(TestRunnerPlan {
            runner: TestRunner::Vitest,
            command: command.clone(),
            evidence: evidence.clone(),
        }),
        TestRunnerCandidate {
            runner: TestRunner::Vitest,
            command,
            available: true,
            reason: evidence.join("; "),
        },
    )
}

/// Read one Vitest config file into a typed declaration. The file's presence is
/// already evidence; this adds what it declares, or why it could not be read.
fn read_config(
    repo_root: &Path,
    filename: &str,
    evidence: &mut Vec<String>,
) -> Option<ConfigDeclaration> {
    let raw = match fs::read_to_string(repo_root.join(filename)) {
        Ok(raw) => raw,
        Err(error) => {
            evidence.push(format!(
                "`{filename}` could not be read ({error}); the run is not narrowed to a `test.dir`"
            ));
            return Some(ConfigDeclaration {
                filename: filename.to_owned(),
                scan: ConfigScan::Unreadable(error.to_string()),
            });
        }
    };
    Some(ConfigDeclaration {
        filename: filename.to_owned(),
        scan: scan_config(&raw),
    })
}

fn package_json_mentions_vitest(raw: &str) -> bool {
    let Ok(json) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    dependency_contains(&json, "dependencies", "vitest")
        || dependency_contains(&json, "devDependencies", "vitest")
        || dependency_contains(&json, "peerDependencies", "vitest")
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

#[cfg(test)]
mod tests {
    use super::{detect_vitest, TestRunner};

    /// Package-owned intent (a dependency or a config file) activates the
    /// suite; an installed binary alone must not.
    #[test]
    fn detect_vitest_requires_package_intent_not_just_an_installed_binary() {
        let root = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-vitest-intent-{}",
            std::process::id()
        ))
        .expect("fixture directory");

        // A devDependency is intent, even without a config file.
        std::fs::write(
            root.join("package.json"),
            r#"{ "devDependencies": { "vitest": "^1.0.0" } }"#,
        )
        .expect("write package.json");
        let (plan, candidate) = detect_vitest(&root);
        assert_eq!(
            plan.as_ref().map(|plan| plan.runner),
            Some(TestRunner::Vitest),
            "dependency evidence must activate"
        );
        assert!(candidate.available);

        // A config file is intent too, even without a dependency.
        std::fs::write(
            root.join("vitest.config.ts"),
            "export default { test: { dir: 'src' } }\n",
        )
        .expect("write vitest.config.ts");
        std::fs::write(root.join("package.json"), r#"{ "name": "x" }"#)
            .expect("write package.json");
        let (plan, candidate) = detect_vitest(&root);
        assert_eq!(
            plan.as_ref().map(|plan| plan.runner),
            Some(TestRunner::Vitest),
            "config evidence must activate"
        );
        assert!(candidate.available);
        assert!(
            candidate.reason.contains("found `vitest.config.ts`"),
            "{}",
            candidate.reason
        );

        // An installed binary without package-owned intent stays refused.
        std::fs::remove_file(root.join("vitest.config.ts")).expect("remove config");
        std::fs::create_dir_all(root.join("node_modules/.bin")).expect("mkdir node_modules");
        std::fs::write(root.join("node_modules/.bin/vitest"), "#!/bin/sh\n")
            .expect("write fake vitest binary");
        let (plan, candidate) = detect_vitest(&root);
        assert!(plan.is_none(), "an installed binary alone is not intent");
        assert!(!candidate.available);
        assert!(
            candidate
                .reason
                .contains("no package-owned vitest declaration or config"),
            "{}",
            candidate.reason
        );
        assert!(plan.is_none(), "installed executable cannot create a plan");
    }

    /// A declared `test.dir` narrows the run and says so in the evidence.
    #[test]
    fn detect_vitest_narrows_the_command_to_a_declared_test_dir() {
        let root = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-vitest-dir-{}",
            std::process::id()
        ))
        .expect("fixture directory");
        std::fs::write(
            root.join("vitest.config.ts"),
            "export default { test: { dir: 'src/unit tests' } }\n",
        )
        .expect("write config");

        let (plan, candidate) = detect_vitest(&root);
        let plan = plan.expect("a config file is intent");
        assert_eq!(plan.command, "vitest run --dir 'src/unit tests'");
        assert_eq!(candidate.command, plan.command);
        assert!(
            plan.evidence
                .iter()
                .any(|line| line.contains("test.dir is `src/unit tests`")),
            "{:?}",
            plan.evidence
        );
    }

    /// A config that cannot be tokenised keeps the suite runnable but says the
    /// run is not narrowed. Before the typed scan the failure was silent: the
    /// command silently widened to the repository root with no evidence note.
    #[test]
    fn detect_vitest_reports_an_unreadable_config_instead_of_widening_silently() {
        let root = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-vitest-unreadable-{}",
            std::process::id()
        ))
        .expect("fixture directory");
        std::fs::write(
            root.join("vitest.config.ts"),
            "export default { test: { dir: 'src } }\n",
        )
        .expect("write unterminated config");

        let (plan, candidate) = detect_vitest(&root);
        let plan = plan.expect("a config file is intent even when it cannot be read");
        assert_eq!(
            plan.command, "vitest run",
            "an unreadable config cannot narrow the run"
        );
        assert!(
            plan.evidence.iter().any(|line| {
                line.contains("could not be read as JavaScript")
                    && line.contains("unterminated string literal")
                    && line.contains("not narrowed to a `test.dir`")
            }),
            "the evidence names the cause and the effect: {:?}",
            plan.evidence
        );
        assert!(
            candidate.reason.contains("not narrowed"),
            "the candidate reason carries the note: {}",
            candidate.reason
        );
    }

    /// A Vite config without a readable `test` block is not Vitest intent.
    #[test]
    fn detect_vitest_ignores_a_vite_config_without_a_test_block() {
        let root = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-vitest-vite-{}",
            std::process::id()
        ))
        .expect("fixture directory");
        std::fs::write(
            root.join("vite.config.ts"),
            "export default { plugins: [] }\n",
        )
        .expect("write vite config");

        let (plan, candidate) = detect_vitest(&root);
        assert!(plan.is_none());
        assert!(!candidate.available);
    }
}
