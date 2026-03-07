use super::ConfigDocProfile;

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
