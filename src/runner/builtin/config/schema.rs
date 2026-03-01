pub(super) fn normalize_test_runner_name(value: &str) -> Option<&'static str> {
    match value {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

pub(super) fn render_builtin_config_schema() -> String {
    [
        "# Canonical strict-valid effigy.toml schema template",
        "",
        "[package_manager]",
        "# Preferred JS/TS package manager for built-in test runners.",
        "js = \"bun\"",
        "",
        "[test]",
        "# Built-in test fanout and execution behavior.",
        "max_parallel = 3",
        "",
        "[test.suites]",
        "# Optional named suite commands used as source of truth.",
        "unit = \"bun x vitest run\"",
        "integration = \"cargo nextest run\"",
        "",
        "[test.runners]",
        "# Per-runner command overrides for built-in detection.",
        "vitest = \"bun x vitest run\"",
        "\"cargo-nextest\" = \"cargo nextest run\"",
        "\"cargo-test\" = \"cargo test\"",
        "",
        "[defer]",
        "# Fallback command for unresolved task requests.",
        "run = \"my-process {request} {args}\"",
        "",
        "[shell]",
        "# Interactive shell command used by managed shell tabs.",
        "run = \"exec ${SHELL:-/bin/zsh} -i\"",
        "",
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
        "[tasks.build.cache]",
        "# Phase-1 cache contract: explicit opt-in only, no implicit discovery.",
        "enabled = true",
        "inputs = [\"src/**/*.rs\", \"Cargo.toml\"]",
        "outputs = [\"target/build-artifact\"]",
        "env = [\"RUSTFLAGS\", \"NODE_ENV\"]",
        "",
    ]
    .join("\n")
}

pub(super) fn render_builtin_config_schema_minimal() -> String {
    [
        "# Minimal strict-valid effigy.toml starter",
        "",
        "[package_manager]",
        "# Preferred JS/TS package manager for built-in test runners.",
        "js = \"bun\"",
        "",
        "[test.runners]",
        "# Per-runner command overrides for built-in detection.",
        "vitest = \"bun x vitest run\"",
        "",
        "[tasks]",
        "# Compact task command mappings.",
        "test = \"bun x vitest run\"",
        "",
    ]
    .join("\n")
}

pub(super) fn render_builtin_config_schema_target(target: &str, minimal: bool) -> Option<String> {
    match (target, minimal) {
        ("package_manager", true) => Some(
            [
                "# Minimal strict-valid effigy.toml starter (package_manager target)",
                "",
                "[package_manager]",
                "# Preferred JS/TS package manager for built-in test runners.",
                "js = \"bun\"",
                "",
            ]
            .join("\n"),
        ),
        ("tasks", true) => Some(
            [
                "# Minimal strict-valid effigy.toml starter (tasks target)",
                "",
                "[tasks]",
                "# Compact task command mappings.",
                "test = \"bun x vitest run\"",
                "",
            ]
            .join("\n"),
        ),
        ("defer", true) => Some(
            [
                "# Minimal strict-valid effigy.toml starter (defer target)",
                "",
                "[defer]",
                "# Fallback command for unresolved task requests.",
                "run = \"my-process {request} {args}\"",
                "",
            ]
            .join("\n"),
        ),
        ("shell", true) => Some(
            [
                "# Minimal strict-valid effigy.toml starter (shell target)",
                "",
                "[shell]",
                "# Interactive shell command used by managed shell tabs.",
                "run = \"exec ${SHELL:-/bin/zsh} -i\"",
                "",
            ]
            .join("\n"),
        ),
        ("package_manager", false) => Some(
            [
                "# Canonical strict-valid effigy.toml schema template (package_manager target)",
                "",
                "[package_manager]",
                "# Preferred JS/TS package manager for built-in test runners.",
                "js = \"bun\"",
                "",
            ]
            .join("\n"),
        ),
        ("tasks", false) => Some(
            [
                "# Canonical strict-valid effigy.toml schema template (tasks target)",
                "",
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
                "[tasks.build.cache]",
                "# Phase-1 cache contract: explicit opt-in only, no implicit discovery.",
                "enabled = true",
                "inputs = [\"src/**/*.rs\", \"Cargo.toml\"]",
                "outputs = [\"target/build-artifact\"]",
                "env = [\"RUSTFLAGS\", \"NODE_ENV\"]",
                "",
            ]
            .join("\n"),
        ),
        ("defer", false) => Some(
            [
                "# Canonical strict-valid effigy.toml schema template (defer target)",
                "",
                "[defer]",
                "# Fallback command for unresolved task requests.",
                "run = \"my-process {request} {args}\"",
                "",
            ]
            .join("\n"),
        ),
        ("shell", false) => Some(
            [
                "# Canonical strict-valid effigy.toml schema template (shell target)",
                "",
                "[shell]",
                "# Interactive shell command used by managed shell tabs.",
                "run = \"exec ${SHELL:-/bin/zsh} -i\"",
                "",
            ]
            .join("\n"),
        ),
        _ => None,
    }
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
    if !minimal {
        lines.push("[test]".to_owned());
        lines.push("# Built-in test fanout and execution behavior.".to_owned());
        lines.push("max_parallel = 3".to_owned());
        lines.push(String::new());
        lines.push("[test.suites]".to_owned());
        lines.push("# Optional named suite commands used as source of truth.".to_owned());
        lines.push("unit = \"bun x vitest run\"".to_owned());
        lines.push("integration = \"cargo nextest run\"".to_owned());
        lines.push(String::new());
    }
    lines.push("[test.runners]".to_owned());
    lines.push("# Per-runner command overrides for built-in detection.".to_owned());
    match runner {
        Some("vitest") => lines.push("vitest = \"bun x vitest run\"".to_owned()),
        Some("cargo-nextest") => lines.push("\"cargo-nextest\" = \"cargo nextest run\"".to_owned()),
        Some("cargo-test") => lines.push("\"cargo-test\" = \"cargo test\"".to_owned()),
        Some(_) => {}
        None => {
            lines.push("vitest = \"bun x vitest run\"".to_owned());
            lines.push("\"cargo-nextest\" = \"cargo nextest run\"".to_owned());
            lines.push("\"cargo-test\" = \"cargo test\"".to_owned());
        }
    }
    lines.push(String::new());
    lines.join("\n")
}
