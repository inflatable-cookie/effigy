use super::prelude::*;

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
        let out = run_task_in_workspace(&root, case.request, case.args)
            .expect("deferred run should succeed");
        assert_eq!(out, "");
    }
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
