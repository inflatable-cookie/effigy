use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{NoticeLevel, Renderer, UiResult};

pub(crate) fn render_test_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("test Help")?;
    render_info_notices(
        renderer,
        &[
            "Run built-in test runner detection by default (including <catalog>/test fallback).",
            "If `tasks.test` is defined, it takes precedence over built-in detection.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]",
            "effigy test --help",
        ],
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "When multiple suites are detected and runner args are provided, prefix the suite explicitly (for example `effigy test vitest my-test`).",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "If `[test.suites]` is defined in effigy.toml, those suites are used as source of truth and auto-detection is skipped.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy test --plan ...` and check `available-suites` per target before running filtered tests.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "When suite names are mistyped or unavailable, effigy suggests nearest suite names and copy-paste retry commands.",
    )?;
    renderer.text("")?;

    render_options_section(
        renderer,
        &[
            (
                "--plan",
                "Print per-target detection plan and fallback chain without executing",
            ),
            (
                "--verbose-results",
                "Include runner/root/command fields in Test Results output",
            ),
            (
                "--tui",
                "Force TUI mode when interactive (auto-enabled when multiple suites are detected)",
            ),
            ("-h, --help", "Print command help"),
        ],
    )?;

    render_bullet_section(
        renderer,
        "Detection Order",
        "runners",
        &[
            "vitest (package/config/bin markers)",
            "cargo nextest run (when Cargo.toml exists and cargo-nextest is available)",
            "cargo test (Rust fallback)",
        ],
    )?;
    renderer.text("")?;

    renderer.section("Configuration")?;
    renderer.text("Root manifest (fanout concurrency):")?;
    renderer.text("[package_manager]")?;
    renderer.text("js = \"bun\"  # optional: bun|pnpm|npm|direct")?;
    renderer.text("[test]")?;
    renderer.text("max_parallel = 2")?;
    renderer
        .text("cargo_env_match = \"prefix-aware\"  # executable-only|prefix-aware|shell-aware")?;
    renderer.text("[test.suites]")?;
    renderer.text("unit = \"bun x vitest run\"")?;
    renderer.text("integration = \"cargo nextest run\"")?;
    renderer.text("[test.runners]")?;
    renderer.text("vitest = \"bun x vitest run\"")?;
    renderer.text("\"cargo-nextest\" = \"cargo nextest run --workspace\"")?;
    renderer.text("")?;
    renderer.text("Task-ref chain with quoted args:")?;
    renderer.text("[tasks.validate]")?;
    renderer
        .text("run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]")?;
    renderer.notice(
        NoticeLevel::Info,
        "Task-ref chain parsing is shell-like tokenization only; Effigy does not perform shell expansion inside `task = \"...\"` values.",
    )?;
    renderer.text("")?;

    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy test",
            "effigy test vitest",
            "effigy test vitest user-service",
            "effigy test nextest user_service --nocapture",
            "effigy <catalog>/test",
            "effigy test --plan",
            "effigy test --plan user-service",
            "effigy test --plan viteest user-service",
            "effigy test --verbose-results",
            "effigy test --tui",
            "effigy test -- --runInBand",
            "effigy test -- --watch",
        ],
    )?;
    renderer.text("")?;

    render_bullet_section(
        renderer,
        "Named Test Selection",
        "patterns",
        &[
            "effigy test user-service",
            "effigy test vitest user-service",
            "effigy test viteest user-service  # suggests vitest",
            "effigy <catalog>/test billing",
            "effigy test -- tests/api/user.test.ts",
            "effigy test -- user_service --nocapture",
        ],
    )?;
    renderer.text("")?;

    render_bullet_section(
        renderer,
        "Error Recovery",
        "modes",
        &[
            "Ambiguity: `effigy test user-service` in multi-suite repos fails and suggests suite-first retries.",
            "Unavailable or mistyped suite: `effigy test viteest user-service` fails with nearest suite name and a copy-paste command.",
        ],
    )?;
    renderer.text("")?;

    render_bullet_section(
        renderer,
        "Migration",
        "before/after",
        &[
            "before: effigy test user-service (ambiguous in multi-suite repos)",
            "after: effigy test vitest user-service",
            "after: effigy test nextest user_service --nocapture",
            "after: effigy test viteest user-service -> suggests `effigy test vitest user-service`",
        ],
    )?;
    Ok(())
}
