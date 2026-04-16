use super::super::super::prelude::cases::*;
use super::super::super::prelude::harness::*;
use super::super::super::prelude::json::*;
use crate::contract_test_support::{lock_test, EnvGuard};
use std::fs;

#[test]
fn run_manifest_task_builtin_test_plan_multi_suite_recovery_outputs_hints() {
    let cases = [
        BuiltinInvocationCase {
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
        BuiltinInvocationCase {
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

    assert_builtin_ok_case_table_with_setup("test", &cases, setup_multi_suite_repo);
}

#[test]
fn run_manifest_task_builtin_test_plan_json_recovery_has_versioned_schema() {
    let root = temp_workspace("builtin-test-plan-json-recovery-schema");
    setup_multi_suite_repo(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "--json", "user-service"]);
    let parsed = parse_json_output_with_schema_version(&out, "effigy.test.plan.v1", 1);
    assert_json_string_field_eq(&parsed, "runtime", "plan-recovery");
    assert_json_object_field(&parsed, "recovery");
}

#[test]
fn run_manifest_task_builtin_test_plan_json_recovery_preserves_message_and_suite_candidates() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-plan-json-recovery-details");
    setup_multi_suite_repo(&root);
    let empty_bin = root.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("mkdir empty");
    let _env = EnvGuard::set_many(&[("PATH", Some(empty_bin.display().to_string()))]);

    let out = run_builtin_ok(
        root,
        "test",
        &["--plan", "--json", "viteest", "user-service"],
    );
    let parsed = parse_json_output_with_schema_version(&out, "effigy.test.plan.v1", 1);
    let recovery_message = parsed["recovery"]["message"]
        .as_str()
        .expect("recovery.message string");
    assert!(recovery_message.contains("runner `viteest` is not available"));
    assert!(recovery_message.contains("Did you mean `vitest`?"));
    assert!(recovery_message.contains("Use `effigy test --plan <args>`"));
    let available = parsed["recovery"]["available_suites"]
        .as_array()
        .expect("available_suites array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<&str>>();
    assert_eq!(available, vec!["cargo-test", "vitest"]);
}
