use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{Renderer, UiResult};
use crate::HelpTopic;

pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    match topic {
        HelpTopic::General => render_general_help(renderer),
        HelpTopic::Doctor => render_doctor_help(renderer),
        HelpTopic::Tasks => render_tasks_help(renderer),
        HelpTopic::Test => render_test_help(renderer),
        HelpTopic::Watch => render_watch_help(renderer),
        HelpTopic::Init => render_init_help(renderer),
        HelpTopic::Migrate => render_migrate_help(renderer),
    }
}

pub fn render_cli_header<R: Renderer>(renderer: &mut R, root: &Path) -> UiResult<()> {
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let color_mode = std::env::var("EFFIGY_COLOR")
        .ok()
        .unwrap_or_else(|| "auto".to_owned());
    let use_color = !no_color && color_mode != "never";

    let title_line = "EFFIGY".to_owned();
    let path_line = root.display().to_string();
    let combined_line = format!("{title_line}  {path_line}");
    let version = format!(" v{} ", env!("CARGO_PKG_VERSION"));
    let inner_width = combined_line.len();
    let top = format!("╭{}╮", "─".repeat(inner_width + 2));
    let middle = format!("│ {:<width$} │", combined_line, width = inner_width);
    let bottom_fill = (inner_width + 2).saturating_sub(version.len());
    let bottom = format!("╰{}{}╯", "─".repeat(bottom_fill), version);

    renderer.text("")?;
    if use_color {
        let theme = Theme::default();
        let accent = theme.accent;
        let accent_soft = theme.accent_soft;
        let muted = theme.muted;
        let accent_on = format!("{}", accent.render());
        let accent_soft_on = format!("{}", accent_soft.render());
        let muted_on = format!("{}", muted.render());
        let reset = format!("{}", accent.render_reset());
        let spacer = "  ";
        let trailing =
            inner_width.saturating_sub(title_line.len() + spacer.len() + path_line.len());
        let trailing_spaces = " ".repeat(trailing);

        renderer.text(&format!("{accent_on}{top}{reset}"))?;
        renderer.text(&format!(
            "{accent_on}│ {reset}{accent_on}{title_line}{reset}{muted_on}{spacer}{path_line}{trailing_spaces}{reset}{accent_on} │{reset}"
        ))?;
        renderer.text(&format!(
            "{accent_on}╰{}{reset}{accent_soft_on}{version}{reset}{accent_on}╯{reset}",
            "─".repeat(bottom_fill)
        ))?;
    } else {
        renderer.text(&top)?;
        renderer.text(&middle)?;
        renderer.text(&bottom)?;
    }
    renderer.text("")?;
    Ok(())
}

fn render_general_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("Commands")?;
    renderer.table(&crate::ui::TableSpec::new(
        Vec::new(),
        vec![
            vec![
                "effigy help".to_owned(),
                "Show general help (same as --help)".to_owned(),
            ],
            vec![
                "effigy tasks".to_owned(),
                "List discovered catalogs/task commands and probe routing".to_owned(),
            ],
            vec![
                "effigy config".to_owned(),
                "Show supported effigy.toml configuration keys and examples".to_owned(),
            ],
            vec![
                "effigy doctor".to_owned(),
                "Run remedial-first health checks for environment, manifests, and task references"
                    .to_owned(),
            ],
            vec![
                "effigy test".to_owned(),
                "Run built-in auto-detected tests (or explicit tasks.test); supports <catalog>/test fallback".to_owned(),
            ],
            vec![
                "effigy watch".to_owned(),
                "Watch mode phase-1 runtime with explicit owner policy and debounce/glob controls"
                    .to_owned(),
            ],
            vec![
                "effigy init".to_owned(),
                "Initialize baseline effigy.toml scaffold with safe overwrite/dry-run controls"
                    .to_owned(),
            ],
            vec![
                "effigy migrate".to_owned(),
                "Migrate package scripts into `[tasks]` with preview/apply flow".to_owned(),
            ],
            vec![
                "effigy unlock".to_owned(),
                "Manually clear lock scopes (`workspace`, `task:*`, `profile:*/*`)".to_owned(),
            ],
            vec![
                "effigy cache".to_owned(),
                "Inspect/invalidate phase-1 task cache metadata (`inspect`, `invalidate`)"
                    .to_owned(),
            ],
            vec![
                "effigy <task>".to_owned(),
                "Resolve task across discovered catalogs".to_owned(),
            ],
            vec![
                "effigy <catalog>/<task>".to_owned(),
                "Run task from explicit catalog alias".to_owned(),
            ],
        ],
    ))?;
    renderer.text("")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Use `effigy <built-in-task> --help` for task-specific flags and examples.",
    )?;
    renderer.key_values(&[
        crate::ui::KeyValue::new("-h, --help", "Print this help panel"),
        crate::ui::KeyValue::new("--json", "Render command-envelope JSON for CI/tooling"),
    ])?;
    Ok(())
}

fn render_doctor_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("doctor Help")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Run remediation-first health checks for environment tooling, manifest validity, and task references.",
    )?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Explain task resolution with `effigy doctor <task> <args>`.",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text("effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]")?;
    renderer.text("effigy doctor <task> <args> [--json]")?;
    renderer.text("")?;

    renderer.section("Options")?;
    renderer.table(&crate::ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned(),
            ],
            vec![
                "--fix".to_owned(),
                "Apply safe automatic remediations when available".to_owned(),
            ],
            vec![
                "--verbose".to_owned(),
                "Include expanded per-finding detail in text output".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable doctor report payload".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy doctor".to_owned(),
            "effigy doctor --repo /path/to/workspace".to_owned(),
            "effigy doctor --fix".to_owned(),
            "effigy doctor --verbose".to_owned(),
            "effigy doctor farmyard/build -- --watch".to_owned(),
            "effigy --json doctor --repo /path/to/workspace".to_owned(),
        ],
    )?;
    Ok(())
}

fn render_tasks_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("tasks Help")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "List discovered task catalogs and task commands; use routing probes only when debugging selector resolution.",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text(
        "effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]",
    )?;
    renderer.text("")?;

    renderer.section("Options")?;
    renderer.table(&crate::ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned(),
            ],
            vec![
                "--task <TASK_NAME>".to_owned(),
                "Filter output to matching task entries".to_owned(),
            ],
            vec![
                "--resolve <SELECTOR>".to_owned(),
                "Probe task routing evidence for a selector (for example `<catalog>/task` or `test`)"
                    .to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable task catalog payload".to_owned(),
            ],
            vec![
                "--pretty <true|false>".to_owned(),
                "When used with --json, toggle pretty formatting (default: true)".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy tasks".to_owned(),
            "effigy tasks --repo /path/to/workspace".to_owned(),
            "effigy tasks --repo /path/to/workspace --task db:reset".to_owned(),
            "effigy tasks --resolve <catalog>/<task>".to_owned(),
            "effigy tasks --json --resolve test".to_owned(),
            "effigy --json tasks --repo /path/to/workspace --task test".to_owned(),
        ],
    )?;
    Ok(())
}

fn render_test_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("test Help")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Run built-in test runner detection by default (including <catalog>/test fallback).",
    )?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "If `tasks.test` is defined, it takes precedence over built-in detection.",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text("effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]")?;
    renderer.text("effigy test --help")?;
    renderer.text("")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "When multiple suites are detected and runner args are provided, prefix the suite explicitly (for example `effigy test vitest my-test`).",
    )?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "If `[test.suites]` is defined in effigy.toml, those suites are used as source of truth and auto-detection is skipped.",
    )?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Use `effigy test --plan ...` and check `available-suites` per target before running filtered tests.",
    )?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "When suite names are mistyped or unavailable, effigy suggests nearest suite names and copy-paste retry commands.",
    )?;
    renderer.text("")?;

    renderer.section("Options")?;
    renderer.table(&crate::ui::TableSpec::new(
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
    renderer.text("Task-ref chain with quoted args:")?;
    renderer.text("[tasks.validate]")?;
    renderer
        .text("run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Task-ref chain parsing is shell-like tokenization only; Effigy does not perform shell expansion inside `task = \"...\"` values.",
    )?;
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
            "effigy test --plan user-service".to_owned(),
            "effigy test --plan viteest user-service".to_owned(),
            "effigy test --verbose-results".to_owned(),
            "effigy test --tui".to_owned(),
            "effigy test -- --runInBand".to_owned(),
            "effigy test -- --watch".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Named Test Selection")?;
    renderer.bullet_list(
        "patterns",
        &[
            "effigy test user-service".to_owned(),
            "effigy test vitest user-service".to_owned(),
            "effigy test viteest user-service  # suggests vitest".to_owned(),
            "effigy <catalog>/test billing".to_owned(),
            "effigy test -- tests/api/user.test.ts".to_owned(),
            "effigy test -- user_service --nocapture".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Error Recovery")?;
    renderer.bullet_list(
        "modes",
        &[
            "Ambiguity: `effigy test user-service` in multi-suite repos fails and suggests suite-first retries.".to_owned(),
            "Unavailable or mistyped suite: `effigy test viteest user-service` fails with nearest suite name and a copy-paste command.".to_owned(),
        ],
    )?;
    renderer.text("")?;

    renderer.section("Migration")?;
    renderer.bullet_list(
        "before/after",
        &[
            "before: effigy test user-service (ambiguous in multi-suite repos)".to_owned(),
            "after: effigy test vitest user-service".to_owned(),
            "after: effigy test nextest user_service --nocapture".to_owned(),
            "after: effigy test viteest user-service -> suggests `effigy test vitest user-service`"
                .to_owned(),
        ],
    )?;
    Ok(())
}

fn render_watch_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("watch Help")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Run file-triggered reruns for non-watcher tasks with explicit watch-owner policy controls.",
    )?;
    renderer.text("")?;
    renderer.section("Usage")?;
    renderer.text(
        "effigy watch --owner <effigy|external> [--debounce-ms <MS>] [--include <GLOB>] [--exclude <GLOB>] <task> [task args]",
    )?;
    renderer.text("effigy watch --owner effigy --once <task> [task args]")?;
    renderer.text("")?;
    renderer.section("Options")?;
    renderer.table(&crate::ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--owner <effigy|external>".to_owned(),
                "Required owner policy. `effigy` enables file-triggered reruns; `external` blocks nested loops and expects task-managed watching.".to_owned(),
            ],
            vec![
                "--debounce-ms <MS>".to_owned(),
                "Debounce quiet window before rerunning after detected changes (default: 400)."
                    .to_owned(),
            ],
            vec![
                "--include <GLOB>".to_owned(),
                "Optional repeatable include glob set (defaults to all files).".to_owned(),
            ],
            vec![
                "--exclude <GLOB>".to_owned(),
                "Optional repeatable exclude glob set, merged with default excludes (`.git/**`, `node_modules/**`, `target/**`).".to_owned(),
            ],
            vec![
                "--once".to_owned(),
                "Run target once with watch policy checks, then exit (useful for CI/contracts)."
                    .to_owned(),
            ],
            vec![
                "--max-runs <N>".to_owned(),
                "Stop after N executions (useful for bounded automation/testing).".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render JSON payload for bounded runs (`--once` or `--max-runs`).".to_owned(),
            ],
            vec![
                "lock scope".to_owned(),
                "Effigy owner mode acquires `task:watch:<target>`; clear manually with `effigy unlock task:watch:<target>` when needed.".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;
    renderer.section("Phase-1 Scope")?;
    renderer.bullet_list(
        "phase-1 scope",
        &[
            "file-triggered reruns for non-watcher tasks".to_owned(),
            "explicit watch-owner policy safeguards".to_owned(),
            "debounce and include/exclude glob controls".to_owned(),
            "fail-fast guidance when owner policy indicates external watcher ownership".to_owned(),
        ],
    )?;
    Ok(())
}

fn render_init_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("init Help")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Generate a baseline `effigy.toml` scaffold with minimal defaults and commented examples.",
    )?;
    renderer.text("")?;
    renderer.section("Usage")?;
    renderer.text("effigy init [--dry-run] [--force] [--json]")?;
    renderer.text("")?;
    renderer.section("Options")?;
    renderer.table(&crate::ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--dry-run".to_owned(),
                "Print scaffold content without writing to disk.".to_owned(),
            ],
            vec![
                "--force".to_owned(),
                "Overwrite existing `effigy.toml` if present.".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable init report payload.".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;
    renderer.section("Phase-1 Scope")?;
    renderer.bullet_list(
        "init scope",
        &[
            "generate minimal valid effigy.toml".to_owned(),
            "include commented DAG and managed task examples".to_owned(),
            "safe file existence handling (`--dry-run`/`--force`)".to_owned(),
        ],
    )?;
    Ok(())
}

fn render_migrate_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("migrate Help")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "Import `package.json` scripts into `[tasks]` with preview-first, explicit apply flow.",
    )?;
    renderer.text("")?;
    renderer.section("Usage")?;
    renderer.text("effigy migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]")?;
    renderer.text("")?;
    renderer.section("Options")?;
    renderer.table(&crate::ui::TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--from <PATH>".to_owned(),
                "Override source package file (default: <repo>/package.json).".to_owned(),
            ],
            vec![
                "--script <NAME>".to_owned(),
                "Repeatable script selector filter (defaults to all scripts).".to_owned(),
            ],
            vec![
                "--apply".to_owned(),
                "Write ready imports into `[tasks]` (preview-only by default).".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable migration report payload.".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;
    renderer.section("Phase-1 Scope")?;
    renderer.bullet_list(
        "phase-1 scope",
        &[
            "import package.json scripts only".to_owned(),
            "preview + explicit apply flow".to_owned(),
            "non-destructive source preservation".to_owned(),
            "manual remediation hints for task-name conflicts".to_owned(),
        ],
    )?;
    Ok(())
}
