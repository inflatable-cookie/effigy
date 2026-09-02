pub(super) const COMPLETION_CANDIDATES_SUBCOMMAND: &str = "candidates";
pub(super) const COMPLETION_SHELL_TARGETS_QUOTED: &str = "`bash`, `zsh`, or `fish`";
pub(super) const COMPLETION_ACTION_TARGETS_QUOTED: &str = "`--install` or `--export`";
pub(super) const COMPLETION_TARGETS_WITH_CANDIDATES_QUOTED: &str =
    "`bash`, `zsh`, `fish`, or `candidates`";
pub(super) const COMPLETION_HELP_USAGE_LINE: &str =
    "effigy admin config completion [<bash|zsh|fish>] [--install|--export] [--json]";
pub(super) const COMPLETION_CANDIDATES_USAGE_LINE: &str =
    "effigy admin config completion candidates [--repo <path>] [--prefix <value>] [--json]";
pub(super) const COMPLETION_CANDIDATES_EXAMPLE_LINE: &str =
    "effigy admin config completion candidates --prefix farm";

pub(super) const COMPLETION_COMMAND_OPTIONS: &[&str] = &[
    "bash",
    "zsh",
    "fish",
    COMPLETION_CANDIDATES_SUBCOMMAND,
    "--install",
    "--export",
    "--repo",
    "--prefix",
    "--json",
    "--help",
    "-h",
];
