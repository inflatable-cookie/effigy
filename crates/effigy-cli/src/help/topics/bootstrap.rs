use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    option_rows, render_standard_topic_help_spec, text_lines, CommonOption, StandardTopicHelpSpec,
};

pub(crate) fn render_bootstrap_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &BOOTSTRAP_HELP)
}

const BOOTSTRAP_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "bootstrap",
    notices: text_lines![
        "The one command for downloading and starting someone else's project.",
        "It clones the repo (or uses this checkout when no URL is given), materialises the pinned bundle, catalog and `[sources.<name>]` sources, initialises submodules when `[bootstrap] submodules = true`, runs the repo's `setup` selectors in order, then starts its `start` selector.",
        "`setup` and `start` are ordinary task selectors, so environment, gateway, readiness, secrets and cleanup behave exactly as they do for `effigy <selector>`.",
        "Nothing resets a working copy: an existing destination, pinned source or submodule with local changes or at another commit is reported as drift and left untouched.",
    ],
    usage: text_lines![
        "effigy bootstrap [<GIT_URL>] [--path <DIR>] [--branch <NAME>] [--no-start] [--plan] [--json]",
        "effigy bootstrap",
        "effigy --json bootstrap <GIT_URL> --plan",
    ],
    leading_common_options: &[],
    options: option_rows![
        "--path <DIR>" => "Use this destination directory instead of the URL-derived clone directory",
        "--branch <NAME>" => "Clone or update this branch/ref instead of the remote default",
        "--no-start" => "Run the repo's `setup` selectors but skip its `start` selector",
        "--plan" => "List the clone, sources, submodules, setup and start without changing anything",
    ],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable `effigy.bootstrap.v1` payloads"),
        CommonOption::Help,
    ],
    examples: text_lines![
        "effigy bootstrap git@github.com:acme/app.git --plan",
        "effigy bootstrap https://github.com/acme/app.git --path ./app --plan",
        "effigy bootstrap git@github.com:Cumberland-BS/cbs.git --branch main",
        "effigy bootstrap",
        "effigy bootstrap --no-start",
    ],
};
