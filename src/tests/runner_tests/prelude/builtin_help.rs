use super::cases::{builtin_help_case, BuiltinHelpCase};
use super::fixture_support::run_task_in_workspace;
use super::output::assert_output_equals;
use super::runtime::{Path, RunnerError};

pub(in crate::runner::tests) fn builtin_shared_help_precedence_cases(
    completion_workspace: &'static str,
    completion_candidates_workspace: &'static str,
) -> Vec<BuiltinHelpCase> {
    vec![
        builtin_help_case(
            completion_workspace,
            "completion",
            &["--help", "--wat"],
            &[
                "completion Help",
                "effigy config completion [<bash|zsh|fish>] [--install|--export]",
            ],
        ),
        builtin_help_case(
            completion_candidates_workspace,
            "completion",
            &["candidates", "--help", "--wat"],
            &[
                "completion candidates Help",
                "effigy config completion candidates [--repo <path>] [--prefix <value>] [--json]",
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
                "effigy scan <subcommand> [options]",
                "duplicate-blocks : detect repeated code blocks across source files (opt-in)",
                "TODO/FIXME searches are not a scan: use `rg -n 'TODO|FIXME' <path>`",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-god-files",
            "scan",
            &["god-files", "--help", "--wat"],
            &[
                "scan god-files Help",
                "effigy scan god-files [--markdown] [--out reports/god-files.md]",
                "--show-warnings : include warning rows in terminal text output",
                "generated, derived, vendored and agent-skill trees are skipped by default",
            ],
        ),
        builtin_help_case(
            "builtin-subcommand-help-scan-duplicate-blocks",
            "scan",
            &["duplicate-blocks", "--help", "--wat"],
            &[
                "scan duplicate-blocks Help",
                "effigy scan duplicate-blocks [--markdown] [--out reports/duplicate-blocks.md]",
                "--show-warnings : include warning rows in terminal text output",
                "opt-in: doctor runs it only when `[scan.duplicate_blocks] doctor = true`",
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
                "effigy scan god-files [--threshold <N>] [--high <N>] [--critical <N>]",
                "effigy scan god-files [--show-warnings] [--no-gitignore]",
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
                "effigy scan duplicate-blocks [--threshold <N>] [--high <N>] [--critical <N>]",
                "effigy scan duplicate-blocks [--show-warnings] [--no-gitignore]",
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
