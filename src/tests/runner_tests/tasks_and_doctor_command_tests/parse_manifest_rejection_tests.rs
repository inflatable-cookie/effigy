use super::prelude::{
    assert_tasks_manifest_parse_rejection_case_table, ManifestParseRejectionCase,
};

#[test]
fn run_tasks_rejects_invalid_manifest_shapes() {
    let cases = [
        ManifestParseRejectionCase {
            workspace: "reject-legacy-builtin-group",
            manifest: "[builtin.test]\nmax_parallel = 2\n",
            expected: &["unknown field `builtin`"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-unknown-test-field",
            manifest: "[test]\nmax_parallels = 2\n",
            expected: &["unknown field `max_parallels`"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-invalid-cargo-env-match-mode",
            manifest: "[test]\ncargo_env_match = \"shell\"\n",
            expected: &["unknown variant `shell`"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-unknown-package-manager-field",
            manifest: "[package_manager]\njss = \"pnpm\"\n",
            expected: &["unknown field `jss`"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-unknown-test-runner-override-field",
            manifest: "[test.runners.vitest]\ncmd = \"vitest run\"\n",
            expected: &["unknown field `cmd`", "data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-unknown-task-field",
            manifest: "[tasks.dev]\nrun = \"printf dev\"\nfial_on_non_zero = true\n",
            expected: &[
                "unknown field `fial_on_non_zero`",
                "data did not match any variant",
            ],
        },
        ManifestParseRejectionCase {
            workspace: "reject-legacy-task-shell-flag",
            manifest: "[tasks.dev]\nmode = \"tui\"\nshell = true\nconcurrent = [{ run = \"printf api\" }]\n",
            expected: &["unknown field `shell`", "data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-unknown-process-field",
            manifest: "[tasks.dev]\nmode = \"tui\"\nconcurrent = [{ run = \"printf api\", tas = \"api\" }]\n",
            expected: &["unknown field `tas`", "data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-legacy-managed-processes-block",
            manifest: "[tasks.dev]\nmode = \"tui\"\n\n[tasks.dev.processes.api]\nrun = \"printf api\"\n",
            expected: &["unknown field `processes`", "data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-legacy-managed-profile-list-entry",
            manifest: "[tasks.dev]\nmode = \"tui\"\n\n[tasks.dev.profiles]\ndefault = [\"farmyard/api\"]\n",
            expected: &["invalid type", "data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-unknown-run-step-field",
            manifest: "[tasks.reset-db]\nrun = [\n  { run = \"echo one\", rnu = \"echo two\" }\n]\n",
            expected: &["unknown field `rnu`", "data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-unknown-catalog-field",
            manifest: "[catalog]\nalias = \"farmyard\"\naliass = \"dup\"\n",
            expected: &["unknown field `aliass`"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-invalid-env-profile-shape",
            manifest: "[env]\ncargo = { CARGO_HOME = \"tmp\" }\n",
            expected: &["invalid type", "data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-invalid-task-env-file-type",
            manifest: "[tasks.api]\nenv_file = true\nrun = \"printf api\"\n",
            expected: &["data did not match any variant"],
        },
        ManifestParseRejectionCase {
            workspace: "reject-invalid-run-step-env-file-type",
            manifest: "[tasks.api]\nrun = [{ env_file = 1 }, { run = \"printf api\" }]\n",
            expected: &["data did not match any variant"],
        },
    ];
    assert_tasks_manifest_parse_rejection_case_table(&cases);
}
