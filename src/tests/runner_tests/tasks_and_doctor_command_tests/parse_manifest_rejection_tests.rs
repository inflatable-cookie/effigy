use super::prelude::*;

fn assert_tasks_manifest_parse_error_contains_any(root: PathBuf, expected: &[&str]) {
    let err = run_tasks_with_repo(root).expect_err("expected manifest parse failure");
    assert_task_manifest_parse_runner_error_contains_any(err, expected);
}

struct ParseRejectionCase {
    workspace: &'static str,
    manifest: &'static str,
    expected: &'static [&'static str],
}

#[test]
fn run_tasks_rejects_invalid_manifest_shapes() {
    let cases = [
        ParseRejectionCase {
            workspace: "reject-legacy-builtin-group",
            manifest: "[builtin.test]\nmax_parallel = 2\n",
            expected: &["unknown field `builtin`"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-test-field",
            manifest: "[test]\nmax_parallels = 2\n",
            expected: &["unknown field `max_parallels`"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-package-manager-field",
            manifest: "[package_manager]\njss = \"pnpm\"\n",
            expected: &["unknown field `jss`"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-test-runner-override-field",
            manifest: "[test.runners.vitest]\ncmd = \"vitest run\"\n",
            expected: &["unknown field `cmd`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-task-field",
            manifest: "[tasks.dev]\nrun = \"printf dev\"\nfial_on_non_zero = true\n",
            expected: &[
                "unknown field `fial_on_non_zero`",
                "data did not match any variant",
            ],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-process-field",
            manifest: "[tasks.dev]\nmode = \"tui\"\nconcurrent = [{ run = \"printf api\", tas = \"api\" }]\n",
            expected: &["unknown field `tas`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-legacy-managed-processes-block",
            manifest: "[tasks.dev]\nmode = \"tui\"\n\n[tasks.dev.processes.api]\nrun = \"printf api\"\n",
            expected: &["unknown field `processes`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-legacy-managed-profile-list-entry",
            manifest: "[tasks.dev]\nmode = \"tui\"\n\n[tasks.dev.profiles]\ndefault = [\"farmyard/api\"]\n",
            expected: &["invalid type", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-run-step-field",
            manifest: "[tasks.reset-db]\nrun = [\n  { run = \"echo one\", rnu = \"echo two\" }\n]\n",
            expected: &["unknown field `rnu`", "data did not match any variant"],
        },
        ParseRejectionCase {
            workspace: "reject-unknown-catalog-field",
            manifest: "[catalog]\nalias = \"farmyard\"\naliass = \"dup\"\n",
            expected: &["unknown field `aliass`"],
        },
        ParseRejectionCase {
            workspace: "reject-invalid-env-profile-shape",
            manifest: "[env]\ncargo = { CARGO_HOME = \"tmp\" }\n",
            expected: &["invalid type", "data did not match any variant"],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_manifest(&root.join("effigy.toml"), case.manifest);
        assert_tasks_manifest_parse_error_contains_any(root, case.expected);
    }
}
