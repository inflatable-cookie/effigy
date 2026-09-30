use super::{
    compose_scoped_command, passthrough_has_explicit_package_selection,
    passthrough_has_explicit_workspace_selection,
};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

const NEXTEST_WORKSPACE: &str = "cargo nextest run --workspace";
const CARGO_TEST_WORKSPACE: &str = "cargo test --workspace";

#[test]
fn package_scope_compose_drops_auto_workspace_flag_for_short_package_flag() {
    let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["-p", "scope-good"]));
    assert_eq!(scoped.command, "cargo nextest run '-p' 'scope-good'");
    assert!(scoped.workspace_flag_dropped);
    assert!(scoped.package_scope_narrowed);
}

#[test]
fn package_scope_compose_drops_auto_workspace_flag_for_long_package_flag() {
    let scoped =
        compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["--package", "scope-good"]));
    assert_eq!(scoped.command, "cargo nextest run '--package' 'scope-good'");
    assert!(scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_compose_drops_auto_workspace_flag_for_equals_package_form() {
    let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["--package=scope-good"]));
    assert_eq!(scoped.command, "cargo nextest run '--package=scope-good'");
    assert!(scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_compose_drops_auto_workspace_flag_for_attached_short_form() {
    let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["-pscope-good"]));
    assert_eq!(scoped.command, "cargo nextest run '-pscope-good'");
    assert!(scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_compose_drops_auto_workspace_flag_for_repeated_selections() {
    let scoped = compose_scoped_command(
        NEXTEST_WORKSPACE,
        true,
        &args(&["-p", "scope-good", "--package", "scope-bad"]),
    );
    assert_eq!(
        scoped.command,
        "cargo nextest run '-p' 'scope-good' '--package' 'scope-bad'"
    );
    assert!(scoped.workspace_flag_dropped);
}

#[test]
fn package_scope_compose_preserves_cargo_test_fallback_base() {
    let scoped = compose_scoped_command(CARGO_TEST_WORKSPACE, true, &args(&["-p", "scope-good"]));
    assert_eq!(scoped.command, "cargo test '-p' 'scope-good'");
    assert!(scoped.workspace_flag_dropped);
    assert!(scoped.package_scope_narrowed);
}

#[test]
fn package_scope_compose_yields_auto_flag_to_explicit_workspace_selection() {
    let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["--workspace"]));
    assert_eq!(scoped.command, "cargo nextest run '--workspace'");
    assert!(scoped.workspace_flag_dropped);
    assert!(!scoped.package_scope_narrowed);
}

#[test]
fn package_scope_compose_yields_auto_flag_to_explicit_all_alias() {
    let scoped = compose_scoped_command(NEXTEST_WORKSPACE, true, &args(&["--all"]));
    assert_eq!(scoped.command, "cargo nextest run '--all'");
    assert!(scoped.workspace_flag_dropped);
    assert!(!scoped.package_scope_narrowed);
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
