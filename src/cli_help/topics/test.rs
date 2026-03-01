use crate::ui::{NoticeLevel, Renderer, TableSpec, UiResult};

pub(crate) fn render_test_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("test Help")?;
    renderer.notice(
        NoticeLevel::Info,
        "Run built-in test runner detection by default (including <catalog>/test fallback).",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "If `tasks.test` is defined, it takes precedence over built-in detection.",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text("effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]")?;
    renderer.text("effigy test --help")?;
    renderer.text("")?;
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

    renderer.section("Options")?;
    renderer.table(&TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--plan".to_owned(),
                "Print per-target detection plan and fallback chain without executing".to_owned(),
            ],
            vec![
                "--verbose-results".to_owned(),
                "Include runner/root/command fields in Test Results output".to_owned(),
            ],
            vec![
                "--tui".to_owned(),
                "Force TUI mode when interactive (auto-enabled when multiple suites are detected)"
                    .to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Detection Order")?;
    renderer.bullet_list(
        "runners",
        &[
            "vitest (package/config/bin markers)".to_owned(),
            "cargo nextest run (when Cargo.toml exists and cargo-nextest is available)".to_owned(),
            "cargo test (Rust fallback)".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Configuration")?;
    renderer.text("Root manifest (fanout concurrency):")?;
    renderer.text("[package_manager]")?;
    renderer.text("js = \"bun\"  # optional: bun|pnpm|npm|direct")?;
    renderer.text("[test]")?;
    renderer.text("max_parallel = 2")?;
    renderer.text("[test.suites]")?;
    renderer.text("unit = \"bun x vitest run\"")?;
    renderer.text("integration = \"cargo nextest run\"")?;
    renderer.text("[test.runners]")?;
    renderer.text("vitest = \"bun x vitest run\"")?;
    renderer.text("\"cargo-nextest\" = \"cargo nextest run --workspace\"")?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy test".to_owned(),
            "effigy test vitest".to_owned(),
            "effigy test nextest user_service --nocapture".to_owned(),
            "effigy <catalog>/test".to_owned(),
            "effigy test --plan".to_owned(),
            "effigy test --verbose-results".to_owned(),
            "effigy test --tui".to_owned(),
            "effigy test -- --runInBand".to_owned(),
        ],
    )?;
    Ok(())
}
