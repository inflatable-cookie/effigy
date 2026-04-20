use super::super::{HelpRenderer, HelpResult, KeyValue, NoticeLevel};

pub(crate) fn render_system_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.notice(
        NoticeLevel::Info,
        "Operate the manifest default system substrate by resolving its default workspace container.",
    )?;
    renderer.section("Usage")?;
    renderer.text(
        "effigy system <up|down|status|logs|repair> [--system <NAME>] [--repo <PATH>] [--json]",
    )?;
    renderer.text("")?;
    renderer.key_values(&[
        KeyValue::new(
            "--system <NAME>",
            "Select one explicit manifest system instead of `[systems].default`",
        ),
        KeyValue::new("--repo <PATH>", "Override target repository path"),
        KeyValue::new("--follow", "For `logs`, keep streaming system logs"),
        KeyValue::new(
            "--json",
            "Render machine-readable output for non-interactive subcommands",
        ),
        KeyValue::new("-h, --help", "Print command help"),
    ])?;
    renderer.text("")?;
    renderer.section("Examples")?;
    renderer.text("effigy system up")?;
    renderer.text("effigy system status")?;
    renderer.text("effigy system logs --follow")?;
    renderer.text("effigy system repair")?;
    renderer.text("effigy system down --system dev")?;
    Ok(())
}
