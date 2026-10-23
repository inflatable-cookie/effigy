use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_fmt_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &FMT_HELP)
}

const FMT_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "fmt",
    notices: &[
        "Format only the files you name. Effigy runs rustfmt on each of them with `skip_children`, so a linked or path-dependent workspace is never reformatted.",
        "Relative file paths are resolved from the directory `effigy` runs in, not from the repository root. Every path must be an existing `.rs` file inside the target repository.",
        "Each file is formatted with the rustfmt configuration nearest to it, not the one nearest your shell.",
        "A repository task named `fmt` owns `effigy fmt`; the same built-in stays reachable as `effigy fmt-rust <FILE>...`.",
    ],
    usage: &[
        "effigy fmt rust <FILE>... [--repo <PATH>] [--json]",
        "effigy fmt rust <FILE>... --check [--repo <PATH>] [--json]",
        "effigy fmt-rust <FILE>... [--repo <PATH>] [--json]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        ("rust", "Format the named Rust files with rustfmt"),
        ("--check", "Report files that need formatting without writing"),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render the versioned command envelope around the format report"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy fmt rust src/main.rs",
        "effigy fmt rust $(git diff --name-only -- '*.rs')",
        "effigy fmt rust crates/effigy-release/src/lib.rs --check",
        "effigy fmt-rust src/main.rs --check",
    ],
};
