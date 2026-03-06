pub(super) use super::super::prelude::{
    assert_case_table, assert_file_text_equals, assert_output_equals, assert_path_missing,
    lock_test, run_task_in_workspace, temp_workspace, write_defer_manifest, write_executable,
    write_manifest, EnvGuard, Path, PathBuf, RunnerError, fs,
};

pub(super) struct DeferredTaskCase {
    pub(super) workspace: &'static str,
    pub(super) defer_run: &'static str,
    pub(super) request: &'static str,
    pub(super) args: &'static [&'static str],
}

pub(super) struct ImplicitFallbackDisabledCase {
    pub(super) workspace: &'static str,
    pub(super) create_effigy_marker: bool,
    pub(super) create_composer_marker: bool,
    pub(super) use_nested_markers: bool,
}

pub(super) enum ImplicitDeferralExpectation {
    Deferred { expected_args: &'static str },
    TaskNotFound,
    ExplicitDeferral,
}

pub(super) struct ImplicitDeferralCase {
    pub(super) workspace: &'static str,
    pub(super) create_effigy_marker: bool,
    pub(super) create_composer_marker: bool,
    pub(super) use_nested_markers: bool,
    pub(super) explicit_defer_run: Option<&'static str>,
    pub(super) request: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) expectation: ImplicitDeferralExpectation,
}

pub(super) fn assert_task_not_found_any(err: RunnerError) {
    match err {
        RunnerError::TaskNotFoundAny { .. } => {}
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn assert_defer_loop_detected(err: RunnerError, expected_depth: u8) {
    match err {
        RunnerError::DeferLoopDetected { depth } => assert_eq!(depth, expected_depth),
        other => panic!("unexpected error: {other}"),
    }
}

pub(super) fn setup_composer_stub(root: &Path, script: &str, marker_file: &Path) -> EnvGuard {
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

pub(super) fn write_implicit_deferral_markers(
    root: &Path,
    create_effigy_marker: bool,
    create_composer_marker: bool,
    nested: bool,
) {
    let marker_root = if nested {
        let nested_root = root.join("nested");
        fs::create_dir_all(&nested_root).expect("mkdir nested");
        nested_root
    } else {
        root.to_path_buf()
    };

    if create_effigy_marker {
        fs::write(marker_root.join("effigy.json"), "{}\n").expect("write effigy marker");
    }
    if create_composer_marker {
        fs::write(marker_root.join("composer.json"), "{}\n").expect("write composer marker");
    }
}

pub(super) fn composer_script(exit_code: u8) -> String {
    format!(
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit {exit_code}\n"
    )
}

pub(super) fn workspace_with_optional_defer_manifest(
    workspace: &str,
    defer_run: Option<&str>,
) -> PathBuf {
    let root = temp_workspace(workspace);
    if let Some(defer_run) = defer_run {
        write_defer_manifest(&root, defer_run);
    }
    root
}

fn for_each_workspace_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    defer_run_of: impl Fn(&T) -> Option<&str>,
    mut assert_case: impl FnMut(&T, PathBuf),
) {
    assert_case_table(cases.iter(), |case| {
        let root = workspace_with_optional_defer_manifest(workspace_of(case), defer_run_of(case));
        assert_case(case, root);
    });
}

fn run_case_task(root: &Path, request: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_task_in_workspace(root, request, args)
}

pub(super) fn run_task_expect_empty_output(
    root: &Path,
    request: &str,
    args: &[&str],
    context: &str,
) {
    let out = run_case_task(root, request, args).expect(context);
    assert_output_equals(&out, "");
}

pub(super) fn assert_deferred_task_case_table(cases: &[DeferredTaskCase]) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |case| Some(case.defer_run),
        |case, root| {
            run_task_expect_empty_output(
                &root,
                case.request,
                case.args,
                "deferred run should succeed",
            );
        },
    );
}

pub(super) fn assert_implicit_deferral_case_table(cases: &[ImplicitDeferralCase]) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |case| case.explicit_defer_run,
        run_implicit_deferral_case,
    );
}

fn composer_exit_code(expectation: &ImplicitDeferralExpectation) -> u8 {
    match expectation {
        ImplicitDeferralExpectation::ExplicitDeferral => 99,
        _ => 0,
    }
}

fn assert_implicit_deferred_case(
    root: &Path,
    marker: &Path,
    case: &ImplicitDeferralCase,
    expected_args: &str,
) {
    run_task_expect_empty_output(
        root,
        case.request,
        case.args,
        "implicit root deferral should succeed",
    );
    assert_file_text_equals(marker, expected_args);
}

fn assert_implicit_task_not_found_case(root: &Path, marker: &Path, case: &ImplicitDeferralCase) {
    let err = run_case_task(root, case.request, case.args)
        .expect_err("implicit deferral should not run when required root markers are missing");
    assert_task_not_found_any(err);
    assert_path_missing(
        marker,
        "composer fallback marker for missing required root markers",
    );
}

fn assert_implicit_explicit_deferral_case(root: &Path, marker: &Path, case: &ImplicitDeferralCase) {
    run_task_expect_empty_output(
        root,
        case.request,
        case.args,
        "explicit deferral should succeed",
    );
    assert_path_missing(marker, "composer fallback marker");
}

pub(super) fn run_implicit_deferral_case(case: &ImplicitDeferralCase, root: PathBuf) {
    write_implicit_deferral_markers(
        &root,
        case.create_effigy_marker,
        case.create_composer_marker,
        case.use_nested_markers,
    );

    let marker = root.join("composer-args.log");
    let composer_exit = composer_exit_code(&case.expectation);
    let _env = setup_composer_stub(&root, &composer_script(composer_exit), &marker);

    match &case.expectation {
        ImplicitDeferralExpectation::Deferred { expected_args } => {
            assert_implicit_deferred_case(&root, &marker, case, expected_args);
        }
        ImplicitDeferralExpectation::TaskNotFound => {
            assert_implicit_task_not_found_case(&root, &marker, case);
        }
        ImplicitDeferralExpectation::ExplicitDeferral => {
            assert_implicit_explicit_deferral_case(&root, &marker, case);
        }
    }
}
