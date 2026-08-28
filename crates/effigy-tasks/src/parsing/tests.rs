use super::*;

#[test]
fn selector_parse_and_render_are_stable() {
    assert_eq!(
        parse_task_selector("api/test").expect("selector"),
        TaskSelector {
            prefix: Some("api".to_owned()),
            task_name: "test".to_owned(),
        }
    );
    assert_eq!(
        render_task_selector(&TaskSelector {
            prefix: Some("api".to_owned()),
            task_name: "test".to_owned(),
        }),
        "api/test"
    );
}

#[test]
fn runtime_args_parse_is_stable() {
    let parsed = parse_task_runtime_args(&[
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--env-schema".to_owned(),
        "env.toml".to_owned(),
        "--verbose-root".to_owned(),
        "--watch".to_owned(),
    ])
    .expect("runtime args");
    assert_eq!(parsed.repo_override, Some(PathBuf::from("/tmp/repo")));
    assert_eq!(parsed.env_schema_override, Some(PathBuf::from("env.toml")));
    assert!(parsed.verbose_root);
    assert_eq!(parsed.passthrough, vec!["--watch".to_owned()]);
}

#[test]
fn runtime_args_stop_at_passthrough_delimiter() {
    let parsed = parse_task_runtime_args(&[
        "--verbose-root".to_owned(),
        "--".to_owned(),
        "--repo".to_owned(),
        "/tmp/task-repo".to_owned(),
        "--env-schema".to_owned(),
        "env.toml".to_owned(),
    ])
    .expect("runtime args");
    assert_eq!(parsed.repo_override, None);
    assert!(parsed.verbose_root);
    assert_eq!(parsed.env_schema_override, None);
    assert_eq!(
        parsed.passthrough,
        vec![
            "--".to_owned(),
            "--repo".to_owned(),
            "/tmp/task-repo".to_owned(),
            "--env-schema".to_owned(),
            "env.toml".to_owned(),
        ]
    );
}

#[test]
fn runtime_args_keep_repo_override_before_passthrough_delimiter() {
    let parsed = parse_task_runtime_args(&[
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--".to_owned(),
        "--repo".to_owned(),
        "/tmp/task-repo".to_owned(),
    ])
    .expect("runtime args");
    assert_eq!(parsed.repo_override, Some(PathBuf::from("/tmp/repo")));
    assert_eq!(
        parsed.passthrough,
        vec![
            "--".to_owned(),
            "--repo".to_owned(),
            "/tmp/task-repo".to_owned(),
        ]
    );
}

#[test]
fn command_passthrough_args_strips_leading_delimiter() {
    assert_eq!(
        crate::command_passthrough_args(&["--".to_owned(), "src/foo.test.ts".to_owned(),]),
        &["src/foo.test.ts".to_owned()]
    );
    assert_eq!(
        crate::render_passthrough_args(&["--".to_owned(), "src/foo.test.ts".to_owned(),]),
        "'src/foo.test.ts'"
    );
}

#[test]
fn command_passthrough_args_keeps_embedded_delimiter() {
    assert_eq!(
        crate::command_passthrough_args(&[
            "unit".to_owned(),
            "--".to_owned(),
            "src/foo.test.ts".to_owned(),
        ]),
        &[
            "unit".to_owned(),
            "--".to_owned(),
            "src/foo.test.ts".to_owned(),
        ]
    );
}
