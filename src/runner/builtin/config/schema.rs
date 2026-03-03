pub(super) fn normalize_test_runner_name(value: &str) -> Option<&'static str> {
    match value {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

const HEADER_CANONICAL: &str = "# Canonical strict-valid effigy.toml schema template";
const HEADER_MINIMAL: &str = "# Minimal strict-valid effigy.toml starter";
const RUNNER_COMMENT: &str = "# Per-runner command overrides for built-in detection.";

const SECTION_PACKAGE_MANAGER: &[&str] = &[
    "[package_manager]",
    "# Preferred JS/TS package manager for built-in test runners.",
    "js = \"bun\"",
    "",
];

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

const SECTION_TASKS_MINIMAL: &[&str] = &[
    "[tasks]",
    "# Compact task command mappings.",
    "test = \"bun x vitest run\"",
    "",
];

const SECTION_TASKS_CANONICAL: &[&str] = &[
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
    "  { run = \"my-other-arbitrary-process\", start = 4, tab = 1 }",
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
    "# Phase-1 cache contract: explicit opt-in only, no implicit discovery.",
    "enabled = true",
    "inputs = [\"src/**/*.rs\", \"Cargo.toml\"]",
    "outputs = [\"target/build-artifact\"]",
    "env = [\"RUSTFLAGS\", \"NODE_ENV\"]",
    "",
];

fn runner_lines(runner: Option<&str>) -> Vec<&'static str> {
    match runner {
        Some("vitest") => vec!["vitest = \"bun x vitest run\""],
        Some("cargo-nextest") => vec!["\"cargo-nextest\" = \"cargo nextest run\""],
        Some("cargo-test") => vec!["\"cargo-test\" = \"cargo test\""],
        Some(_) => Vec::new(),
        None => vec![
            "vitest = \"bun x vitest run\"",
            "\"cargo-nextest\" = \"cargo nextest run\"",
            "\"cargo-test\" = \"cargo test\"",
        ],
    }
}

fn section_test_lines(minimal: bool, runner: Option<&str>) -> Vec<String> {
    let mut lines = Vec::<String>::new();
    if !minimal {
        lines.extend(
            [
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
            ]
            .into_iter()
            .map(str::to_owned),
        );
    }

    lines.push("[test.runners]".to_owned());
    lines.push(RUNNER_COMMENT.to_owned());
    lines.extend(runner_lines(runner).into_iter().map(str::to_owned));
    lines.push(String::new());
    lines
}

fn join_lines(lines: &[String]) -> String {
    lines.join("\n")
}

fn prefixed_section(header: &str, section_lines: &[&str]) -> String {
    let mut lines = vec![header.to_owned(), String::new()];
    lines.extend(section_lines.iter().copied().map(str::to_owned));
    join_lines(&lines)
}

pub(super) fn render_builtin_config_schema() -> String {
    let mut lines = vec![HEADER_CANONICAL.to_owned(), String::new()];
    lines.extend(SECTION_PACKAGE_MANAGER.iter().copied().map(str::to_owned));
    lines.extend(section_test_lines(false, None));
    lines.extend(SECTION_DEFER.iter().copied().map(str::to_owned));
    lines.extend(SECTION_SHELL.iter().copied().map(str::to_owned));
    lines.extend(SECTION_TASKS_CANONICAL.iter().copied().map(str::to_owned));
    join_lines(&lines)
}

pub(super) fn render_builtin_config_schema_minimal() -> String {
    let mut lines = vec![HEADER_MINIMAL.to_owned(), String::new()];
    lines.extend(SECTION_PACKAGE_MANAGER.iter().copied().map(str::to_owned));
    lines.extend(section_test_lines(true, Some("vitest")));
    lines.extend(SECTION_TASKS_MINIMAL.iter().copied().map(str::to_owned));
    join_lines(&lines)
}

pub(super) fn render_builtin_config_schema_target(target: &str, minimal: bool) -> Option<String> {
    let header_prefix = if minimal {
        "# Minimal strict-valid effigy.toml starter"
    } else {
        "# Canonical strict-valid effigy.toml schema template"
    };

    let rendered = match (target, minimal) {
        ("package_manager", true) | ("package_manager", false) => prefixed_section(
            &format!("{header_prefix} (package_manager target)"),
            SECTION_PACKAGE_MANAGER,
        ),
        ("tasks", true) => prefixed_section(
            &format!("{header_prefix} (tasks target)"),
            SECTION_TASKS_MINIMAL,
        ),
        ("tasks", false) => prefixed_section(
            &format!("{header_prefix} (tasks target)"),
            SECTION_TASKS_CANONICAL,
        ),
        ("defer", true) | ("defer", false) => {
            prefixed_section(&format!("{header_prefix} (defer target)"), SECTION_DEFER)
        }
        ("shell", true) | ("shell", false) => {
            prefixed_section(&format!("{header_prefix} (shell target)"), SECTION_SHELL)
        }
        _ => return None,
    };

    Some(rendered)
}

pub(super) fn render_builtin_config_schema_test_target(
    minimal: bool,
    runner: Option<&str>,
) -> String {
    let header = match (minimal, runner) {
        (true, Some(name)) => {
            format!("# Minimal strict-valid effigy.toml starter (test target, runner: {name})")
        }
        (true, None) => "# Minimal strict-valid effigy.toml starter (test target)".to_owned(),
        (false, Some(name)) => {
            format!("# Canonical strict-valid effigy.toml schema template (test target, runner: {name})")
        }
        (false, None) => {
            "# Canonical strict-valid effigy.toml schema template (test target)".to_owned()
        }
    };

    let mut lines = vec![header, String::new()];
    lines.extend(section_test_lines(minimal, runner));
    join_lines(&lines)
}
