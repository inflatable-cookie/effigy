use super::*;

pub(super) fn render_scan_help() -> String {
    render_titled_help(
        "scan",
        &[
            HelpSection::Plain {
                heading: "Deprecation",
                lines: &[
                    "the direct `effigy scan` spelling is deprecated; use `effigy repo scan` (removal at v1.0)",
                ],
            },
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy repo scan <subcommand> [options]",
                    "effigy repo scan god-files [--threshold <N>] [--markdown] [--out <PATH>]",
                    "effigy repo scan boundary-violations [--markdown] [--out <PATH>]",
                    "effigy repo scan dead-code [--markdown] [--out <PATH>]",
                    "effigy repo scan validation-gaps [--path <PATH>]... [--stdin] [--markdown] [--out <PATH>]",
                    "effigy repo scan duplicate-blocks [--threshold <N>] [--markdown] [--out <PATH>]",
                    "effigy repo scan comment-ratio [--threshold <RATIO>] [--markdown] [--out <PATH>]",
                    "effigy repo scan generated-assets [--threshold <BYTES>] [--markdown] [--out <PATH>]",
                    "effigy repo scan generated-in-src [--threshold <BYTES>] [--source-root <GLOB>] [--markdown] [--out <PATH>]",
                    "effigy repo scan attention-markers [--markdown] [--out <PATH>]",
                    "effigy repo scan stale-suppressions [--markdown] [--out <PATH>]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Common Options",
                items: &[
                    "--threshold, --warn <N|RATIO|BYTES> : warning threshold (meaning depends on the scanner)",
                    "--high <N|RATIO|BYTES> : high severity threshold (threshold scanners only)",
                    "--critical <N|RATIO|BYTES> : critical severity threshold (threshold scanners only)",
                    "--path <PATH> : changed path for validation-gap narrowing, repeatable",
                    "--stdin : read newline-delimited changed paths from stdin (validation-gaps only)",
                    "--include <GLOB> : include glob, repeatable",
                    "--exclude <GLOB> : exclude glob, repeatable",
                    "--source-root <GLOB> : source-tree glob to scan for generated files (generated-in-src only, repeatable)",
                    "--warning-marker <TEXT> : add a warning marker (marker scanners only, repeatable)",
                    "--high-marker <TEXT> : add a high marker (marker scanners only, repeatable)",
                    "--critical-marker <TEXT> : add a critical marker (marker scanners only, repeatable)",
                    "--show-warnings : include warning rows in terminal text output",
                    "--graph-context : attach graph readiness metadata and optional graph context when supported",
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
                    "boundary-violations : detect disallowed graph edges between configured path layers",
                    "dead-code : detect likely isolated files and unreferenced symbols from graph evidence",
                    "validation-gaps : detect hotspot owners and changed owners without nearby test targets",
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
                    "use `effigy repo scan <subcommand> --help` for per-scanner defaults and additional flags",
                    "terminal text hides warning rows by default and prints a warning count summary; markdown and json include full findings",
                ],
            },
        ],
    )
}

pub(super) fn render_boundary_violations_help() -> String {
    render_titled_help(
        "scan boundary-violations",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy repo scan boundary-violations",
                    "effigy repo scan boundary-violations [--markdown] [--out reports/boundary-violations.md]",
                    "effigy repo scan boundary-violations [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--json : render machine-readable scan payload",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--graph-context : attach graph readiness metadata and optional graph context when supported",
                ],
            },
            HelpSection::Bulleted {
                heading: "Manifest Shape",
                items: &[
                    "[scan.boundary_violations.layers.<name>] with `paths = [...]` and optional `may_depend_on = [...]`",
                    "resolved non-heuristic graph edges are checked by default",
                    "repos without configured layers return a clean no-rules result",
                ],
            },
        ],
    )
}

pub(super) fn render_dead_code_help() -> String {
    render_titled_help(
        "scan dead-code",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy repo scan dead-code",
                    "effigy repo scan dead-code [--markdown] [--out reports/dead-code.md]",
                    "effigy repo scan dead-code [--json] [--fail-on-findings]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--json : render machine-readable scan payload",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--graph-context : attach graph readiness metadata and optional graph context when supported",
                ],
            },
            HelpSection::Bulleted {
                heading: "Manifest Shape",
                items: &[
                    "[scan.dead_code] with optional `allow_paths = [...]` and `allow_symbols = [...]`",
                    "findings stay advisory and only use concrete graph evidence from supported languages",
                    "tests, docs, fixtures, generated files, config, migrations, and entrypoint-like scripts are skipped by default",
                ],
            },
        ],
    )
}

pub(super) fn render_validation_gaps_help() -> String {
    render_titled_help(
        "scan validation-gaps",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy repo scan validation-gaps",
                    "effigy repo scan validation-gaps --path src/live/mod.rs --path src/orphan/mod.rs",
                    "git diff --name-only | effigy repo scan validation-gaps --stdin --json",
                ],
            },
            HelpSection::Bulleted {
                heading: "Options",
                items: &[
                    "--path <PATH> : changed path for targeted graph affected/test-target lookup, repeatable",
                    "--stdin : read newline-delimited changed paths from stdin",
                    "--markdown : render markdown instead of terminal text",
                    "--out <PATH> : write rendered report to a file",
                    "--json : render machine-readable scan payload",
                    "--fail-on-findings : return non-zero when findings exist",
                    "--graph-context : attach graph readiness metadata and optional graph context when supported",
                ],
            },
            HelpSection::Bulleted {
                heading: "Manifest Shape",
                items: &[
                    "[scan.validation_gaps] with optional `hotspot_threshold`, `affected_depth`, and `allow_paths`",
                    "without changed paths, the scan reports hotspot owners that lack nearby test targets",
                    "with changed paths, the scan narrows to changed owners and suggested test files/tasks",
                ],
            },
        ],
    )
}
