use super::super::{HelpRenderer, HelpResult, KeyValue, NoticeLevel};

pub(crate) fn render_system_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    renderer.notice(
        NoticeLevel::Info,
        "Operate the manifest default system substrate by resolving its default workspace container.",
    )?;
    renderer.section("Usage")?;
    renderer.text(
        "effigy local system <up|down|status|logs|repair|reset-runtime> [--system <NAME>] [--repo <PATH>] [--follow] [--json]",
    )?;
    renderer.text("")?;
    renderer.section("Options")?;
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
    renderer.text("effigy local system up")?;
    renderer.text("effigy local system status")?;
    renderer.text("effigy local system logs --follow")?;
    renderer.text("effigy local system repair")?;
    renderer.text("effigy local system reset-runtime")?;
    renderer.text("effigy local system down --system dev")?;
    Ok(())
}
