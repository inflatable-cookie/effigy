use super::prelude::*;

#[test]
fn run_manifest_task_builtin_catalogs_pretty_requires_json() {
    let root = temp_workspace("builtin-catalogs-pretty-requires-json");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");

    let err = run_catalogs_err(root, &["--pretty", "false"]);
    assert_task_invocation_error_contains(
        err,
        &["`--pretty` is only supported together with `--json`"],
    );
}

#[test]
fn run_manifest_task_builtin_catalogs_rejects_invalid_pretty_value() {
    let root = temp_workspace("builtin-catalogs-invalid-pretty");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");

    let err = run_catalogs_err(root, &["--json", "--pretty", "nope"]);
    assert_task_invocation_error_contains(err, &["value `nope` is invalid"]);
}

#[test]
fn run_manifest_task_builtin_catalogs_validates_missing_value_flags() {
    let root = temp_workspace("builtin-catalogs-missing-values");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");

    let err = run_catalogs_err(root.clone(), &["--resolve"]);
    assert_task_invocation_error_contains(err, &["catalogs argument --resolve requires a value"]);

    let err = run_catalogs_err(root.clone(), &["--json", "--pretty"]);
    assert_task_invocation_error_contains(
        err,
        &["catalogs argument --pretty requires a value (`true` or `false`)"],
    );

    let err = run_catalogs_err(root, &["--task"]);
    assert_task_invocation_error_contains(err, &["task argument --task requires a value"]);
}

#[test]
fn run_manifest_task_builtin_catalogs_reports_unknown_argument_grouping() {
    let root = temp_workspace("builtin-catalogs-unknown-args");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");

    let err = run_catalogs_err(root, &["--wat", "--huh"]);
    assert_task_invocation_error_contains(
        err,
        &["unknown argument(s) for built-in `catalogs`: --wat --huh"],
    );
}
