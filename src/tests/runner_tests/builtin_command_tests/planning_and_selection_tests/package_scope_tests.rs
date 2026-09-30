use crate::runner::tests::prelude::harness::EnvGuard;
use crate::runner::tests::prelude::{
    assert_builtin_test_non_zero, assert_invocation_error_contains, assert_output_contains_all,
    assert_output_excludes_all, fs, lock_test, parse_json_output_with_schema, run_builtin_err,
    run_builtin_ok, temp_workspace, write_executable,
};
use std::path::Path;
use std::path::PathBuf;

const NARROWED_EVIDENCE: &str = "auto-added workspace flag was omitted";

/// Two-package Cargo workspace whose `scope-bad` member fails when selected:
/// a passing run proves the failing package was not selected, and a failing
/// run proves it was.
fn write_two_package_cargo_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"scope-good\", \"scope-bad\"]\n",
    )
    .expect("write workspace toml");
    for (dir, test_body) in [
        (
            "scope-good",
            "#[test]\n    fn good_passes() {\n        assert_eq!(2 + 2, 4);\n    }",
        ),
        (
            "scope-bad",
            "#[test]\n    fn bad_fails_if_selected() {\n        panic!(\"scope-bad must not be selected\");\n    }",
        ),
    ] {
        let package = root.join(dir);
        fs::create_dir_all(package.join("src")).expect("mkdir package src");
        fs::write(
            package.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{dir}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
            ),
        )
        .expect("write package toml");
        fs::write(
            package.join("src").join("lib.rs"),
            format!("#[cfg(test)]\nmod tests {{\n    {test_body}\n}}\n"),
        )
        .expect("write lib.rs");
    }
}

fn plan_command_at(root: PathBuf, args: &[&str]) -> serde_json::Value {
    let json = run_builtin_ok(root, "test", args);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    parsed["targets"][0]["commands"][0].clone()
}

#[test]
fn run_manifest_task_builtin_test_plan_package_scope_narrows_cargo_command() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-package-scope-plan-narrows");
    write_two_package_cargo_workspace(&root);

    let command = plan_command_at(
        root.to_path_buf(),
        &["--plan", "--json", "-p", "scope-good"],
    );
    let command = command.as_str().expect("plan command string");
    assert!(
        command.contains("run '-p' 'scope-good'"),
        "unexpected plan command: {command}"
    );
    assert!(
        !command.contains("--workspace"),
        "package scope must not widen to the workspace: {command}"
    );

    let text = run_builtin_ok(root.to_path_buf(), "test", &["--plan", "-p", "scope-good"]);
    assert_output_contains_all(
        &text,
        &["Test Plan", "'-p' 'scope-good'", NARROWED_EVIDENCE],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_package_scope_covers_selection_forms() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-package-scope-plan-forms");
    write_two_package_cargo_workspace(&root);

    let equals_form = plan_command_at(
        root.to_path_buf(),
        &["--plan", "--json", "--package=scope-good"],
    );
    let equals_form = equals_form.as_str().expect("plan command string");
    assert!(equals_form.contains("'--package=scope-good'"));
    assert!(!equals_form.contains("--workspace"));

    let repeated = plan_command_at(
        root.to_path_buf(),
        &["--plan", "--json", "-p", "scope-good", "-p", "scope-bad"],
    );
    let repeated = repeated.as_str().expect("plan command string");
    assert!(repeated.contains("'-p' 'scope-good' '-p' 'scope-bad'"));
    assert!(!repeated.contains("--workspace"));

    let workspace = plan_command_at(root.to_path_buf(), &["--plan", "--json", "--workspace"]);
    let workspace = workspace.as_str().expect("plan command string");
    assert!(
        workspace.contains("run '--workspace'"),
        "explicit workspace selection must be preserved: {workspace}"
    );

    let excluded = plan_command_at(
        root.to_path_buf(),
        &["--plan", "--json", "--workspace", "--exclude", "scope-bad"],
    );
    let excluded = excluded.as_str().expect("plan command string");
    assert!(excluded.contains("run '--workspace' '--exclude' 'scope-bad'"));

    let bare_exclusion = plan_command_at(
        root.to_path_buf(),
        &["--plan", "--json", "--exclude", "scope-bad"],
    );
    let bare_exclusion = bare_exclusion.as_str().expect("plan command string");
    assert!(
        bare_exclusion.contains("run --workspace '--exclude' 'scope-bad'"),
        "exclusion without an explicit workspace keeps the auto workspace scope: {bare_exclusion}"
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_package_scope_ignores_selectors_after_runner_separator() {
    let root = temp_workspace("builtin-test-package-scope-runner-separator");
    write_two_package_cargo_workspace(&root);

    // A second `--` survives into the passthrough and becomes the runner's
    // own separator: the trailing `--workspace` is a test-binary argument,
    // never a Cargo selector, so the package scope stands and nothing is
    // rejected.
    let command = plan_command_at(
        root.to_path_buf(),
        &[
            "--plan",
            "--json",
            "-p",
            "scope-good",
            "--",
            "good_passes",
            "--",
            "--workspace",
        ],
    );
    let command = command.as_str().expect("plan command string");
    assert!(
        command.contains("run '-p' 'scope-good' 'good_passes' '--' '--workspace'"),
        "unexpected plan command: {command}"
    );
    assert!(!command.starts_with("cargo nextest run --workspace"));

    // Pre-separator `-p` scopes Cargo while post-boundary tokens flow to the
    // runner as a matching name filter.
    let out = run_builtin_ok(
        root.to_path_buf(),
        "test",
        &["-p", "scope-good", "--", "good_passes"],
    );
    assert_output_contains_all(&out, &["Test Results", "targets:", "root"]);
}

#[test]
fn run_manifest_task_builtin_test_plan_package_scope_rejects_workspace_plus_package() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-package-scope-rejects-ambiguous");
    write_two_package_cargo_workspace(&root);

    let recovery = run_builtin_ok(
        root.to_path_buf(),
        "test",
        &["--plan", "--workspace", "-p", "scope-good"],
    );
    assert_output_contains_all(
        &recovery,
        &[
            "Test Plan",
            "plan-recovery",
            "combines an explicit workspace selection",
            "Keep one",
        ],
    );

    let err = run_builtin_err(
        root.to_path_buf(),
        "test",
        &["--workspace", "-p", "scope-good"],
    );
    assert_invocation_error_contains(
        err,
        &[
            "combines an explicit workspace selection",
            "-p",
            "--workspace",
        ],
    );

    let err_all_alias = run_builtin_err(root.to_path_buf(), "test", &["--all", "-p", "scope-good"]);
    assert_invocation_error_contains(err_all_alias, &["combines an explicit workspace selection"]);

    let err_reordered = run_builtin_err(
        root.to_path_buf(),
        "test",
        &["-p", "scope-good", "--workspace"],
    );
    assert_invocation_error_contains(err_reordered, &["combines an explicit workspace selection"]);
}

#[test]
fn run_manifest_task_builtin_test_package_scope_execution_omits_unrelated_failing_package() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-package-scope-execution");
    write_two_package_cargo_workspace(&root);

    let out = run_builtin_ok(root.to_path_buf(), "test", &["-p", "scope-good"]);
    assert_output_contains_all(&out, &["Test Results", "targets:", "root"]);

    let out_equals = run_builtin_ok(root.to_path_buf(), "test", &["--package=scope-good"]);
    assert_output_contains_all(&out_equals, &["Test Results"]);

    let out_excluded = run_builtin_ok(
        root.to_path_buf(),
        "test",
        &["--workspace", "--exclude", "scope-bad"],
    );
    assert_output_contains_all(&out_excluded, &["Test Results"]);

    let err = run_builtin_err(root.to_path_buf(), "test", &["--workspace"]);
    assert_builtin_test_non_zero(err, None, &["Test Results", "root"], &[]);

    let err_repeated = run_builtin_err(
        root.to_path_buf(),
        "test",
        &["-p", "scope-good", "-p", "scope-bad"],
    );
    assert_builtin_test_non_zero(err_repeated, None, &["Test Results", "root"], &[]);

    let err_bad = run_builtin_err(root.to_path_buf(), "test", &["-p", "scope-bad"]);
    assert_builtin_test_non_zero(err_bad, None, &["Test Results", "root"], &[]);
}

#[test]
fn run_manifest_task_builtin_test_package_scope_execution_cargo_test_fallback() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-package-scope-cargo-fallback");
    write_two_package_cargo_workspace(&root);

    let Some(cargo_path) = locate_cargo() else {
        eprintln!("skipping: no cargo executable found to force the cargo-test fallback");
        return;
    };
    let bin_dir = root.join("fallback-bin");
    fs::create_dir_all(&bin_dir).expect("mkdir fallback bin");
    write_executable(
        &bin_dir.join("cargo"),
        &format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", cargo_path.display()),
    );
    let Some(rustc_path) = locate_rustc(&cargo_path) else {
        eprintln!("skipping: no rustc executable found to force the cargo-test fallback");
        return;
    };
    // System paths keep `sh`, the compiler driver, and the linker reachable
    // while hiding any PATH-installed `cargo-nextest` from detection. RUSTC
    // and RUSTDOC pin the toolchain binaries the hidden PATH no longer
    // carries (cargo runs `rustc -vV` and doctests need `rustdoc`).
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:/usr/bin:/bin", bin_dir.display()))),
        ("RUSTC", Some(rustc_path.display().to_string())),
        (
            "RUSTDOC",
            Some(rustc_path.with_file_name("rustdoc").display().to_string()),
        ),
    ]);

    let command = plan_command_at(
        root.to_path_buf(),
        &["--plan", "--json", "-p", "scope-good"],
    );
    let command = command.as_str().expect("plan command string");
    assert!(
        command.starts_with("cargo test ") && command.contains("'-p' 'scope-good'"),
        "unexpected fallback plan command: {command}"
    );
    assert!(!command.contains("--workspace"));

    let out = run_builtin_ok(root.to_path_buf(), "test", &["-p", "scope-good"]);
    assert_output_contains_all(&out, &["Test Results", "targets:", "root"]);

    let err = run_builtin_err(root.to_path_buf(), "test", &["--workspace"]);
    assert_builtin_test_non_zero(err, None, &["Test Results", "root"], &[]);
}

/// Locate a real cargo executable for the forced cargo-test fallback: the
/// `CARGO` env var when running under cargo, or a `cargo` entry on `PATH`.
fn locate_cargo() -> Option<PathBuf> {
    if let Ok(cargo) = std::env::var("CARGO") {
        let cargo = PathBuf::from(cargo);
        if cargo.is_file() {
            return Some(cargo);
        }
    }
    std::env::var("PATH")
        .ok()?
        .split(':')
        .map(PathBuf::from)
        .find_map(|dir| {
            let candidate = dir.join("cargo");
            candidate.is_file().then_some(candidate)
        })
}

/// Locate a rustc executable for the fallback test: cargo runs `rustc -vV`
/// from `PATH` (or `RUSTC`), and the hidden PATH no longer carries it.
fn locate_rustc(cargo_path: &Path) -> Option<PathBuf> {
    if let Ok(rustc) = std::env::var("RUSTC") {
        let rustc = PathBuf::from(rustc);
        if rustc.is_file() {
            return Some(rustc);
        }
    }
    let sibling = cargo_path.with_file_name("rustc");
    if sibling.is_file() {
        return Some(sibling);
    }
    std::env::var("PATH")
        .ok()?
        .split(':')
        .map(PathBuf::from)
        .find_map(|dir| {
            let candidate = dir.join("rustc");
            candidate.is_file().then_some(candidate)
        })
}
