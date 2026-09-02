use super::cases::{builtin_help_case, BuiltinHelpCase};
use super::fixture_support::run_task_in_workspace;
use super::output::assert_output_equals;
use super::runtime::{Path, RunnerError};

pub(in crate::runner::tests) fn builtin_shared_help_precedence_cases(
    cache_workspace: &'static str,
    completion_workspace: &'static str,
    completion_candidates_workspace: &'static str,
) -> Vec<BuiltinHelpCase> {
    vec![
        builtin_help_case(
            cache_workspace,
            "cache",
            &["--wat", "--help"],
            &["tasks cache Help", "effigy tasks cache inspect"],
        ),
        builtin_help_case(
            completion_workspace,
            "completion",
            &["--help", "--wat"],
            &[
                "completion Help",
                "effigy admin config completion [<bash|zsh|fish>] [--install|--export]",
            ],
        ),
        builtin_help_case(
            completion_candidates_workspace,
            "completion",
            &["candidates", "--help", "--wat"],
            &[
                "completion candidates Help",
                "effigy admin config completion candidates [--repo <path>] [--prefix <value>] [--json]",
            ],
        ),
    ]
}

pub(in crate::runner::tests) fn builtin_scan_subcommand_help_cases() -> Vec<BuiltinHelpCase> {
    vec![
        builtin_help_case(
            "builtin-subcommand-help-scan-bare",
            "scan",
            &[],
            &[
                "scan Help",
                "effigy repo scan <subcommand> [options]",
                "stale-suppressions : detect lint/type/tool suppression markers that hide warnings and failures",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-god-files",
            "scan",
            &["god-files", "--help", "--wat"],
            &[
                "scan god-files Help",
                "effigy repo scan god-files [--markdown] [--out reports/god-files.md]",
                "--show-warnings : include warning rows in terminal text output",
                "common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts are skipped by default",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-duplicate-blocks",
            "scan",
            &["duplicate-blocks", "--help", "--wat"],
            &[
                "scan duplicate-blocks Help",
                "effigy repo scan duplicate-blocks [--markdown] [--out reports/duplicate-blocks.md]",
                "--show-warnings : include warning rows in terminal text output",
                "detects repeated normalized code blocks across files, excluding common docs/data/generated paths by default",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-comment-ratio",
            "scan",
            &["comment-ratio", "--help", "--wat"],
            &[
                "scan comment-ratio Help",
                "effigy repo scan comment-ratio [--markdown] [--out reports/comment-ratio.md]",
                "--show-warnings : include warning rows in terminal text output",
                "counts comment-only lines against code-only lines in source and test files",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-generated-in-src",
            "scan",
            &["generated-in-src", "--help", "--wat"],
            &[
                "scan generated-in-src Help",
                "effigy repo scan generated-in-src [--markdown] [--out reports/generated-in-src.md]",
                "--show-warnings : include warning rows in terminal text output",
                "targets source roots such as src, app, lib, crates, and packages/*/src",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-attention-markers",
            "scan",
            &["attention-markers", "--help", "--wat"],
            &[
                "scan attention-markers Help",
                "effigy repo scan attention-markers [--markdown] [--out reports/attention-markers.md]",
                "--show-warnings : include warning rows in terminal text output",
                "detects TODO/FIXME/HACK/deprecation/workaround-style markers in source and test files",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-stale-suppressions",
            "scan",
            &["stale-suppressions", "--help", "--wat"],
            &[
                "scan stale-suppressions Help",
                "effigy repo scan stale-suppressions [--markdown] [--out reports/stale-suppressions.md]",
                "--show-warnings : include warning rows in terminal text output",
                "matches common TS, Python, Rust, shell, and linter suppression markers in source and test files",
            ],
        ),
    ]
}

pub(in crate::runner::tests) fn builtin_scan_help_precedence_cases() -> Vec<BuiltinHelpCase> {
    vec![
        builtin_help_case(
            "builtin-scan-help-precedence",
            "scan",
            &["god-files", "--help", "--wat"],
            &[
                "scan god-files Help",
                "effigy repo scan god-files [--threshold <N>] [--high <N>] [--critical <N>]",
                "effigy repo scan god-files [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        ),
        builtin_help_case(
            "builtin-scan-duplicate-help-precedence",
            "scan",
            &["duplicate-blocks", "--help", "--wat"],
            &[
                "scan duplicate-blocks Help",
                "effigy repo scan duplicate-blocks [--threshold <N>] [--high <N>] [--critical <N>]",
                "effigy repo scan duplicate-blocks [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        ),
        builtin_help_case(
            "builtin-scan-comment-ratio-help-precedence",
            "scan",
            &["comment-ratio", "--help", "--wat"],
            &[
                "scan comment-ratio Help",
                "effigy repo scan comment-ratio [--threshold <RATIO>] [--high <RATIO>] [--critical <RATIO>]",
                "effigy repo scan comment-ratio [--min-code-lines <N>] [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        ),
        builtin_help_case(
            "builtin-scan-generated-in-src-help-precedence",
            "scan",
            &["generated-in-src", "--help", "--wat"],
            &[
                "scan generated-in-src Help",
                "effigy repo scan generated-in-src [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>]",
                "effigy repo scan generated-in-src [--source-root <GLOB>] [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        ),
        builtin_help_case(
            "builtin-scan-attention-help-precedence",
            "scan",
            &["attention-markers", "--help", "--wat"],
            &[
                "scan attention-markers Help",
                "effigy repo scan attention-markers [--show-warnings] [--no-gitignore]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        ),
        builtin_help_case(
            "builtin-scan-stale-suppressions-help-precedence",
            "scan",
            &["stale-suppressions", "--help", "--wat"],
            &[
                "scan stale-suppressions Help",
                "effigy repo scan stale-suppressions [--show-warnings] [--no-gitignore]",
                "effigy repo scan stale-suppressions [--warning-marker <VALUE>] [--high-marker <VALUE>] [--critical-marker <VALUE>]",
                "--show-warnings : include warning rows in terminal text output",
                "terminal text hides warning rows and prints a warning count summary",
            ],
        ),
    ]
}

pub(in crate::runner::tests) fn run_task(
    root: &Path,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_task_in_workspace(root, name, args)
}

pub(in crate::runner::tests) fn write_empty_manifest(root: &Path) {
    super::harness::write_root_manifest(root, "");
}

pub(in crate::runner::tests) fn assert_run_task_ok_empty(root: &Path, name: &str, args: &[&str]) {
    let out = run_task(root, name, args).expect("task should succeed");
    assert_output_equals(&out, "");
}

pub(in crate::runner::tests) fn assert_builtin_ok_for_empty_manifest(
    command: &str,
    cases: &[super::cases::BuiltinInvocationCase],
) {
    super::cases::assert_builtin_ok_case_table_with_setup(command, cases, write_empty_manifest);
}

pub(in crate::runner::tests) fn assert_builtin_error_for_empty_manifest(
    command: &str,
    cases: &[super::cases::BuiltinInvocationCase],
) {
    super::cases::assert_builtin_error_case_table_with_setup(command, cases, write_empty_manifest);
}
