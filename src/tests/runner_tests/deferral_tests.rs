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

#[test]
fn run_manifest_task_defers_when_unprefixed_task_missing() {
    let _guard = lock_test();
    let root = temp_workspace("defer-missing");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n",
    );

    let out = run_task(&root, "unknown-task", &[]).expect("deferred run should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_defers_and_supports_request_and_args_tokens() {
    let _guard = lock_test();
    let root = temp_workspace("defer-tokens");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"test {request} = 'unknown-task' && test {args} = '--dry-run'\"\n",
    );

    let out = run_task(&root, "unknown-task", &["--dry-run"])
        .expect("deferred token substitution should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_defers_for_path_like_request_when_prefix_not_found() {
    let _guard = lock_test();
    let root = temp_workspace("defer-path-like-request");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"test {request} = 'services/api/dev' && test {args} = '--watch'\"\n",
    );

    let out = run_task(&root, "services/api/dev", &["--watch"])
        .expect("path-like deferred request should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let _guard = lock_test();
    let root = temp_workspace("defer-prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(&root.join("effigy.toml"), "[defer]\nrun = \"false\"\n");
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
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n",
    );

    std::env::set_var("EFFIGY_DEFER_DEPTH", "1");
    let err = run_task(&root, "unknown-task", &[]).expect_err("loop guard should fail");
    std::env::remove_var("EFFIGY_DEFER_DEPTH");
    assert_defer_loop_detected(err, 1);
}

#[test]
fn run_manifest_task_implicitly_defers_to_root_when_no_configured_deferral() {
    let _guard = lock_test();
    let root = temp_workspace("implicit-root-defer");
    fs::write(root.join("effigy.json"), "{}\n").expect("write effigy marker");
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");

    let args_log = root.join("composer-args.log");
    let _env = setup_composer_stub(
        &root,
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\n",
        &args_log,
    );
    let out =
        run_task(&root, "version", &["--dry-run"]).expect("implicit root deferral should succeed");

    assert_eq!(out, "");
    let args = fs::read_to_string(args_log).expect("read composer args");
    assert_eq!(args, "global\nexec\neffigy\n--\nversion\n--dry-run\n");
}

#[test]
fn run_manifest_task_does_not_implicitly_defer_without_effigy_json_marker() {
    let _guard = lock_test();
    let root = temp_workspace("implicit-root-defer-missing-effigy-json");
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");

    let marker = root.join("composer-called.log");
    let _env = setup_composer_stub(
        &root,
        "#!/bin/sh\nprintf called > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit 0\n",
        &marker,
    );
    let err = run_task(&root, "version", &[])
        .expect_err("implicit deferral should not run without effigy.json marker");
    assert_task_not_found_any(err);
    assert!(
        !marker.exists(),
        "composer fallback should not be invoked when effigy.json is missing"
    );
}

#[test]
fn run_manifest_task_does_not_implicitly_defer_without_composer_json_marker() {
    let _guard = lock_test();
    let root = temp_workspace("implicit-root-defer-missing-composer-json");
    fs::write(root.join("effigy.json"), "{}\n").expect("write effigy marker");

    let marker = root.join("composer-called.log");
    let _env = setup_composer_stub(
        &root,
        "#!/bin/sh\nprintf called > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit 0\n",
        &marker,
    );
    let err = run_task(&root, "version", &[])
        .expect_err("implicit deferral should not run without composer.json marker");
    assert_task_not_found_any(err);
    assert!(
        !marker.exists(),
        "composer fallback should not be invoked when composer.json is missing"
    );
}

#[test]
fn run_manifest_task_does_not_implicitly_defer_when_markers_exist_only_in_nested_directory() {
    let _guard = lock_test();
    let root = temp_workspace("implicit-root-defer-nested-markers-only");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("mkdir nested");
    fs::write(nested.join("effigy.json"), "{}\n").expect("write nested effigy marker");
    fs::write(nested.join("composer.json"), "{}\n").expect("write nested composer marker");

    let marker = root.join("composer-called.log");
    let _env = setup_composer_stub(
        &root,
        "#!/bin/sh\nprintf called > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit 0\n",
        &marker,
    );
    let err = run_task(&root, "version", &[])
        .expect_err("implicit deferral should not run from nested marker files");
    assert_task_not_found_any(err);
    assert!(
        !marker.exists(),
        "composer fallback should not be invoked when markers are only nested"
    );
}

#[test]
fn run_manifest_task_explicit_deferral_wins_over_implicit_root_deferral() {
    let _guard = lock_test();
    let root = temp_workspace("explicit-over-implicit");
    fs::write(root.join("effigy.json"), "{}\n").expect("write effigy marker");
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf explicit\"\n",
    );

    let marker = root.join("composer-called.log");
    let _env = setup_composer_stub(
        &root,
        "#!/bin/sh\nprintf called > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit 99\n",
        &marker,
    );
    let out = run_task(&root, "missing", &[]).expect("explicit deferral should succeed");

    assert_eq!(out, "");
    assert!(!marker.exists(), "composer fallback should not be invoked");
}
