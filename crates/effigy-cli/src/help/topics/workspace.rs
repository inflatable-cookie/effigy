use super::super::{HelpRenderer, HelpResult, KeyValue, NoticeLevel};

pub(crate) fn render_workspace_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    renderer.notice(
        NoticeLevel::Info,
        "Ensure the selected system substrate is up, then open the resolved workspace shell.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Mounted sibling repos listed in `systems.<name>.mounts` auto-adopt any producer-declared `[isolation].paths` into the workspace container; the normal path does not require a duplicate isolation list.",
    )?;
    renderer.section("Usage")?;
    renderer.text("effigy local workspace [<NAME>] [--system <NAME>] [--repo <PATH>]")?;
    renderer.text("")?;
    renderer.section("Options")?;
    renderer.key_values(&[
        KeyValue::new(
            "<NAME>",
            "Optional workspace name; defaults to `[systems.<system>].default_workspace`, the sole declared workspace, or an implied `default` workspace",
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
    renderer.text("effigy local workspace")?;
    renderer.text("effigy local workspace admin")?;
    renderer.text("effigy local workspace jobs --system dev")?;
    Ok(())
}
