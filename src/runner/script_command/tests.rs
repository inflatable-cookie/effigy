use super::{
    container_exec_operation_from_options, is_runner_dispatch_feature, parse_rhai_embedded_command,
    run_rhai_feature,
};
use effigy_cli::{Command, DocsArgs, DocsCheckKind, DocsSubcommand};
use effigy_rhai::surface::{
    FEATURE_DEPLOY_APPLY, FEATURE_DEPLOY_MODEL, FEATURE_DISTRIBUTION_VALIDATE_ARTIFACTS,
    FEATURE_DISTRIBUTION_VALIDATE_METADATA, FEATURE_NAMES,
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
    for feature in FEATURE_NAMES {
        assert!(
            is_runner_dispatch_feature(feature) || *feature == "state.capture_set",
            "feature `{feature}` is registered in effigy-rhai but is neither runner-dispatched nor explicitly host-handled"
        );
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
fn run_rhai_feature_dispatches_distribution_validate_metadata_for_current_repo() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let current_tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    let output = run_rhai_feature(
        repo_root,
        FEATURE_DISTRIBUTION_VALIDATE_METADATA,
        json!({ "tag": current_tag }),
    )
    .expect("distribution validate metadata should dispatch");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("json payload");

    assert_eq!(payload["schema"], "effigy.distribution.metadata.v1");
    assert_eq!(payload["ok"], true);
}

#[test]
fn run_rhai_feature_preserves_deploy_apply_confirmation_guard() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/render-provider-smoke");
    let error = run_rhai_feature(
        &repo_root,
        FEATURE_DEPLOY_APPLY,
        json!({
            "env": "uat",
            "yes": false
        }),
    )
    .expect_err("deploy apply without yes should fail");

    let rendered = error.to_string();
    assert!(
        rendered.contains("`deploy apply` is plan-only unless `--yes` is supplied"),
        "unexpected error: {rendered}"
    );
}

#[test]
fn run_rhai_feature_preserves_distribution_required_input_guard() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let error = run_rhai_feature(
        repo_root,
        FEATURE_DISTRIBUTION_VALIDATE_ARTIFACTS,
        json!({
            "expect_homebrew": true
        }),
    )
    .expect_err("distribution validate artifacts without artifacts_dir should fail");

    let rendered = error.to_string();
    assert!(
        rendered.contains("`artifacts_dir` is required"),
        "unexpected error: {rendered}"
    );
}
