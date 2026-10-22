use super::*;

pub(super) fn render_scan_help() -> String {
    render_titled_help(
        "scan",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan <subcommand> [options]",
                    "effigy scan god-files [--threshold <N>] [--markdown] [--out <PATH>]",
                    "effigy scan duplicate-blocks [--threshold <N>] [--markdown] [--out <PATH>]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Common Options",
                items: &[
                    "--threshold, --warn <N> : warning threshold in code lines",
                    "--high <N> : high severity threshold in code lines",
                    "--critical <N> : critical severity threshold in code lines",
                    "--include <GLOB> : include glob, repeatable",
                    "--exclude <GLOB> : exclude glob, repeatable",
                    "--show-warnings : include warning rows in terminal text output",
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--json : render machine-readable scan payload",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--no-gitignore : ignore .gitignore/.ignore rules during traversal",
                ],
            },
            HelpSection::Bulleted {
                heading: "Subcommands",
                items: &[
                    "god-files : detect oversized code files using code-only line counts",
                    "duplicate-blocks : detect repeated code blocks across source files (opt-in)",
                ],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &[
                    "generated, derived, vendored and agent-skill trees are skipped by default",
                    "TODO/FIXME searches are not a scan: use `rg -n 'TODO|FIXME' <path>`",
                    "`effigy scan <subcommand> --help` shows per-scanner defaults and flags",
                    "terminal text hides warning rows by default and prints a warning count summary; markdown and json include full findings",
                ],
            },
        ],
    )
}
