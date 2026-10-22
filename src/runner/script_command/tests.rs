use super::feature_dispatch::{is_runner_dispatch_feature, run_rhai_feature};
use super::{container_exec_operation_from_options, parse_rhai_embedded_command};
use effigy_cli::{Command, DocsArgs, DocsCheckKind, DocsSubcommand};
use effigy_rhai::surface::{
    rhai_feature_descriptors, RhaiFeatureDispatch, FEATURE_DEPLOY_APPLY, FEATURE_DEPLOY_MODEL,
    FEATURE_DEPLOY_REDEPLOY,
};
use serde_json::json;
use std::path::Path;
use std::{ffi::OsString, path::PathBuf};

#[test]
fn parse_rhai_embedded_command_defaults_repo_override_when_missing() {
    let command = parse_rhai_embedded_command(
        Path::new("/tmp/repo"),
        &["docs".to_owned(), "check".to_owned(), "links".to_owned()],
        false,
    )
    .expect("parse rhai embedded command");

    assert!(matches!(
        command,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Links,
                ..
            },
            repo_override: Some(path),
            output_json: false,
        }) if path == Path::new("/tmp/repo")
    ));
}

#[test]
fn parse_rhai_embedded_command_preserves_explicit_repo_override() {
    let command = parse_rhai_embedded_command(
        Path::new("/tmp/repo"),
        &[
            "docs".to_owned(),
            "check".to_owned(),
            "links".to_owned(),
            "--repo".to_owned(),
            "/tmp/other".to_owned(),
        ],
        false,
    )
    .expect("parse rhai embedded command");

    assert!(matches!(
        command,
        Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::Check {
                kind: DocsCheckKind::Links,
                ..
            },
            repo_override: Some(path),
            output_json: false,
        }) if path == Path::new("/tmp/other")
    ));
}

#[test]
fn every_registered_rhai_feature_has_a_runner_dispatch_branch() {
    for descriptor in rhai_feature_descriptors() {
        match descriptor.dispatch {
            RhaiFeatureDispatch::Runner => assert!(
                is_runner_dispatch_feature(descriptor.id),
                "feature `{}` is marked runner-dispatched but has no runner dispatch branch",
                descriptor.id
            ),
            RhaiFeatureDispatch::HostHandled => assert!(
                !is_runner_dispatch_feature(descriptor.id),
                "feature `{}` is marked host-handled but is also runner-dispatched",
                descriptor.id
            ),
        }
    }
}

#[test]
fn container_exec_operation_from_options_preserves_cwd_env_and_stdin_file() {
    let operation = container_exec_operation_from_options(
        Some("db"),
        &["mysql".to_owned(), "app".to_owned()],
        json!({
            "cwd": "/workspace/repo/db",
            "stdin_file": "/workspace/repo/input.sql",
            "env": {
                "MYSQL_PWD": "secret",
                "FOO": "bar"
            }
        }),
    )
    .expect("operation");

    assert_eq!(operation.service.as_deref(), Some("db"));
    assert_eq!(operation.command, vec!["mysql", "app"]);
    assert_eq!(operation.cwd, Some(PathBuf::from("/workspace/repo/db")));
    assert_eq!(
        operation.stdin_file,
        Some(PathBuf::from("/workspace/repo/input.sql"))
    );
    assert_eq!(
        operation.env.get("MYSQL_PWD"),
        Some(&OsString::from("secret"))
    );
    assert_eq!(operation.env.get("FOO"), Some(&OsString::from("bar")));
}

#[test]
fn run_rhai_feature_dispatches_deploy_model_for_fixture_repo() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/render-provider-smoke");
    let output = run_rhai_feature(&repo_root, FEATURE_DEPLOY_MODEL, json!({}))
        .expect("deploy model feature should dispatch");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("json payload");

    assert_eq!(payload["schema"], "deploy.model.v1");
    assert_eq!(payload["app"]["project_name"], "render-provider-smoke-dev");
    assert!(
        payload["services"].is_array(),
        "services should be array: {payload}"
    );
}

#[test]
fn run_rhai_feature_refuses_deploy_execution() {
    let fixture = tempfile::tempdir().expect("create isolated deploy fixture");
    let repo_root = fixture.path();
    std::fs::write(repo_root.join("effigy.toml"), "").expect("write fixture manifest");
    for (feature, operation) in [
        (FEATURE_DEPLOY_APPLY, "apply"),
        (FEATURE_DEPLOY_REDEPLOY, "redeploy"),
    ] {
        for yes in [false, true] {
            let error = run_rhai_feature(
                repo_root,
                feature,
                json!({ "env": "uat", "deployment": "missing", "yes": yes }),
            )
            .expect_err("Rhai deploy execution must refuse regardless of confirmation");
            let details: serde_json::Value =
                serde_json::from_str(error.json_error_details().expect("structured refusal"))
                    .expect("JSON refusal");
            assert_eq!(details["ok"], false);
            assert_eq!(details["reason"], "not_supported");
            assert_eq!(details["operation"], operation);
            assert!(details["message"]
                .as_str()
                .expect("message")
                .contains("not yet supported in v0.15"));
        }
    }
    assert!(!repo_root.join(".effigy/reports/deploy").exists());
    assert!(!repo_root.join(".effigy/runtime/deploy").exists());
}
