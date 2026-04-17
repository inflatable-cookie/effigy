use crate::runner::json_contract_tests::prelude::{execution::*, harness::*, json::*, runtime::*};

#[test]
fn builtin_test_plan_json_contract_has_versioned_shape_and_suite_source_fields() {
    let root = temp_workspace("test-plan-json-contract");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned(), "--json".to_owned()],
        },
        root,
    )
    .expect("run test --plan --json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.test.plan.v1");
    assert!(parsed["targets"].is_array());
    let first = parsed["targets"]
        .as_array()
        .and_then(|targets| targets.first())
        .expect("target entry");
    assert!(first["cargo_env_match"].is_string());
    assert!(first["suite_source"].is_string());
    assert!(first["available_suites"].is_array());
    assert!(first["fallback_chain"].is_array());
}

#[test]
fn builtin_test_results_json_contract_has_versioned_shape_and_hint_fields() {
    let root = temp_workspace("test-results-json-contract");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec![
                "--json".to_owned(),
                "vitest".to_owned(),
                "user-service".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected non-zero test failure");

    let rendered = match err {
        RunnerError::BuiltinTestNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.test.results.v1");
    assert!(parsed["targets"].is_array());
    assert!(parsed["targets"][0]["cargo_env_match"].is_string());
    assert!(parsed["failures"].is_array());
    assert!(parsed["hint"].is_object());
    assert_eq!(
        parsed["hint"]["kind"],
        serde_json::Value::String("selected-suite-filter-no-match".to_owned())
    );
}
