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
    let mut unknown = Vec::<String>::new();
    for arg in args {
        match arg.as_str() {
            "--schema" => schema = true,
            "--minimal" => minimal = true,
            _ => unknown.push(arg.clone()),
        }
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
    if schema {
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
