use std::io::IsTerminal;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{NoticeLevel, OutputMode, PlainRenderer, Renderer};

use super::super::RunnerError;

pub(super) fn run_builtin_config() -> Result<Option<String>, RunnerError> {
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
