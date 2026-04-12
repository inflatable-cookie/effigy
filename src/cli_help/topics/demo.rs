use crate::ui::{Renderer, UiResult};

use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_demo_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("demo Help")?;
    renderer.text("Discover, inspect, execute, and control the repo-owned demo registry.")?;
    renderer.text("")?;

    render_info_notices(
        renderer,
        &[
            "Use `effigy demo browser` for the first interactive proof browser, `effigy demo list` for direct CLI discovery, `effigy demo inspect <DEMO_ID>` to inspect one record in detail, and `effigy demo run <DEMO_ID>` to record a new normalized attempt before `stop` or `rerun` when lifecycle control exists for that demo. Inside the browser, `←` and `→` switch between the demo list and the detail/artifact pane, `↑` and `↓` act inside the focused panel, `Enter` opens the action sheet from the list or opens the selected artifact from the detail side, `Esc` closes or quits, `/` edits search, and `f` opens the single filter sheet for owner/tag/mode/cover/status/gap/stale/grouping controls.",
        ],
    )?;

    render_usage_section(
        renderer,
        &[
            "effigy demo browser [--group-by <FIELD>] [--repo <PATH>]",
            "effigy demo list [--search <TEXT>] [--owner <NAME>] [--tag <TAG>] [--mode <MODE>] [--cover <AREA>] [--status <STATUS>] [--gap <GAP>] [--stale-only] [--group-by <FIELD>] [--repo <PATH>] [--json]",
            "effigy demo inspect <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy demo history <DEMO_ID> [--limit <N>] [--outcome <OUTCOME>] [--attempt <ATTEMPT_ID> | --ordinal <N>] [--repo <PATH>] [--json]",
            "effigy demo run <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy demo stop <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy demo rerun <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy --json demo list [--search <TEXT>] [--owner <NAME>] [--tag <TAG>] [--mode <MODE>] [--cover <AREA>] [--status <STATUS>] [--gap <GAP>] [--stale-only] [--group-by <FIELD>] [--repo <PATH>]",
            "effigy --json demo inspect <DEMO_ID> [--repo <PATH>]",
            "effigy --json demo history <DEMO_ID> [--limit <N>] [--outcome <OUTCOME>] [--attempt <ATTEMPT_ID> | --ordinal <N>] [--repo <PATH>]",
            "effigy --json demo run <DEMO_ID> [--repo <PATH>]",
            "effigy --json demo stop <DEMO_ID> [--repo <PATH>]",
            "effigy --json demo rerun <DEMO_ID> [--repo <PATH>]",
        ],
    )?;

    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Run against a different repo root"),
            (
                "--group-by <FIELD>",
                "Open the browser with list grouping by owner, tag, mode, cover, status, or gap",
            ),
            ("--search <TEXT>", "Filter demos by id, title, or summary text"),
            ("--owner <NAME>", "Filter demos by owner"),
            ("--tag <TAG>", "Filter demos by tag"),
            (
                "--mode <MODE>",
                "Filter demos by mode: headless, interactive, or hybrid",
            ),
            ("--cover <AREA>", "Filter demos by a declared coverage key"),
            (
                "--status <STATUS>",
                "Filter demos by current browser status: planned, ready, running, passed, failed, broken, or missing",
            ),
            (
                "--gap <GAP>",
                "Filter demos by gap class: existing, planned, missing, broken, or stale",
            ),
            (
                "--stale-only",
                "Show only demos whose latest recorded proof is stale",
            ),
            (
                "--group-by <FIELD>",
                "Group list output by owner, tag, mode, cover, status, or gap",
            ),
            (
                "--limit <N>",
                "Limit the number of retained history entries rendered for `demo history`",
            ),
            (
                "--outcome <OUTCOME>",
                "Filter `demo history` to retained outcomes: passed, failed, or terminated",
            ),
            (
                "--attempt <ATTEMPT_ID>",
                "Inspect one retained historical attempt in detail from `demo history`",
            ),
            (
                "--ordinal <N>",
                "Inspect the Nth retained attempt from the current `demo history` result set",
            ),
            (
                "--json",
                "Render machine-readable demo discovery, inspection, or run payloads",
            ),
            ("-h, --help", "Print demo command help"),
        ],
    )?;

    render_bullet_section(
        renderer,
        "Examples",
        "example",
        &[
            "effigy demo browser",
            "effigy demo browser --group-by status",
            "effigy demo list",
            "effigy demo list --owner auth --status ready",
            "effigy demo list --group-by owner --stale-only",
            "effigy demo inspect plugin-capability-browser",
            "effigy demo history login-smoke --limit 5",
            "effigy demo history login-smoke --outcome failed",
            "effigy demo history login-smoke --attempt login-smoke-1775944053944",
            "effigy demo history login-smoke --outcome terminated --ordinal 1",
            "effigy demo run login-smoke",
            "effigy demo stop login-smoke",
            "effigy demo rerun login-smoke",
            "effigy --json demo inspect login-smoke",
        ],
    )?;

    Ok(())
}
