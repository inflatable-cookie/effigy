use crate::runner::json_contract_tests::prelude::{execution::*, harness::*, json::*};

#[test]
fn catalog_task_plan_json_contract_has_versioned_shape() {
    let root = temp_workspace("task-plan-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    );

    let parsed = run_invocation_json(root, "build", &["--json", "--plan"]);
    assert_schema_v1(&parsed, "effigy.task.plan.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["executed"], false);
    assert_eq!(parsed["task"], "build");
    assert_eq!(parsed["selector"], "build");
    assert!(parsed["command"].as_str().is_some_and(|command| command.contains("printf build-ok")));
    assert!(parsed["catalog"]["root"].is_string());
    assert!(parsed["catalog"]["manifest"].is_string());
}

#[test]
fn catalog_task_plan_json_contract_covers_unavailable_command() {
    let root = temp_workspace("task-plan-json-missing-bin");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.missing]\nrun = \"/definitely-missing-effigy-plan-bin --flag\"\n",
    );

    let parsed = run_invocation_json(root, "missing", &["--json", "--plan"]);
    assert_schema_v1(&parsed, "effigy.task.plan.v1");
    assert_eq!(parsed["executed"], false);
    assert_eq!(parsed["task"], "missing");
    assert_eq!(
        parsed["command"],
        "/definitely-missing-effigy-plan-bin --flag"
    );
}
