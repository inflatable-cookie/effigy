use super::super::super::prelude::*;

fn assert_plan_schema_v1(parsed: &serde_json::Value) {
    assert_eq!(parsed["schema"], "effigy.test.plan.v1");
    assert_eq!(parsed["schema_version"], 1);
}

#[test]
fn run_manifest_task_builtin_test_plan_multi_suite_recovery_outputs_hints() {
    let cases = [
        BuiltinTestRecoveryCase {
            workspace: "builtin-test-multi-suite-plan-recovery",
            args: &["--plan", "user-service"],
            expected: &[
                "Test Plan",
                "runtime: plan-recovery",
                "available-suites:",
                "ambiguous",
                "Try one of:",
            ],
        },
        BuiltinTestRecoveryCase {
            workspace: "builtin-test-plan-mistyped-suite-recovery",
            args: &["--plan", "viteest", "user-service"],
            expected: &[
                "Test Plan",
                "runtime: plan-recovery",
                "Did you mean `vitest`?",
                "Try: effigy test vitest user-service",
            ],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        setup_multi_suite_repo(&root);
        let out = run_builtin_ok(root, "test", case.args);
        assert_contains_all(&out, case.expected);
    }
}

#[test]
fn run_manifest_task_builtin_test_plan_json_recovery_has_versioned_schema() {
    let root = temp_workspace("builtin-test-plan-json-recovery-schema");
    setup_multi_suite_repo(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "--json", "user-service"]);
    let parsed = parse_json_output(&out);
    assert_plan_schema_v1(&parsed);
    assert_eq!(parsed["runtime"], "plan-recovery");
    assert!(parsed["recovery"].is_object());
}
