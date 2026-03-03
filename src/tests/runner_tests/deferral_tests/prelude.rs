pub(super) use super::super::prelude::*;

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
    DeferredViaComposer { expected_args: &'static str },
    TaskNotFoundWithoutComposer,
    ExplicitDeferralWithoutComposer,
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

pub(super) fn setup_composer_stub(root: &PathBuf, script: &str, marker_file: &PathBuf) -> EnvGuard {
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

pub(super) fn composer_script(exit_code: u8) -> String {
    format!(
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit {exit_code}\n"
    )
}

pub(super) fn run_implicit_deferral_case(case: &ImplicitDeferralCase) {
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
            let out = run_task_in_workspace(&root, case.request, case.args)
                .expect("implicit root deferral should succeed");
            assert_eq!(out, "");
            let args = fs::read_to_string(marker).expect("read composer args");
            assert_eq!(args, expected_args);
        }
        ImplicitDeferralExpectation::TaskNotFoundWithoutComposer => {
            let err = run_task_in_workspace(&root, case.request, case.args).expect_err(
                "implicit deferral should not run when required root markers are missing",
            );
            assert_task_not_found_any(err);
            assert!(
                !marker.exists(),
                "composer fallback should not be invoked when required root markers are unavailable"
            );
        }
        ImplicitDeferralExpectation::ExplicitDeferralWithoutComposer => {
            let out = run_task_in_workspace(&root, case.request, case.args)
                .expect("explicit deferral should succeed");
            assert_eq!(out, "");
            assert!(!marker.exists(), "composer fallback should not be invoked");
        }
    }
}
