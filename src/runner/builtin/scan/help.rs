use super::super::help_text::{render_titled_help, HelpSection};

pub(super) fn render_scan_help() -> String {
    render_titled_help(
        "scan",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan <subcommand> [options]",
                    "effigy scan god-files [--threshold <N>] [--markdown] [--out <PATH>]",
                    "effigy scan generated-assets [--threshold <BYTES>] [--markdown] [--out <PATH>]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Subcommands",
                items: &[
                    "god-files : detect oversized code files using code-only line counts",
                    "generated-assets : report bulky vendored/generated artifacts that slipped into the repo",
                ],
            },
        ],
    )
}

pub(super) fn render_god_files_help() -> String {
    render_titled_help(
        "scan god-files",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan god-files [--threshold <N>] [--high <N>] [--critical <N>]",
                    "effigy scan god-files [--show-warnings] [--no-gitignore]",
                    "effigy scan god-files [--markdown] [--out reports/god-files.md]",
                    "effigy scan god-files [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--threshold, --warn <N> : warning threshold (default 250)",
                    "--high <N> : high severity threshold (default 400)",
                    "--critical <N> : critical threshold (default 700)",
                    "--include <GLOB> : include glob, repeatable",
                    "--exclude <GLOB> : exclude glob, repeatable",
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--no-gitignore : ignore .gitignore/.ignore rules during traversal",
                    "--show-warnings : include warning rows in terminal text output",
                    "--json : render machine-readable scan payload",
                ],
            },
            HelpSection::Bulleted {
                heading: "Defaults",
                items: &[
                    "terminal text hides warning rows and prints a warning count summary",
                    "markdown and json still include the full findings list",
                    "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
                ],
            },
        ],
    )
}

pub(super) fn render_generated_assets_help() -> String {
    render_titled_help(
        "scan generated-assets",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan generated-assets [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>]",
                    "effigy scan generated-assets [--show-warnings] [--no-gitignore]",
                    "effigy scan generated-assets [--markdown] [--out reports/generated-assets.md]",
                    "effigy scan generated-assets [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--threshold, --warn <BYTES> : warning threshold in bytes (default 1000000)",
                    "--high <BYTES> : high severity threshold in bytes (default 5000000)",
                    "--critical <BYTES> : critical threshold in bytes (default 20000000)",
                    "--include <GLOB> : include glob, repeatable",
                    "--exclude <GLOB> : exclude glob, repeatable",
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--no-gitignore : ignore .gitignore/.ignore rules during traversal",
                    "--show-warnings : include warning rows in terminal text output",
                    "--json : render machine-readable scan payload",
                ],
            },
            HelpSection::Bulleted {
                heading: "Defaults",
                items: &[
                    "terminal text hides warning rows and prints a warning count summary",
                    "markdown and json still include the full findings list",
                    "matches vendored/build paths, bundle/minified/source-map names, and generated markers",
                ],
            },
        ],
    )
}
