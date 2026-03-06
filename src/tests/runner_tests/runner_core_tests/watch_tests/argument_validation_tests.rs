use super::prelude::{
    assert_builtin_error_case_table, assert_output_contains_all, run_builtin_ok, temp_workspace,
    write_empty_manifest, BuiltinErrorCase,
};

#[test]
fn run_manifest_task_builtin_watch_help_renders_topic() {
    let root = temp_workspace("builtin-watch-help");
    write_empty_manifest(&root);

    let out = run_builtin_ok(root, "watch", &["--help"]);
    assert_output_contains_all(
        &out,
        &[
            "watch Help",
            "--owner <effigy|external>",
            "--debounce-ms <MS>",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_watch_validates_owner_and_arguments() {
    let cases = [
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-required-legacy",
            command: "watch",
            args: &[],
            manifest: "",
            expected: &["--owner <effigy|external>` is required"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-unknown-arg",
            command: "watch",
            args: &["--wat"],
            manifest: "",
            expected: &["unknown argument(s) for built-in `watch`: --wat"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-required",
            command: "watch",
            args: &["build", "--once"],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["--owner <effigy|external>` is required"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-missing-value",
            command: "watch",
            args: &["--owner"],
            manifest: "",
            expected: &["`--owner` requires a value (`effigy` or `external`)"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-invalid-value",
            command: "watch",
            args: &["--owner", "robot"],
            manifest: "",
            expected: &["invalid `--owner` value `robot`"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-external",
            command: "watch",
            args: &["--owner", "external", "build", "--once"],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["watch owner `external`", "Run the task directly"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-missing-max-runs-value",
            command: "watch",
            args: &["--owner", "effigy", "--max-runs"],
            manifest: "",
            expected: &["`--max-runs` requires a numeric value"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-invalid-max-runs-value",
            command: "watch",
            args: &["--owner", "effigy", "--max-runs", "nope"],
            manifest: "",
            expected: &["invalid `--max-runs` value `nope`"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-zero-max-runs-value",
            command: "watch",
            args: &["--owner", "effigy", "--max-runs", "0"],
            manifest: "",
            expected: &["`--max-runs` must be greater than zero"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-missing-debounce-value",
            command: "watch",
            args: &["--owner", "effigy", "--debounce-ms"],
            manifest: "",
            expected: &["`--debounce-ms` requires a numeric value"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-invalid-debounce-value",
            command: "watch",
            args: &["--owner", "effigy", "--debounce-ms", "nope"],
            manifest: "",
            expected: &["invalid `--debounce-ms` value `nope`"],
        },
    ];

    assert_builtin_error_case_table(&cases);
}
