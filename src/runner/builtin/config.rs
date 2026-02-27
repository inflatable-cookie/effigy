use std::io::IsTerminal;

use crate::ui::theme::resolve_color_enabled;
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
            return Ok(Some(selected));
        }
        if runner.is_some() {
            return Err(RunnerError::TaskInvocation(
                "`--runner` requires `--target test` for built-in `config`".to_owned(),
            ));
        }
        return Ok(Some(if minimal {
            render_builtin_config_schema_minimal()
        } else {
            render_builtin_config_schema()
        }));
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
    renderer.text("run = \"composer global exec effigy -- {request} {args}\"")?;
    renderer.text("")?;
    renderer.text("[shell]")?;
    renderer.text("run = \"exec ${SHELL:-/bin/zsh} -i\"")?;
    renderer.text("")?;

    renderer.section("Built-in Test")?;
    renderer.text("[package_manager]")?;
    renderer.text("js = \"pnpm\"  # applies to JS/TS tooling")?;
    renderer.text("")?;
    renderer.text("[test]")?;
    renderer.text("max_parallel = 3")?;
    renderer.text("")?;
    renderer.text("[test.runners]")?;
    renderer.text("vitest = \"pnpm exec vitest run\"")?;
    renderer.text("\"cargo-nextest\" = \"cargo nextest run --workspace\"")?;
    renderer.text("\"cargo-test\" = \"cargo test --workspace\"")?;
    renderer.text("")?;
    renderer.text("[test.runners.vitest]")?;
    renderer.text("command = \"bun x vitest run\"")?;
    renderer.text("")?;

    renderer.section("Tasks")?;
    renderer.text("[tasks]")?;
    renderer.text("api = \"cargo run -p api\"")?;
    renderer.text("reset-db = [\"sqlx database reset -y\", \"sqlx migrate run\"]")?;
    renderer.text("")?;
    renderer.text("[tasks.test]")?;
    renderer.text("run = \"bun test {args}\"")?;
    renderer.text("")?;
    renderer.text("[tasks.dev]")?;
    renderer.text("mode = \"managed\"")?;
    renderer.text("fail_on_non_zero = true")?;
    renderer.text("")?;
    renderer.text("[tasks.dev.processes.api]")?;
    renderer.text("run = \"cargo run -p api\"")?;
    renderer.text("")?;
    renderer.text("[tasks.dev.profiles.default]")?;
    renderer.text("start = [\"api\"]")?;
    renderer.text("tabs = [\"api\"]")?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Compact tasks entries are shorthand for `run` commands; use table form for managed mode, profiles, and shell flags.",
    )?;

    let out = renderer.into_inner();
    String::from_utf8(out)
        .map(Some)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
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
        "js = \"pnpm\"",
        "",
        "[test]",
        "max_parallel = 3",
        "",
        "[test.runners]",
        "vitest = \"pnpm exec vitest run\"",
        "\"cargo-nextest\" = \"cargo nextest run\"",
        "\"cargo-test\" = \"cargo test\"",
        "",
        "[defer]",
        "run = \"composer global exec effigy -- {request} {args}\"",
        "",
        "[shell]",
        "run = \"exec ${SHELL:-/bin/zsh} -i\"",
        "",
        "[tasks]",
        "api = \"cargo run -p api\"",
        "reset-db = [\"sqlx database reset -y\", \"sqlx migrate run\"]",
        "",
        "[tasks.test]",
        "run = \"bun test {args}\"",
        "",
        "[tasks.dev]",
        "mode = \"managed\"",
        "fail_on_non_zero = true",
        "",
        "[tasks.dev.processes.api]",
        "run = \"cargo run -p api\"",
        "",
        "[tasks.dev.profiles.default]",
        "start = [\"api\"]",
        "tabs = [\"api\"]",
        "",
    ]
    .join("\n")
}

fn render_builtin_config_schema_minimal() -> String {
    [
        "# Minimal strict-valid effigy.toml starter",
        "",
        "[package_manager]",
        "js = \"pnpm\"",
        "",
        "[test.runners]",
        "vitest = \"pnpm exec vitest run\"",
        "",
        "[tasks]",
        "test = \"vitest run\"",
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
                "js = \"pnpm\"",
                "",
            ]
            .join("\n"),
        ),
        ("tasks", true) => Some(
            [
                "# Minimal strict-valid effigy.toml starter (tasks target)",
                "",
                "[tasks]",
                "test = \"vitest run\"",
                "",
            ]
            .join("\n"),
        ),
        ("defer", true) => Some(
            [
                "# Minimal strict-valid effigy.toml starter (defer target)",
                "",
                "[defer]",
                "run = \"composer global exec effigy -- {request} {args}\"",
                "",
            ]
            .join("\n"),
        ),
        ("shell", true) => Some(
            [
                "# Minimal strict-valid effigy.toml starter (shell target)",
                "",
                "[shell]",
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
                "js = \"pnpm\"",
                "",
            ]
            .join("\n"),
        ),
        ("tasks", false) => Some(
            [
                "# Canonical strict-valid effigy.toml schema template (tasks target)",
                "",
                "[tasks]",
                "api = \"cargo run -p api\"",
                "reset-db = [\"sqlx database reset -y\", \"sqlx migrate run\"]",
                "",
                "[tasks.test]",
                "run = \"bun test {args}\"",
                "",
                "[tasks.dev]",
                "mode = \"managed\"",
                "fail_on_non_zero = true",
                "",
                "[tasks.dev.processes.api]",
                "run = \"cargo run -p api\"",
                "",
                "[tasks.dev.profiles.default]",
                "start = [\"api\"]",
                "tabs = [\"api\"]",
                "",
            ]
            .join("\n"),
        ),
        ("defer", false) => Some(
            [
                "# Canonical strict-valid effigy.toml schema template (defer target)",
                "",
                "[defer]",
                "run = \"composer global exec effigy -- {request} {args}\"",
                "",
            ]
            .join("\n"),
        ),
        ("shell", false) => Some(
            [
                "# Canonical strict-valid effigy.toml schema template (shell target)",
                "",
                "[shell]",
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
        lines.push("max_parallel = 3".to_owned());
        lines.push(String::new());
    }
    lines.push("[test.runners]".to_owned());
    match runner {
        Some("vitest") => lines.push("vitest = \"pnpm exec vitest run\"".to_owned()),
        Some("cargo-nextest") => lines.push("\"cargo-nextest\" = \"cargo nextest run\"".to_owned()),
        Some("cargo-test") => lines.push("\"cargo-test\" = \"cargo test\"".to_owned()),
        Some(_) => {}
        None => {
            lines.push("vitest = \"pnpm exec vitest run\"".to_owned());
            lines.push("\"cargo-nextest\" = \"cargo nextest run\"".to_owned());
            lines.push("\"cargo-test\" = \"cargo test\"".to_owned());
        }
    }
    lines.push(String::new());
    lines.join("\n")
}
