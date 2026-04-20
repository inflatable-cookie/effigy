use super::super::{HelpRenderer, HelpResult, KeyValue, NoticeLevel};

pub(crate) fn render_workspace_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.notice(
        NoticeLevel::Info,
        "Ensure the selected system substrate is up, then open the resolved workspace shell.",
    )?;
    renderer.section("Usage")?;
    renderer.text("effigy workspace [<NAME>] [--system <NAME>] [--repo <PATH>]")?;
    renderer.text("")?;
    renderer.key_values(&[
        KeyValue::new(
            "<NAME>",
            "Optional workspace name; defaults to `[systems.<system>].default_workspace` or the sole declared workspace",
        ),
        KeyValue::new(
            "--system <NAME>",
            "Select one explicit manifest system instead of `[systems].default` or the sole declared system",
        ),
        KeyValue::new("--repo <PATH>", "Override target repository path"),
        KeyValue::new("-h, --help", "Print command help"),
    ])?;
    renderer.text("")?;
    renderer.section("Examples")?;
    renderer.text("effigy workspace")?;
    renderer.text("effigy workspace admin")?;
    renderer.text("effigy workspace jobs --system dev")?;
    Ok(())
}
