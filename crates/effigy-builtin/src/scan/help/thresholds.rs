use super::*;

pub(super) fn render_generated_in_src_help() -> String {
    render_threshold_scan_help(
        "scan generated-in-src",
        &[
            "effigy scan generated-in-src [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>]",
            "effigy scan generated-in-src [--source-root <GLOB>] [--show-warnings] [--no-gitignore]",
            "effigy scan generated-in-src [--markdown] [--out reports/generated-in-src.md]",
            "effigy scan generated-in-src [--json] [--fail-on-findings]",
        ],
        &[
            "--threshold, --warn <BYTES> : warning threshold in bytes (default 1)",
            "--high <BYTES> : high severity threshold in bytes (default 20000)",
            "--critical <BYTES> : critical threshold in bytes (default 200000)",
        ],
        &["--source-root <GLOB> : source-tree glob to scan for generated files, repeatable"],
        &[
            "targets source roots such as src, app, lib, crates, and packages/*/src",
            "matches generated markers, generated-style filenames, and minified/source-map artifacts inside source trees",
        ],
    )
}

pub(super) fn render_comment_ratio_help() -> String {
    render_threshold_scan_help(
        "scan comment-ratio",
        &[
            "effigy scan comment-ratio [--threshold <RATIO>] [--high <RATIO>] [--critical <RATIO>]",
            "effigy scan comment-ratio [--min-code-lines <N>] [--show-warnings] [--no-gitignore]",
            "effigy scan comment-ratio [--markdown] [--out reports/comment-ratio.md]",
            "effigy scan comment-ratio [--json] [--fail-on-findings]",
        ],
        &[
            "--threshold, --warn <RATIO> : warning threshold in comment/code ratio (default 1.5)",
            "--high <RATIO> : high severity threshold (default 2.0)",
            "--critical <RATIO> : critical threshold (default 3.0)",
        ],
        &["--min-code-lines <N> : minimum code-only lines before a file is evaluated (default 20)"],
        &[
            "counts comment-only lines against code-only lines in source and test files",
            "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
        ],
    )
}

pub(super) fn render_duplicate_blocks_help() -> String {
    render_threshold_scan_help(
        "scan duplicate-blocks",
        &[
            "effigy scan duplicate-blocks [--threshold <N>] [--high <N>] [--critical <N>]",
            "effigy scan duplicate-blocks [--show-warnings] [--no-gitignore]",
            "effigy scan duplicate-blocks [--markdown] [--out reports/duplicate-blocks.md]",
            "effigy scan duplicate-blocks [--json] [--fail-on-findings]",
        ],
        &[
            "--threshold, --warn <N> : warning threshold in normalized code lines (default 20)",
            "--high <N> : high severity threshold (default 40)",
            "--critical <N> : critical threshold (default 80)",
        ],
        &[],
        &["detects repeated normalized code blocks across files, excluding common docs/data/generated paths by default"],
    )
}

pub(super) fn render_god_files_help() -> String {
    render_threshold_scan_help(
        "scan god-files",
        &[
            "effigy scan god-files [--threshold <N>] [--high <N>] [--critical <N>]",
            "effigy scan god-files [--show-warnings] [--no-gitignore]",
            "effigy scan god-files [--markdown] [--out reports/god-files.md]",
            "effigy scan god-files [--json] [--fail-on-findings]",
        ],
        &[
            "--threshold, --warn <N> : warning threshold (default 250)",
            "--high <N> : high severity threshold (default 400)",
            "--critical <N> : critical threshold (default 700)",
        ],
        &[],
        &["common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default"],
    )
}

pub(super) fn render_generated_assets_help() -> String {
    render_threshold_scan_help(
        "scan generated-assets",
        &[
            "effigy scan generated-assets [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>]",
            "effigy scan generated-assets [--show-warnings] [--no-gitignore]",
            "effigy scan generated-assets [--markdown] [--out reports/generated-assets.md]",
            "effigy scan generated-assets [--json] [--fail-on-findings]",
        ],
        &[
            "--threshold, --warn <BYTES> : warning threshold in bytes (default 1000000)",
            "--high <BYTES> : high severity threshold in bytes (default 5000000)",
            "--critical <BYTES> : critical threshold in bytes (default 20000000)",
        ],
        &[],
        &["matches vendored/build paths, bundle/minified/source-map names, and generated markers"],
    )
}
