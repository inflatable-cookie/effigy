use std::io::IsTerminal;

use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;

use super::super::RunnerError;

pub(super) fn run_builtin_config(
    task: &TaskInvocation,
    args: &[String],
) -> Result<Option<String>, RunnerError> {
    let mut schema = false;
    let mut minimal = false;
    let mut target: Option<String> = None;
    let mut runner: Option<String> = None;
    let mut unknown = Vec::<String>::new();
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--schema" => schema = true,
            "--minimal" => minimal = true,
            "--target" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--target` requires a value for built-in `config`".to_owned(),
                    ));
                };
                target = Some(value.to_lowercase());
                i += 1;
            }
            "--runner" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--runner` requires a value for built-in `config`".to_owned(),
                    ));
                };
                runner = Some(value.to_lowercase());
                i += 1;
            }
            _ => unknown.push(arg.clone()),
        }
        i += 1;
    }

    if !unknown.is_empty() {
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            unknown.join(" ")
        )));
    }
    if minimal && !schema {
        return Err(RunnerError::TaskInvocation(
            "`--minimal` requires `--schema` for built-in `config`".to_owned(),
        ));
    }
    if target.is_some() && !schema {
        return Err(RunnerError::TaskInvocation(
            "`--target` requires `--schema` for built-in `config`".to_owned(),
        ));
    }
    if runner.is_some() && !schema {
        return Err(RunnerError::TaskInvocation(
            "`--runner` requires `--schema` for built-in `config`".to_owned(),
        ));
    }
    if runner.is_some() && target.as_deref() != Some("test") {
        return Err(RunnerError::TaskInvocation(
            "`--runner` requires `--target test` for built-in `config`".to_owned(),
        ));
    }
    if schema {
        let color_enabled =
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
        if let Some(section) = target.as_deref() {
            let selected = if section == "test" {
                let normalized_runner = match runner.as_deref() {
                    Some(value) => Some(normalize_test_runner_name(value).ok_or_else(|| {
                        RunnerError::TaskInvocation(format!(
                            "invalid `--runner` value `{value}` for built-in `config` (supported: vitest, cargo-nextest, cargo-test)"
                        ))
                    })?),
                    None => None,
                };
                render_builtin_config_schema_test_target(minimal, normalized_runner)
            } else {
                render_builtin_config_schema_target(section, minimal).ok_or_else(|| {
                    RunnerError::TaskInvocation(format!(
                        "invalid `--target` value `{section}` for built-in `config` (supported: package_manager, test, tasks, defer, shell)"
                    ))
                })?
            };
            return Ok(Some(style_schema_comments(selected, color_enabled)));
        }
        if runner.is_some() {
            return Err(RunnerError::TaskInvocation(
                "`--runner` requires `--target test` for built-in `config`".to_owned(),
            ));
        }
        let rendered = if minimal {
            render_builtin_config_schema_minimal()
        } else {
            render_builtin_config_schema()
        };
        return Ok(Some(style_schema_comments(rendered, color_enabled)));
    }

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("effigy.toml Reference")?;
    renderer.notice(
        NoticeLevel::Info,
        "Supported project-level configuration keys for task execution and built-in test behavior",
    )?;
    renderer.text("")?;

    renderer.section("Global")?;
    renderer.text("[defer]")?;
    renderer.text(&muted_comment(color_enabled, "# Fallback command for unresolved task requests."))?;
    renderer.text("run = \"my-process {request} {args}\"")?;
    renderer.text("")?;
    renderer.text("[shell]")?;
    renderer.text(&muted_comment(color_enabled, "# Interactive shell command used by managed shell tabs."))?;
    renderer.text("run = \"exec ${SHELL:-/bin/zsh} -i\"")?;
    renderer.text("")?;

    renderer.section("Built-in Test")?;
    renderer.text("[package_manager]")?;
    renderer.text(&muted_comment(color_enabled, "# Preferred JS/TS package manager for built-in test runners."))?;
    renderer.text("js = \"bun\"  # applies to JS/TS tooling")?;
    renderer.text("")?;
    renderer.text("[test]")?;
    renderer.text(&muted_comment(color_enabled, "# Built-in test fanout and execution behavior."))?;
    renderer.text("max_parallel = 3")?;
    renderer.text("")?;
    renderer.text("[test.suites]")?;
    renderer.text(&muted_comment(color_enabled, "# Optional named suite commands used as source of truth."))?;
    renderer.text("unit = \"bun x vitest run\"")?;
    renderer.text("integration = \"cargo nextest run\"")?;
    renderer.text("")?;
    renderer.text("[test.runners]")?;
    renderer.text(&muted_comment(color_enabled, "# Per-runner command overrides for built-in detection."))?;
    renderer.text("vitest = \"bun x vitest run\"")?;
    renderer.text("\"cargo-nextest\" = \"cargo nextest run --workspace\"")?;
    renderer.text("\"cargo-test\" = \"cargo test --workspace\"")?;
    renderer.text("")?;
    renderer.text("[test.runners.vitest]")?;
    renderer.text(&muted_comment(color_enabled, "# Optional nested override example for a single runner."))?;
    renderer.text("command = \"bun x vitest run\"")?;
    renderer.text("")?;

    renderer.section("Tasks")?;
    renderer.text("[tasks]")?;
    renderer.text(&muted_comment(color_enabled, "# Compact task command mappings."))?;
    renderer.text("api = \"cargo run -p api\"")?;
    renderer.text("\"db:reset\" = [\"sqlx database reset -y\", \"sqlx migrate run\"]")?;
    renderer.text("")?;
    renderer.text("[tasks.dev]")?;
    renderer.text(&muted_comment(color_enabled, "# Managed dev task configuration."))?;
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
    renderer.text(&muted_comment(color_enabled, "# Example task-ref chain combining built-ins and shell commands."))?;
    renderer
        .text("run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]")?;
    renderer.text("")?;

    let out = renderer.into_inner();
    String::from_utf8(out)
        .map(Some)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

fn muted_comment(color_enabled: bool, line: &str) -> String {
    if !color_enabled {
        return line.to_owned();
    }
    let style = Theme::default().muted;
    format!("{}{}{}", style.render(), line, style.render_reset())
}

fn style_schema_comments(schema: String, color_enabled: bool) -> String {
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

fn normalize_test_runner_name(value: &str) -> Option<&'static str> {
    match value {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

fn render_builtin_config_schema() -> String {
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
        "# Example task-ref chain combining built-ins and shell commands.",
        "run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]",
        "",
    ]
    .join("\n")
}

fn render_builtin_config_schema_minimal() -> String {
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

fn render_builtin_config_schema_target(target: &str, minimal: bool) -> Option<String> {
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
                "# Example task-ref chain combining built-ins and shell commands.",
                "run = [{ task = \"test vitest \\\"user service\\\"\" }, \"printf validate-ok\"]",
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

fn render_builtin_config_schema_test_target(minimal: bool, runner: Option<&str>) -> String {
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
