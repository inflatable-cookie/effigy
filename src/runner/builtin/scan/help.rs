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

pub(super) fn render_generated_in_src_help() -> String {
    render_titled_help(
        "scan generated-in-src",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan generated-in-src [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>]",
                    "effigy scan generated-in-src [--source-root <GLOB>] [--show-warnings] [--no-gitignore]",
                    "effigy scan generated-in-src [--markdown] [--out reports/generated-in-src.md]",
                    "effigy scan generated-in-src [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--threshold, --warn <BYTES> : warning threshold in bytes (default 1)",
                    "--high <BYTES> : high severity threshold in bytes (default 20000)",
                    "--critical <BYTES> : critical threshold in bytes (default 200000)",
                    "--source-root <GLOB> : source-tree glob to scan for generated files, repeatable",
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
                    "targets source roots such as src, app, lib, crates, and packages/*/src",
                    "matches generated markers, generated-style filenames, and minified/source-map artifacts inside source trees",
                ],
            },
        ],
    )
}

pub(super) fn render_comment_ratio_help() -> String {
    render_titled_help(
        "scan comment-ratio",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan comment-ratio [--threshold <RATIO>] [--high <RATIO>] [--critical <RATIO>]",
                    "effigy scan comment-ratio [--min-code-lines <N>] [--show-warnings] [--no-gitignore]",
                    "effigy scan comment-ratio [--markdown] [--out reports/comment-ratio.md]",
                    "effigy scan comment-ratio [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--threshold, --warn <RATIO> : warning threshold in comment/code ratio (default 1.5)",
                    "--high <RATIO> : high severity threshold (default 2.0)",
                    "--critical <RATIO> : critical threshold (default 3.0)",
                    "--min-code-lines <N> : minimum code-only lines before a file is evaluated (default 20)",
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
                    "counts comment-only lines against code-only lines in source and test files",
                    "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
                ],
            },
        ],
    )
}

pub(super) fn render_duplicate_blocks_help() -> String {
    render_titled_help(
        "scan duplicate-blocks",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan duplicate-blocks [--threshold <N>] [--high <N>] [--critical <N>]",
                    "effigy scan duplicate-blocks [--show-warnings] [--no-gitignore]",
                    "effigy scan duplicate-blocks [--markdown] [--out reports/duplicate-blocks.md]",
                    "effigy scan duplicate-blocks [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--threshold, --warn <N> : warning threshold in normalized code lines (default 20)",
                    "--high <N> : high severity threshold (default 40)",
                    "--critical <N> : critical threshold (default 80)",
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
                    "detects repeated normalized code blocks across files, excluding common docs/data/generated paths by default",
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

pub(super) fn render_attention_markers_help() -> String {
    render_titled_help(
        "scan attention-markers",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan attention-markers [--show-warnings] [--no-gitignore]",
                    "effigy scan attention-markers [--markdown] [--out reports/attention-markers.md]",
                    "effigy scan attention-markers [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
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
                    "detects TODO/FIXME/HACK/deprecation/workaround-style markers in source and test files",
                    "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
                ],
            },
        ],
    )
}

pub(super) fn render_stale_suppressions_help() -> String {
    render_titled_help(
        "scan stale-suppressions",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy scan stale-suppressions [--show-warnings] [--no-gitignore]",
                    "effigy scan stale-suppressions [--warning-marker <VALUE>] [--high-marker <VALUE>] [--critical-marker <VALUE>]",
                    "effigy scan stale-suppressions [--markdown] [--out reports/stale-suppressions.md]",
                    "effigy scan stale-suppressions [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--warning-marker <VALUE> : override warning markers, repeatable",
                    "--high-marker <VALUE> : override high markers, repeatable",
                    "--critical-marker <VALUE> : override critical markers, repeatable",
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
                    "matches common TS, Python, Rust, shell, and linter suppression markers in source and test files",
                    "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
                ],
            },
        ],
    )
}
