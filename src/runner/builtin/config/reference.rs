use crate::ui::theme::Theme;
use crate::ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::super::RunnerError;

pub(super) fn render_config_reference(color_enabled: bool) -> Result<String, RunnerError> {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("effigy.toml Reference")?;
    renderer.notice(
        NoticeLevel::Info,
        "Supported project-level configuration keys for task execution and built-in test behavior",
    )?;
    renderer.text("")?;

    renderer.section("Global")?;
    renderer.text("[defer]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Fallback command for unresolved task requests.",
    ))?;
    renderer.text("run = \"my-process {request} {args}\"")?;
    renderer.text("")?;
    renderer.text("[shell]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Interactive shell command used by managed shell tabs.",
    ))?;
    renderer.text("run = \"exec ${SHELL:-/bin/zsh} -i\"")?;
    renderer.text("")?;

    renderer.section("Built-in Test")?;
    renderer.text("[package_manager]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Preferred JS/TS package manager for built-in test runners.",
    ))?;
    renderer.text("js = \"bun\"  # applies to JS/TS tooling")?;
    renderer.text("")?;
    renderer.text("[test]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Built-in test fanout and execution behavior.",
    ))?;
    renderer.text("max_parallel = 3")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# cargo env auto-apply matcher: executable-only|prefix-aware|shell-aware",
    ))?;
    renderer.text("cargo_env_match = \"prefix-aware\"")?;
    renderer.text("")?;
    renderer.text("[test.suites]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Optional named suite commands used as source of truth.",
    ))?;
    renderer.text("unit = \"bun x vitest run\"")?;
    renderer.text("integration = \"cargo nextest run\"")?;
    renderer.text("")?;
    renderer.text("[test.runners]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Per-runner command overrides for built-in detection.",
    ))?;
    renderer.text("vitest = \"bun x vitest run\"")?;
    renderer.text("\"cargo-nextest\" = \"cargo nextest run --workspace\"")?;
    renderer.text("\"cargo-test\" = \"cargo test --workspace\"")?;
    renderer.text("")?;
    renderer.text("[test.runners.vitest]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Optional nested override example for a single runner.",
    ))?;
    renderer.text("command = \"bun x vitest run\"")?;
    renderer.text("")?;

    renderer.section("Tasks")?;
    renderer.text("[tasks]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Compact task command mappings.",
    ))?;
    renderer.text("api = \"cargo run -p api\"")?;
    renderer.text("\"db:reset\" = [\"sqlx database reset -y\", \"sqlx migrate run\"]")?;
    renderer.text("")?;
    renderer.text("[tasks.dev]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Managed dev task configuration.",
    ))?;
    renderer.text("mode = \"tui\"")?;
    renderer.text("fail_on_non_zero = true")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Concurrent launch plan with explicit start and tab ordering.",
    ))?;
    renderer.text("concurrent = [")?;
    renderer.text("  { task = \"catalog-a/api\", start = 1, tab = 3 },")?;
    renderer.text("  { task = \"catalog-a/jobs\", start = 2, tab = 4, start_after_ms = 1200 },")?;
    renderer.text("  { task = \"catalog-b/dev\", start = 3, tab = 2 },")?;
    renderer.text("  { run = \"my-other-arbitrary-process\", start = 4, tab = 1 }")?;
    renderer.text("]")?;
    renderer.text("")?;
    renderer.text("[tasks.dev.profiles.admin]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Optional profile-specific concurrent override.",
    ))?;
    renderer.text("concurrent = [")?;
    renderer.text("  { task = \"catalog-a/api\", start = 1, tab = 2 },")?;
    renderer.text("  { run = \"my-admin-process\", start = 2, tab = 1 }")?;
    renderer.text("]")?;
    renderer.text("")?;
    renderer.text("[tasks.validate]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Example DAG-style run sequence with explicit step ids and dependencies.",
    ))?;
    renderer.text(
        "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
    )?;
    renderer.text("")?;
    renderer.text("[env]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Reusable env entries for run-array directives (`{ env = \"<name>\" }` or `{ env = \"<catalog-path>/<name>\" }`).",
    ))?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Missing named entries fall back to process env, then <catalog-root>/.env.",
    ))?;
    renderer.text("CARGO_HOME = \"{project}/.effigy/cargo/home\"")?;
    renderer.text("CARGO_TARGET_DIR = \"{project}/.effigy/cargo/target\"")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Optional grouped profile form:",
    ))?;
    renderer.text(
        "cargo = [{ CARGO_HOME = \"{project}/.effigy/cargo/home\" }, { CARGO_TARGET_DIR = \"{project}/.effigy/cargo/target\" }]",
    )?;
    renderer.text("")?;
    renderer.text("[tasks.api]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Example run-array env directive: applies from this point forward in the chain.",
    ))?;
    renderer.text(
        "run = [{ env = \"CARGO_HOME\" }, { env = \"CARGO_TARGET_DIR\" }, { run = \"cargo run -p api\" }]",
    )?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Optional dotenv fallback override for this task:",
    ))?;
    renderer.text("env_file = \".env.test\"")?;
    renderer.text("env_file = [\".env.local\", \".env.test\"]")?;
    renderer.text("run = [{ env = \"DATABASE_URL\" }, { run = \"cargo test -p api\" }]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Or switch dotenv source mid-chain:",
    ))?;
    renderer.text(
        "run = [{ env_file = \".env.local\" }, { env = \"DATABASE_URL\" }, { task = \"migrate\" }]",
    )?;
    renderer.text("run = [{ env_file = [\".env.local\", \".env.test\"] }, { env = \"DATABASE_URL\" }, { task = \"migrate\" }]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Cross-catalog reference example (relative to current catalog root):",
    ))?;
    renderer.text("run = [{ env = \"../shared/CARGO_HOME\" }, { task = \"build\" }]")?;
    renderer.text("")?;
    renderer.text("[tasks.rust-build]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Task-local environment variables with {project}/{repo} path substitution.",
    ))?;
    renderer.text("run = \"cargo build -p api\"")?;
    renderer.text(
        "env = { CARGO_HOME = \"{project}/.effigy/cargo-home\", CARGO_TARGET_DIR = \"{project}/.effigy/cargo-target\" }",
    )?;
    renderer.text("")?;
    renderer.text("[tasks.build.cache]")?;
    renderer.text(&muted_comment(
        color_enabled,
        "# Phase-1 task cache contract: explicit opt-in declarations only.",
    ))?;
    renderer.text("enabled = true")?;
    renderer.text("inputs = [\"src/**/*.rs\", \"Cargo.toml\"]")?;
    renderer.text("outputs = [\"target/build-artifact\"]")?;
    renderer.text("env = [\"RUSTFLAGS\", \"NODE_ENV\"]")?;
    renderer.text("")?;

    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

pub(super) fn style_schema_comments(schema: String, color_enabled: bool) -> String {
    if !color_enabled {
        return schema;
    }
    let style = Theme::default().muted;
    schema
        .lines()
        .map(|line| {
            if line.starts_with('#') {
                format!("{}{}{}", style.render(), line, style.render_reset())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn muted_comment(color_enabled: bool, line: &str) -> String {
    if !color_enabled {
        return line.to_owned();
    }
    let style = Theme::default().muted;
    format!("{}{}{}", style.render(), line, style.render_reset())
}
