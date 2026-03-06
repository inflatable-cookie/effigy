#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigDocProfile {
    Reference,
    Schema,
}

const SECTION_DEFER: &[&str] = &[
    "[defer]",
    "# Fallback command for unresolved task requests.",
    "run = \"my-process {request} {args}\"",
    "",
];

const SECTION_SHELL: &[&str] = &[
    "[shell]",
    "# Interactive shell command used by managed shell tabs.",
    "run = \"exec ${SHELL:-/bin/zsh} -i\"",
    "",
];

const SECTION_SCAN: &[&str] = &[
    "[scan.god_files]",
    "# Oversized code-file scanner thresholds use code-only lines.",
    "warn = 250",
    "high = 400",
    "critical = 700",
    "# Include this scanner in `effigy doctor` by default.",
    "doctor = true",
    "fail_on_findings = false",
    "respect_gitignore = true",
    "# Optional glob overrides.",
    "include = [\"src/**\", \"app/**\"]",
    "exclude = [\"docs/**\", \"dist/**\", \"coverage/**\"]",
    "",
    "[scan.generated_assets]",
    "# Bulky vendored/generated asset scanner thresholds use bytes.",
    "warn = 1000000",
    "high = 5000000",
    "critical = 20000000",
    "# Include this scanner in `effigy doctor` by default.",
    "doctor = true",
    "fail_on_findings = false",
    "respect_gitignore = true",
    "# Optional glob overrides.",
    "include = [\"dist/**\", \"vendor/**\"]",
    "exclude = [\"docs/**\"]",
    "",
];

const SECTION_TASKS_MINIMAL: &[&str] = &[
    "[tasks]",
    "# Compact task command mappings.",
    "test = \"bun x vitest run\"",
    "",
];

const SECTION_TASKS_CANONICAL_PREFIX: &[&str] = &[
    "[tasks]",
    "# Compact task command mappings.",
    "api = \"cargo run -p api\"",
    "\"db:reset\" = [\"sqlx database reset -y\", \"sqlx migrate run\"]",
    "",
    "[tasks.dev]",
    "# Managed dev task configuration.",
    "mode = \"tui\"",
    "fail_on_non_zero = true",
    "# Concurrent launch plan with explicit start and tab ordering.",
    "concurrent = [",
    "  { task = \"catalog-a/api\", start = 1, tab = 3 },",
    "  { task = \"catalog-a/jobs\", start = 2, tab = 4, start_after_ms = 1200 },",
    "  { task = \"catalog-b/dev\", start = 3, tab = 2 },",
    "  { run = \"my-other-arbitrary-process\", start = 4, tab = 1 },",
    "  { task = \"shell\", start = 5, tab = 5 }",
    "]",
    "",
    "[tasks.dev.profiles.admin]",
    "# Optional profile-specific concurrent override.",
    "concurrent = [",
    "  { task = \"catalog-a/api\", start = 1, tab = 2 },",
    "  { run = \"my-admin-process\", start = 2, tab = 1 }",
    "]",
    "",
    "[tasks.validate]",
    "# Example DAG-style run sequence with explicit step ids and dependencies.",
    "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
    "",
    "[env]",
    "# Reusable env entries for run-array directives (`{ env = \"<name>\" }` or `{ env = \"<catalog-path>/<name>\" }`).",
    "# Missing named entries fall back to process env, then <catalog-root>/.env.",
    "CARGO_HOME = \"{project}/.effigy/cargo/home\"",
    "CARGO_TARGET_DIR = \"{project}/.effigy/cargo/target\"",
    "# Optional grouped profile form:",
    "cargo = [{ CARGO_HOME = \"{project}/.effigy/cargo/home\" }, { CARGO_TARGET_DIR = \"{project}/.effigy/cargo/target\" }]",
    "",
    "[tasks.api]",
    "# Example run-array env directive: applies from this point forward in the chain.",
    "run = [{ env = \"CARGO_HOME\" }, { env = \"CARGO_TARGET_DIR\" }, { run = \"cargo run -p api\" }]",
    "# Optional dotenv fallback override for this task:",
    "env_file = \".env.test\"",
    "env_file = [\".env.local\", \".env.test\"]",
    "run = [{ env = \"DATABASE_URL\" }, { run = \"cargo test -p api\" }]",
    "# Or switch dotenv source mid-chain:",
    "run = [{ env_file = \".env.local\" }, { env = \"DATABASE_URL\" }, { task = \"migrate\" }]",
    "run = [{ env_file = [\".env.local\", \".env.test\"] }, { env = \"DATABASE_URL\" }, { task = \"migrate\" }]",
    "# Cross-catalog reference example (relative to current catalog root):",
    "run = [{ env = \"../shared/CARGO_HOME\" }, { task = \"build\" }]",
    "",
    "[tasks.rust-build]",
    "# Task-local environment variables with {project}/{repo} path substitution.",
    "run = \"cargo build -p api\"",
    "env = { CARGO_HOME = \"{project}/.effigy/cargo-home\", CARGO_TARGET_DIR = \"{project}/.effigy/cargo-target\" }",
    "",
    "[tasks.build.cache]",
];

const SECTION_TASKS_CANONICAL_SUFFIX: &[&str] = &[
    "enabled = true",
    "inputs = [\"src/**/*.rs\", \"Cargo.toml\"]",
    "outputs = [\"target/build-artifact\"]",
    "env = [\"RUSTFLAGS\", \"NODE_ENV\"]",
    "",
];

const SECTION_TEST_CORE: &[&str] = &[
    "[test]",
    "# Built-in test fanout and execution behavior.",
    "max_parallel = 3",
    "# cargo env auto-apply matcher: executable-only|prefix-aware|shell-aware",
    "cargo_env_match = \"prefix-aware\"",
    "",
    "[test.suites]",
    "# Optional named suite commands used as source of truth.",
    "unit = \"bun x vitest run\"",
    "integration = \"cargo nextest run\"",
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

fn tasks_cache_comment(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => {
            "# Phase-1 task cache contract: explicit opt-in declarations only."
        }
        ConfigDocProfile::Schema => {
            "# Phase-1 cache contract: explicit opt-in only, no implicit discovery."
        }
    }
}

fn js_package_manager_line(profile: ConfigDocProfile) -> &'static str {
    match profile {
        ConfigDocProfile::Reference => "js = \"bun\"  # applies to JS/TS tooling",
        ConfigDocProfile::Schema => "js = \"bun\"",
    }
}

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

pub(super) fn defer_lines() -> &'static [&'static str] {
    SECTION_DEFER
}

pub(super) fn shell_lines() -> &'static [&'static str] {
    SECTION_SHELL
}

pub(super) fn scan_lines() -> &'static [&'static str] {
    SECTION_SCAN
}

pub(super) fn tasks_minimal_lines() -> &'static [&'static str] {
    SECTION_TASKS_MINIMAL
}

pub(super) fn package_manager_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    vec![
        "[package_manager]",
        "# Preferred JS/TS package manager for built-in test runners.",
        js_package_manager_line(profile),
        "",
    ]
}

pub(super) fn tasks_canonical_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    let mut lines = Vec::with_capacity(
        SECTION_TASKS_CANONICAL_PREFIX.len() + 1 + SECTION_TASKS_CANONICAL_SUFFIX.len(),
    );
    lines.extend(SECTION_TASKS_CANONICAL_PREFIX.iter().copied());
    lines.push(tasks_cache_comment(profile));
    lines.extend(SECTION_TASKS_CANONICAL_SUFFIX.iter().copied());
    lines
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

#[cfg(test)]
#[path = "docs/tests.rs"]
mod tests;
