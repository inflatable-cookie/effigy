use super::*;

fn run_task(root: &PathBuf, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.clone(),
    )
}

struct DeferredTaskCase {
    workspace: &'static str,
    defer_run: &'static str,
    request: &'static str,
    args: &'static [&'static str],
}

struct ImplicitFallbackDisabledCase {
    workspace: &'static str,
    create_effigy_marker: bool,
    create_composer_marker: bool,
    use_nested_markers: bool,
}

enum ImplicitDeferralExpectation {
    DeferredViaComposer { expected_args: &'static str },
    TaskNotFoundWithoutComposer,
    ExplicitDeferralWithoutComposer,
}

struct ImplicitDeferralCase {
    workspace: &'static str,
    create_effigy_marker: bool,
    create_composer_marker: bool,
    use_nested_markers: bool,
    explicit_defer_run: Option<&'static str>,
    request: &'static str,
    args: &'static [&'static str],
    expectation: ImplicitDeferralExpectation,
}

fn assert_task_not_found_any(err: RunnerError) {
    match err {
        RunnerError::TaskNotFoundAny { .. } => {}
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_defer_loop_detected(err: RunnerError, expected_depth: u8) {
    match err {
        RunnerError::DeferLoopDetected { depth } => assert_eq!(depth, expected_depth),
        other => panic!("unexpected error: {other}"),
    }
}

fn write_executable(path: &PathBuf, script: &str) {
    fs::write(path, script).expect("write executable");
    let mut perms = fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod");
}

fn setup_composer_stub(root: &PathBuf, script: &str, marker_file: &PathBuf) -> EnvGuard {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    write_executable(&bin_dir.join("composer"), script);

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("SHELL", Some("/bin/sh".to_owned())),
        (
            "EFFIGY_TEST_COMPOSER_ARGS_FILE",
            Some(marker_file.display().to_string()),
        ),
    ])
}

fn write_defer_manifest(root: &PathBuf, defer_run: &str) {
    write_manifest(
        &root.join("effigy.toml"),
        &format!("[defer]\nrun = \"{defer_run}\"\n"),
    );
}

fn write_implicit_deferral_markers(
    root: &PathBuf,
    create_effigy_marker: bool,
    create_composer_marker: bool,
    nested: bool,
) {
    let marker_root = if nested {
        let nested_root = root.join("nested");
        fs::create_dir_all(&nested_root).expect("mkdir nested");
        nested_root
    } else {
        root.clone()
    };

    if create_effigy_marker {
        fs::write(marker_root.join("effigy.json"), "{}\n").expect("write effigy marker");
    }
    if create_composer_marker {
        fs::write(marker_root.join("composer.json"), "{}\n").expect("write composer marker");
    }
}

fn composer_script(exit_code: u8) -> String {
    format!(
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit {exit_code}\n"
    )
}

fn run_implicit_deferral_case(case: &ImplicitDeferralCase) {
    let root = temp_workspace(case.workspace);
    write_implicit_deferral_markers(
        &root,
        case.create_effigy_marker,
        case.create_composer_marker,
        case.use_nested_markers,
    );
    if let Some(defer_run) = case.explicit_defer_run {
        write_defer_manifest(&root, defer_run);
    }

    let marker = root.join("composer-args.log");
    let composer_exit = match case.expectation {
        ImplicitDeferralExpectation::ExplicitDeferralWithoutComposer => 99,
        _ => 0,
    };
    let _env = setup_composer_stub(&root, &composer_script(composer_exit), &marker);

    match case.expectation {
        ImplicitDeferralExpectation::DeferredViaComposer { expected_args } => {
            let out = run_task(&root, case.request, case.args)
                .expect("implicit root deferral should succeed");
            assert_eq!(out, "");
            let args = fs::read_to_string(marker).expect("read composer args");
            assert_eq!(args, expected_args);
        }
        ImplicitDeferralExpectation::TaskNotFoundWithoutComposer => {
            let err = run_task(&root, case.request, case.args).expect_err(
                "implicit deferral should not run when required root markers are missing",
            );
            assert_task_not_found_any(err);
            assert!(
                !marker.exists(),
                "composer fallback should not be invoked when required root markers are unavailable"
            );
        }
        ImplicitDeferralExpectation::ExplicitDeferralWithoutComposer => {
            let out = run_task(&root, case.request, case.args)
                .expect("explicit deferral should succeed");
            assert_eq!(out, "");
            assert!(!marker.exists(), "composer fallback should not be invoked");
        }
    }
}

#[test]
fn run_manifest_task_defers_when_task_missing_with_token_support() {
    let _guard = lock_test();
    let cases = [
        DeferredTaskCase {
            workspace: "defer-missing",
            defer_run: "printf deferred",
            request: "unknown-task",
            args: &[],
        },
        DeferredTaskCase {
            workspace: "defer-tokens",
            defer_run: "test {request} = 'unknown-task' && test {args} = '--dry-run'",
            request: "unknown-task",
            args: &["--dry-run"],
        },
        DeferredTaskCase {
            workspace: "defer-path-like-request",
            defer_run: "test {request} = 'services/api/dev' && test {args} = '--watch'",
            request: "services/api/dev",
            args: &["--watch"],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_defer_manifest(&root, case.defer_run);
        let out = run_task(&root, case.request, case.args).expect("deferred run should succeed");
        assert_eq!(out, "");
    }
}

#[test]
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let _guard = lock_test();
    let root = temp_workspace("defer-prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_defer_manifest(&root, "false");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[defer]\nrun = \"printf farmyard-deferred\"\n",
    );

    let out = run_task(&root, "farmyard/missing", &[]).expect("prefixed deferral should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_deferral_loop_guard_fails() {
    let _guard = lock_test();
    let root = temp_workspace("defer-loop");
    write_defer_manifest(&root, "printf deferred");

    let _env = EnvGuard::set_many(&[("EFFIGY_DEFER_DEPTH", Some("1".to_owned()))]);
    let err = run_task(&root, "unknown-task", &[]).expect_err("loop guard should fail");
    assert_defer_loop_detected(err, 1);
}

#[test]
fn run_manifest_task_implicit_deferral_matrix() {
    let _guard = lock_test();
    let fallback_disabled_cases = [
        ImplicitFallbackDisabledCase {
            workspace: "implicit-root-defer-missing-effigy-json",
            create_effigy_marker: false,
            create_composer_marker: true,
            use_nested_markers: false,
        },
        ImplicitFallbackDisabledCase {
            workspace: "implicit-root-defer-missing-composer-json",
            create_effigy_marker: true,
            create_composer_marker: false,
            use_nested_markers: false,
        },
        ImplicitFallbackDisabledCase {
            workspace: "implicit-root-defer-nested-markers-only",
            create_effigy_marker: true,
            create_composer_marker: true,
            use_nested_markers: true,
        },
    ];

    let mut cases = vec![ImplicitDeferralCase {
        workspace: "implicit-root-defer",
        create_effigy_marker: true,
        create_composer_marker: true,
        use_nested_markers: false,
        explicit_defer_run: None,
        request: "version",
        args: &["--dry-run"],
        expectation: ImplicitDeferralExpectation::DeferredViaComposer {
            expected_args: "global\nexec\neffigy\n--\nversion\n--dry-run\n",
        },
    }];
    cases.extend(
        fallback_disabled_cases
            .iter()
            .map(|case| ImplicitDeferralCase {
                workspace: case.workspace,
                create_effigy_marker: case.create_effigy_marker,
                create_composer_marker: case.create_composer_marker,
                use_nested_markers: case.use_nested_markers,
                explicit_defer_run: None,
                request: "version",
                args: &[],
                expectation: ImplicitDeferralExpectation::TaskNotFoundWithoutComposer,
            }),
    );
    cases.push(ImplicitDeferralCase {
        workspace: "explicit-over-implicit",
        create_effigy_marker: true,
        create_composer_marker: true,
        use_nested_markers: false,
        explicit_defer_run: Some("printf explicit"),
        request: "missing",
        args: &[],
        expectation: ImplicitDeferralExpectation::ExplicitDeferralWithoutComposer,
    });

    for case in &cases {
        run_implicit_deferral_case(case);
    }
}
