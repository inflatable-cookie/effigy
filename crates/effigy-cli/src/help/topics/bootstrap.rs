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
        "Bootstrap a repo into the current working tree from a git URL, then follow the repo-owned `[bootstrap]` contract.",
        "Phase 1 ships root clone/update, optional submodule sync, child repo checkout, bootstrap-local `run` steps, and automatic `[bootstrap].start` execution unless `--no-start` is passed.",
        "Use `--fresh` when you need an isolated throwaway runtime namespace for bootstrap testing. Pair it with `effigy bootstrap teardown` afterward to clean up the session-scoped volumes and runtime.",
    ],
    usage: text_lines![
        "effigy bootstrap <GIT_URL> [--path <DIR>] [--branch <NAME>] [--backend <containerd|docker>] [--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>]... [--fresh] [--no-prompt] [--reuse-path] [--no-start] [--plan] [--json]",
        "effigy bootstrap children status [--json]",
        "effigy bootstrap children sync [--fetch-only] [--checkout] [--json]",
        "effigy bootstrap teardown [--yes] [--json]",
        "effigy --json bootstrap <GIT_URL> --plan",
    ],
    leading_common_options: &[],
    options: option_rows![
        "--path <DIR>" => "Override the destination path instead of the default clone directory",
        "--branch <NAME>" => "Override the initial branch/ref target for clone/update",
        "--backend <containerd|docker>" => "Force bootstrap to use a specific container backend for this session instead of ambient detection or machine defaults; on a real TTY Effigy also prompts when both are available and uses any saved machine preference as the default choice",
        "--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>" => "Stage one or more SQL dumps or explicit `oci://` artifacts into the cloned repo for bootstrap-owned database seeding; multi-database bundles require named targets, and a bare target reads `./<target>.sql`",
        "--fresh" => "Append a session-scoped suffix to generated-compose project names during bootstrap so volumes and runtime state stay isolated from prior local runs",
        "--no-prompt" => "Disable interactive bootstrap prompts for missing database seed inputs even on a real TTY; pair with --reuse-path to reuse a non-empty destination non-interactively",
        "--reuse-path" => "Reuse a non-empty bootstrap destination without interactive confirmation",
        "--start" => "Force the repo's configured bootstrap start task to run after bootstrap setup completes",
        "--no-start" => "Skip the repo's configured bootstrap start task after bootstrap setup completes",
        "--plan" => "Preview the resolved bootstrap request without clone/update execution",
        "--fetch-only" => "Fetch existing bootstrap child checkouts without pulling",
        "--checkout" => "Allow bootstrap children sync to switch a child checkout to its configured branch before fast-forwarding",
        "--yes" => "Confirm bootstrap session teardown non-interactively",
    ],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable bootstrap payloads"),
        CommonOption::Help,
    ],
    examples: text_lines![
        "effigy bootstrap git@github.com:inflatable-cookie/loophole.git --plan",
        "effigy bootstrap https://github.com/inflatable-cookie/loophole.git --path ./loophole --plan",
        "effigy bootstrap git@github.com:inflatable-cookie/loophole.git --branch main --no-start --plan",
        "effigy bootstrap git@github.com:inflatable-cookie/loophole.git --backend docker --fresh --no-start",
        "effigy bootstrap git@github.com:inflatable-cookie/legacy.git --db-seed ./backups/latest.sql --start",
        "effigy bootstrap git@github.com:inflatable-cookie/legacy.git --db-seed app=oci://ghcr.io/acme/private-data:uat --start",
        "effigy bootstrap git@github.com:acowtancy/market.git --db-seed legacy_mysql --start",
        "effigy bootstrap git@github.com:Cumberland-BS/cbs.git --db-seed cbs=./backups/cbs.sql --db-seed cbs-mortcalc=./backups/cbs-mortcalc.sql --start",
        "effigy bootstrap git@github.com:acowtancy/market.git --fresh --no-start",
        "effigy bootstrap children status",
        "effigy bootstrap children sync",
        "effigy bootstrap children sync --fetch-only --json",
        "effigy bootstrap teardown --yes",
    ],
};
