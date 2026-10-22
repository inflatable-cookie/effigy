use super::*;

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
            "--critical <N> : critical severity threshold (default 80)",
        ],
        &[],
        &[
            "opt-in: doctor runs it only when `[scan.duplicate_blocks] doctor = true`",
            "generated/derived files and literal-only blocks (data tables, fixtures, string lists) are skipped",
            "string-literal contents are preserved during normalization, so unrelated data maps do not collapse into one block",
        ],
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
            "--threshold, --warn <N> : warning threshold in code lines (overrides per-language defaults)",
            "--high <N> : high severity threshold (overrides per-language defaults)",
            "--critical <N> : critical severity threshold (overrides per-language defaults)",
        ],
        &[],
        &[
            "without an explicit threshold each file uses its language defaults (code lines):",
            "  typescript/javascript/vue/svelte and python/ruby/php: warn 800, high 1400, critical 2400",
            "  rust/c/c++/c#/go/java/kotlin/scala/swift: warn 1000, high 1800, critical 3000",
            "  sql/css/html: warn 1200, high 2000, critical 3200",
            "  shell/lua: warn 500, high 900, critical 1500",
            "  every other extension: warn 1000, high 1800, critical 3000",
            "generated, derived, vendored and agent-skill trees are skipped by default, and `.gitignore` is respected",
        ],
    )
}
