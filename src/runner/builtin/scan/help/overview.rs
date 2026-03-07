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
        ],
    )
}
