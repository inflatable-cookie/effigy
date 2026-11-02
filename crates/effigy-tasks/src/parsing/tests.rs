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

/// Effigy's own flags parse from the unbounded form; everything unknown or
/// after `--` passes through untouched.
#[test]
fn runtime_args_parse_effigy_flags() {
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
    assert_eq!(parsed.lock_wait_ms, None);
    assert_eq!(parsed.passthrough, vec!["--watch".to_owned()]);

    let parsed = parse_task_runtime_args(&[
        "--lock-wait-ms".to_owned(),
        "150".to_owned(),
        "--verbose-root".to_owned(),
    ])
    .expect("runtime args");
    assert_eq!(parsed.lock_wait_ms, Some(150));
    assert!(parsed.verbose_root);
}

#[test]
fn runtime_args_reject_invalid_lock_wait_ms() {
    let err = parse_task_runtime_args(&["--lock-wait-ms".to_owned(), "-1".to_owned()])
        .expect_err("invalid lock wait");
    assert!(err.contains("--lock-wait-ms must be a non-negative integer"));
}

/// Builtin suite names normalize to their canonical selector form; anything
/// else is not a builtin suite.
#[test]
fn builtin_test_suite_names_normalize_to_their_canonical_form() {
    for (raw, canonical) in [
        ("vitest", Some("vitest")),
        ("nextest", Some("cargo-nextest")),
        ("cargo-nextest", Some("cargo-nextest")),
        ("cargo-test", Some("cargo-test")),
        ("pytest", None),
        ("", None),
    ] {
        assert_eq!(normalize_builtin_test_suite(raw), canonical, "{raw}");
    }
}

/// Everything from `--` on is passthrough verbatim; only the flags before it
/// are Effigy's.
#[test]
fn runtime_args_split_at_the_passthrough_delimiter() {
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

/// Task-reference passthrough: the leading delimiter is stripped for the
/// command's argv, rendering re-adds it for nested invocations, and an
/// embedded delimiter is kept verbatim.
#[test]
fn task_reference_passthrough_args_strip_render_and_keep_delimiters() {
    assert_eq!(
        crate::command_passthrough_args(&["--".to_owned(), "src/foo.test.ts".to_owned(),]),
        &["src/foo.test.ts".to_owned()]
    );
    assert_eq!(
        crate::render_template_args(&["--".to_owned(), "src/foo.test.ts".to_owned(),]),
        "'src/foo.test.ts'"
    );
    assert_eq!(
        crate::render_passthrough_args(&["--".to_owned(), "--force".to_owned(),]),
        "'--' '--force'"
    );
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
