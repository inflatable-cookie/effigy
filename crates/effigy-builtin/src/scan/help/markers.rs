use super::*;

pub(super) fn render_attention_markers_help() -> String {
    render_marker_scan_help(
        "scan attention-markers",
        &[
            "effigy scan attention-markers [--show-warnings] [--no-gitignore]",
            "effigy scan attention-markers [--markdown] [--out reports/attention-markers.md]",
            "effigy scan attention-markers [--json] [--fail-on-findings]",
        ],
        &[
            "--warning-marker <VALUE> : override warning markers, repeatable",
            "--high-marker <VALUE> : override high markers, repeatable",
            "--critical-marker <VALUE> : override critical markers, repeatable",
        ],
        &[
            "detects TODO/FIXME/HACK/deprecation/workaround-style markers in source and test files",
            "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
        ],
    )
}

pub(super) fn render_stale_suppressions_help() -> String {
    render_marker_scan_help(
        "scan stale-suppressions",
        &[
            "effigy scan stale-suppressions [--show-warnings] [--no-gitignore]",
            "effigy scan stale-suppressions [--warning-marker <VALUE>] [--high-marker <VALUE>] [--critical-marker <VALUE>]",
            "effigy scan stale-suppressions [--markdown] [--out reports/stale-suppressions.md]",
            "effigy scan stale-suppressions [--json] [--fail-on-findings]",
        ],
        &[
            "--warning-marker <VALUE> : override warning markers, repeatable",
            "--high-marker <VALUE> : override high markers, repeatable",
            "--critical-marker <VALUE> : override critical markers, repeatable",
        ],
        &[
            "matches common TS, Python, Rust, shell, and linter suppression markers in source and test files",
            "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
        ],
    )
}
