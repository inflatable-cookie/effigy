use super::{
    compose_scoped_command, passthrough_has_explicit_package_selection,
    passthrough_has_explicit_workspace_selection,
};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

const NEXTEST_WORKSPACE: &str = "cargo nextest run --workspace";
const CARGO_TEST_WORKSPACE: &str = "cargo test --workspace";

/// The auto `--workspace` flag yields to every explicit package-selection
/// spelling in the passthrough, and the rendered command quotes each form
/// with the auto flag dropped and the scope narrowed.
#[test]
fn package_scope_compose_drops_the_auto_workspace_flag_for_every_package_form() {
    let cases = [
        (
            vec!["-p", "scope-good"],
            "cargo nextest run '-p' 'scope-good'",
        ),
        (
            vec!["--package", "scope-good"],
            "cargo nextest run '--package' 'scope-good'",
        ),
        (
            vec!["--package=scope-good"],
            "cargo nextest run '--package=scope-good'",
        ),
        (vec!["-pscope-good"], "cargo nextest run '-pscope-good'"),
        (
            vec!["-p", "scope-good", "--package", "scope-bad"],
            "cargo nextest run '-p' 'scope-good' '--package' 'scope-bad'",
        ),
    ];
    for (passthrough, expected) in cases {
        let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&passthrough));
        assert_eq!(scoped.command, expected, "form {passthrough:?}");
        assert!(scoped.workspace_flag_dropped, "{passthrough:?}");
        assert!(scoped.package_scope_narrowed, "{passthrough:?}");
    }
}

#[test]
fn package_scope_compose_preserves_cargo_test_fallback_base() {
    let scoped = compose_scoped_command(CARGO_TEST_WORKSPACE, true, &args(&["-p", "scope-good"]));
    assert_eq!(scoped.command, "cargo test '-p' 'scope-good'");
    assert!(scoped.workspace_flag_dropped);
    assert!(scoped.package_scope_narrowed);
}

/// An explicit workspace selection (`--workspace`, or its `--all` alias)
/// also drops the auto flag, but it is not a package narrowing.
#[test]
fn package_scope_compose_yields_the_auto_flag_to_explicit_workspace_selections() {
    for form in [["--workspace"], ["--all"]] {
        let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&form));
        assert_eq!(
            scoped.command,
            format!("cargo nextest run '{}'", form[0]),
            "form {form:?}"
        );
        assert!(scoped.workspace_flag_dropped, "{form:?}");
        assert!(!scoped.package_scope_narrowed, "{form:?}");
    }
}

#[test]
fn package_scope_compose_keeps_auto_workspace_flag_without_package_scope() {
    let scoped =
        compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["--exclude", "scope-bad"]));
    assert_eq!(
        scoped.command,
        "cargo nextest run --workspace '--exclude' 'scope-bad'"
    );
    assert!(!scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_compose_keeps_base_without_passthrough() {
    let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &[]);
    assert_eq!(scoped.command, NEXTEST_WORKSPACE);
    assert!(!scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_compose_keeps_single_package_root_without_workspace_flag() {
    let scoped = compose_scoped_command("cargo nextest run", false, &args(&["-p", "scope-good"]));
    assert_eq!(scoped.command, "cargo nextest run '-p' 'scope-good'");
    assert!(!scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_compose_leaves_non_cargo_bases_untouched() {
    let scoped = compose_scoped_command("bun test", false, &args(&["-p", "scope-good"]));
    assert_eq!(scoped.command, "bun test '-p' 'scope-good'");
    assert!(!scoped.workspace_flag_dropped);
    assert!(!scoped.package_scope_narrowed);
}

#[test]
fn package_scope_compose_quotes_passthrough_arguments() {
    let scoped = compose_scoped_command(
        NEXTEST_WORKSPACE,
        true,
        &args(&["-p", "scope-good", "--", "name filter"]),
    );
    assert_eq!(
        scoped.command,
        "cargo nextest run '-p' 'scope-good' '--' 'name filter'"
    );
    assert!(scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_detection_covers_short_long_equals_and_attached_forms() {
    for arg in [
        "-p",
        "--package",
        "-pscope-good",
        "-p=scope-good",
        "--package=scope-good",
    ] {
        assert!(
            passthrough_has_explicit_package_selection(&args(&[arg])),
            "expected {arg} to count as explicit package selection"
        );
    }
    for arg in [
        "--workspace",
        "--all",
        "--exclude",
        "scope-good",
        "-P",
        "--profile",
    ] {
        assert!(
            !passthrough_has_explicit_package_selection(&args(&[arg])),
            "did not expect {arg} to count as explicit package selection"
        );
    }
}

#[test]
fn package_scope_detection_recognizes_workspace_selection_forms() {
    assert!(passthrough_has_explicit_workspace_selection(&args(&[
        "--workspace"
    ])));
    assert!(passthrough_has_explicit_workspace_selection(&args(&[
        "--all"
    ])));
    assert!(!passthrough_has_explicit_workspace_selection(&args(&[
        "-p",
        "scope-good"
    ])));
    assert!(!passthrough_has_explicit_workspace_selection(&args(&[
        "--workspaceless"
    ])));
}

#[test]
fn package_scope_detection_ignores_selectors_after_the_runner_separator() {
    assert!(!passthrough_has_explicit_package_selection(&args(&[
        "--",
        "-p",
        "scope-good",
    ])));
    assert!(!passthrough_has_explicit_workspace_selection(&args(&[
        "--",
        "--workspace",
    ])));
    assert!(passthrough_has_explicit_workspace_selection(&args(&[
        "--workspace",
        "--",
        "-p",
        "scope-good",
    ])));
    assert!(!passthrough_has_explicit_package_selection(&args(&[
        "--workspace",
        "--",
        "-p",
        "scope-good",
    ])));
    assert!(passthrough_has_explicit_package_selection(&args(&[
        "-p",
        "scope-good",
        "--",
        "--workspace",
    ])));
    assert!(!passthrough_has_explicit_workspace_selection(&args(&[
        "-p",
        "scope-good",
        "--",
        "--workspace",
    ])));
}

#[test]
fn package_scope_compose_keeps_auto_workspace_flag_when_selector_is_past_separator() {
    let scoped =
        compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["--", "-p", "scope-good"]));
    assert_eq!(
        scoped.command,
        "cargo nextest run --workspace '--' '-p' 'scope-good'"
    );
    assert!(!scoped.workspace_flag_dropped);
    assert!(!scoped.package_scope_narrowed);
}

#[test]
fn package_scope_compose_yields_auto_flag_only_to_cargo_level_workspace_selection() {
    let scoped = compose_scoped_command(
        NEXTEST_WORKSPACE,
        true,
        &args(&["--workspace", "--", "-p", "scope-good"]),
    );
    assert_eq!(
        scoped.command,
        "cargo nextest run '--workspace' '--' '-p' 'scope-good'"
    );
    assert!(scoped.workspace_flag_dropped);
    assert!(!scoped.package_scope_narrowed);
}

#[test]
fn package_scope_compose_narrows_only_for_cargo_level_package_selection() {
    let scoped = compose_scoped_command(
        NEXTEST_WORKSPACE,
        true,
        &args(&["-p", "scope-good", "--", "good_passes"]),
    );
    assert_eq!(
        scoped.command,
        "cargo nextest run '-p' 'scope-good' '--' 'good_passes'"
    );
    assert!(scoped.workspace_flag_dropped);
    assert!(scoped.package_scope_narrowed);
}
