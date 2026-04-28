use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_bootstrap_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "bootstrap",
        &[
            "Bootstrap a repo into the current working tree from a git URL, then follow the repo-owned `[bootstrap]` contract.",
            "Phase 1 ships root clone/update, optional submodule sync, child repo checkout, bootstrap-local `run` steps, and automatic `[bootstrap].start` execution unless `--no-start` is passed.",
        ],
        &[
            "effigy bootstrap <GIT_URL> [--path <DIR>] [--branch <NAME>] [--no-start] [--plan] [--json]",
            "effigy --json bootstrap <GIT_URL> --plan",
        ],
        &[
            (
                "--path <DIR>",
                "Override the destination path instead of `./<repo-name>`",
            ),
            (
                "--branch <NAME>",
                "Override the initial branch/ref target for clone/update",
            ),
            (
                "--start",
                "Force the repo's configured bootstrap start task to run after bootstrap setup completes",
            ),
            (
                "--no-start",
                "Skip the repo's configured bootstrap start task after bootstrap setup completes",
            ),
            (
                "--plan",
                "Preview the resolved bootstrap request without clone/update execution",
            ),
            ("--json", "Render machine-readable bootstrap payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy bootstrap git@github.com:inflatable-cookie/loophole.git --plan",
            "effigy bootstrap https://github.com/inflatable-cookie/loophole.git --path ./loophole --plan",
            "effigy bootstrap git@github.com:inflatable-cookie/loophole.git --branch main --no-start --plan",
        ],
    )
}
