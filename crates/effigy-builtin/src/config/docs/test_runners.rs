use super::ConfigDocProfile;

const SECTION_TEST_CORE: &[&str] = &[
    "[test]",
    "# Built-in test fanout and execution behavior.",
    "max_parallel = 3",
    "# cargo env auto-apply matcher: executable-only|prefix-aware|shell-aware",
    "cargo_env_match = \"prefix-aware\"",
    "# Catalog aliases omitted from root test fanout; direct alias/test still works.",
    "exclude_catalogs = [\"legacy\"]",
    "",
    "[test.suites]",
    "# Optional named suite commands used as source of truth.",
    "unit = \"bun x vitest run\"",
    "integration = \"cargo nextest run\"",
    "",
    "[test.suites.managed]",
    "# Optional lifecycle-aware suite example for managed test environments.",
    "run = [{ task = \"db:test:prepare\" }, \"cargo nextest run --workspace\"]",
    "# Set false for a focused suite that only runs when named explicitly.",
    "default = false",
    "env = \"managed-test\"",
    "env_file = [\".env\", \".env.test\"]",
    "setup = [{ run = \"cargo run -p app-db --bin migrate_test_db\" }]",
    "teardown = [{ run = \"cargo run -p app-db --bin reset_test_db\" }]",
    "teardown_policy = \"always\"",
    "",
];

const RUNNER_COMMENT: &str = "# Per-runner command overrides for built-in detection.";

const REFERENCE_RUNNER_LINES: &[&str] = &[
    "vitest = \"bun x vitest run\"",
    "\"cargo-nextest\" = \"cargo nextest run --workspace\"",
    "\"cargo-test\" = \"cargo test --workspace\"",
];

const SCHEMA_RUNNER_LINES: &[&str] = &[
    "vitest = \"bun x vitest run\"",
    "\"cargo-nextest\" = \"cargo nextest run\"",
    "\"cargo-test\" = \"cargo test\"",
];

const REFERENCE_VITEST_RUNNER_EXAMPLE: &[&str] = &[
    "[test.runners.vitest]",
    "# Optional nested override example for a single runner.",
    "command = \"bun x vitest run\"",
    "",
];

fn runner_value_line(profile: ConfigDocProfile, runner: &str) -> Option<&'static str> {
    match (profile, runner) {
        (_, "vitest") => Some("vitest = \"bun x vitest run\""),
        (ConfigDocProfile::Reference, "cargo-nextest") => {
            Some("\"cargo-nextest\" = \"cargo nextest run --workspace\"")
        }
        (ConfigDocProfile::Schema, "cargo-nextest") => {
            Some("\"cargo-nextest\" = \"cargo nextest run\"")
        }
        (ConfigDocProfile::Reference, "cargo-test") => {
            Some("\"cargo-test\" = \"cargo test --workspace\"")
        }
        (ConfigDocProfile::Schema, "cargo-test") => Some("\"cargo-test\" = \"cargo test\""),
        _ => None,
    }
}

pub(super) fn test_section_lines(
    include_core: bool,
    profile: ConfigDocProfile,
    runner: Option<&str>,
) -> Vec<&'static str> {
    let mut lines = Vec::<&'static str>::new();
    if include_core {
        lines.extend(SECTION_TEST_CORE.iter().copied());
    }
    lines.push("[test.runners]");
    lines.push(RUNNER_COMMENT);

    match runner {
        Some(name) => {
            if let Some(line) = runner_value_line(profile, name) {
                lines.push(line);
            }
        }
        None => match profile {
            ConfigDocProfile::Reference => lines.extend(REFERENCE_RUNNER_LINES.iter().copied()),
            ConfigDocProfile::Schema => lines.extend(SCHEMA_RUNNER_LINES.iter().copied()),
        },
    }
    lines.push("");

    if profile == ConfigDocProfile::Reference && runner.is_none() {
        lines.extend(REFERENCE_VITEST_RUNNER_EXAMPLE.iter().copied());
    }

    lines
}
