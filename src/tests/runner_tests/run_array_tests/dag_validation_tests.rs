use crate::runner::tests::prelude::{
    assert_run_array_validate_invocation_error_case_table,
    assert_run_array_validate_invocation_message_case_table,
    assert_run_array_validate_output_case_table, RunArrayInvocationErrorCase,
    RunArrayInvocationMessageCase, RunArrayValidateOutputCase,
};

#[test]
fn run_manifest_task_run_array_validate_success_contract_table() {
    let cases = [
        RunArrayValidateOutputCase {
            workspace: "run-array-task-refs",
            manifest: r#"[tasks.lint]
run = "printf lint-ok"

[tasks.validate]
run = [{ task = "lint" }, "printf validate-ok"]
"#,
            args: &["--verbose-root"],
            expected: &["printf lint-ok", "printf validate-ok"],
        },
        RunArrayValidateOutputCase {
            workspace: "run-array-dag-metadata",
            manifest: r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] },
  { run = "printf validate-ok" }
]
"#,
            args: &["--verbose-root"],
            expected: &["printf lint-ok", "printf build-ok", "printf validate-ok"],
        },
    ];

    assert_run_array_validate_output_case_table(&cases);
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

    assert_run_array_validate_invocation_error_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_rejects_dependency_cycles() {
    let cases = [RunArrayInvocationMessageCase {
        workspace: "run-array-dependency-cycles",
        manifest: r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok", depends_on = ["build"] },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] }
]
"#,
        args: &[],
        expected_all: &["contains dependency cycle"],
        expected_any: &["build -> lint -> build", "lint -> build -> lint"],
        expected_exact: None,
    }];

    assert_run_array_validate_invocation_message_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_depends_on_missing_id_error_text_is_stable() {
    let cases = [RunArrayInvocationMessageCase {
        workspace: "run-array-depends-on-missing-id-stable",
        manifest: r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { run = "printf build-ok", depends_on = ["lint"] }
]
"#,
        args: &[],
        expected_all: &[],
        expected_any: &[],
        expected_exact: Some(
            "task `validate` run step 2 defines `depends_on` but is missing a non-empty `id`",
        ),
    }];

    assert_run_array_validate_invocation_message_case_table(&cases);
}
