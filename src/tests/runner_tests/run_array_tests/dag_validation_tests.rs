use super::prelude::*;

#[test]
fn run_manifest_task_run_array_supports_task_reference_steps() {
    let root = temp_workspace("run-array-task-refs");
    write_validate_manifest(
        &root,
        r#"[tasks.lint]
run = "printf lint-ok"

[tasks.validate]
run = [{ task = "lint" }, "printf validate-ok"]
"#,
    );

    let out = run_validate_ok(&root, &["--verbose-root"]);
    assert_contains_all(&out, &["printf lint-ok", "printf validate-ok"]);
}

#[test]
fn run_manifest_task_run_array_accepts_dag_metadata() {
    let root = temp_workspace("run-array-dag-metadata");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] },
  { run = "printf validate-ok" }
]
"#,
    );

    let out = run_validate_ok(&root, &["--verbose-root"]);
    assert_contains_all(
        &out,
        &["printf lint-ok", "printf build-ok", "printf validate-ok"],
    );
}

#[test]
fn run_manifest_task_run_array_rejects_invalid_dag_metadata() {
    let cases = [
        RunArrayInvocationErrorCase {
            workspace: "run-array-depends-on-without-id",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"lint\", run = \"printf lint-ok\" },\n  { run = \"printf build-ok\", depends_on = [\"lint\"] }\n]\n",
            expected: &["defines `depends_on` but is missing a non-empty `id`"],
        },
        RunArrayInvocationErrorCase {
            workspace: "run-array-missing-dependency-step",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"build\", run = \"printf build-ok\", depends_on = [\"lint\"] }\n]\n",
            expected: &["depends on missing step `lint`"],
        },
        RunArrayInvocationErrorCase {
            workspace: "run-array-duplicate-step-ids",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"lint\", run = \"printf lint-ok\" },\n  { id = \"lint\", run = \"printf lint-again\" }\n]\n",
            expected: &["duplicate step id `lint`"],
        },
        RunArrayInvocationErrorCase {
            workspace: "run-array-self-dependency-cycle",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"lint\", run = \"printf lint-ok\", depends_on = [\"lint\"] }\n]\n",
            expected: &["cannot depend on itself"],
        },
    ];

    assert_case_table(cases, |case| {
        let root = temp_workspace(case.workspace);
        write_validate_manifest(&root, case.manifest);
        let err = run_validate_err(&root, &[]);
        assert_task_invocation_error_contains(err, case.expected);
    });
}

#[test]
fn run_manifest_task_run_array_rejects_dependency_cycles() {
    let root = temp_workspace("run-array-dependency-cycles");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok", depends_on = ["build"] },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] }
]
"#,
    );

    let err = run_validate_err(&root, &[]);

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("contains dependency cycle"));
            assert!(
                message.contains("build -> lint -> build")
                    || message.contains("lint -> build -> lint")
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_run_array_depends_on_missing_id_error_text_is_stable() {
    let root = temp_workspace("run-array-depends-on-missing-id-stable");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { run = "printf build-ok", depends_on = ["lint"] }
]
"#,
    );

    let err = run_validate_err(&root, &[]);

    match err {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(
                message,
                "task `validate` run step 2 defines `depends_on` but is missing a non-empty `id`"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}
