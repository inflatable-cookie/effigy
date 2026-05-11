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
                    "effigy scan comment-ratio [--threshold <RATIO>] [--markdown] [--out <PATH>]",
                    "effigy scan generated-assets [--threshold <BYTES>] [--markdown] [--out <PATH>]",
                    "effigy scan generated-in-src [--threshold <BYTES>] [--source-root <GLOB>] [--markdown] [--out <PATH>]",
                    "effigy scan attention-markers [--markdown] [--out <PATH>]",
                    "effigy scan stale-suppressions [--markdown] [--out <PATH>]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Common Options",
                items: &[
                    "--threshold, --warn <N|RATIO|BYTES> : warning threshold (meaning depends on the scanner)",
                    "--high <N|RATIO|BYTES> : high severity threshold (threshold scanners only)",
                    "--critical <N|RATIO|BYTES> : critical severity threshold (threshold scanners only)",
                    "--include <GLOB> : include glob, repeatable",
                    "--exclude <GLOB> : exclude glob, repeatable",
                    "--source-root <GLOB> : source-tree glob to scan for generated files (generated-in-src only, repeatable)",
                    "--warning-marker <TEXT> : add a warning marker (marker scanners only, repeatable)",
                    "--high-marker <TEXT> : add a high marker (marker scanners only, repeatable)",
                    "--critical-marker <TEXT> : add a critical marker (marker scanners only, repeatable)",
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
                    "duplicate-blocks : detect repeated normalized code blocks across source files",
                    "comment-ratio : detect files where comment-only lines outweigh executable code",
                    "generated-assets : report bulky vendored/generated artifacts that slipped into the repo",
                    "generated-in-src : detect generated files committed inside source-oriented directories",
                    "attention-markers : detect TODO/FIXME/deprecation and deferred-work markers in code",
                    "stale-suppressions : detect lint/type/tool suppression markers that hide warnings and failures",
                ],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &[
                    "use `effigy scan <subcommand> --help` for per-scanner defaults and additional flags",
                    "terminal text hides warning rows by default and prints a warning count summary; markdown and json include full findings",
                ],
            },
        ],
    )
}
