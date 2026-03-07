use super::super::prelude::assert_builtin_test_non_zero;
use super::prelude::{
    fs, lock_test, read_file_text, run_builtin_err, run_builtin_ok, temp_workspace,
    write_executable, write_root_manifest, EnvGuard,
};

fn setup_suite_runner(root: &std::path::Path, script: &str) -> EnvGuard {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    write_executable(&bin_dir.join("suite-run"), script);

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    EnvGuard::set_many(&[("PATH", Some(format!("{}:{prior_path}", bin_dir.display())))])
}

fn assert_event_lines(root: &std::path::Path, expected: &[&str]) {
    let rendered = read_file_text(&root.join("events.log"));
    let actual = rendered
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>();
    assert_eq!(actual, expected);
}

#[test]
fn run_manifest_task_builtin_test_runs_setup_and_default_teardown_on_success() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-suite-lifecycle-success");
    write_root_manifest(
        &root,
        r#"[test.suites.integration]
run = "suite-run"
setup = [{ run = "printf 'setup\n' >> events.log" }]
teardown = [{ run = "printf 'teardown\n' >> events.log" }]
"#,
    );
    let _env = setup_suite_runner(&root, "#!/bin/sh\nprintf 'run\n' >> events.log\nexit 0\n");

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert!(out.contains("Test Results"));
    assert_event_lines(&root, &["setup", "run", "teardown"]);
}

#[test]
fn run_manifest_task_builtin_test_runs_always_teardown_after_suite_failure() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-suite-lifecycle-always-teardown");
    write_root_manifest(
        &root,
        r#"[test.suites.integration]
run = "suite-run"
setup = [{ run = "printf 'setup\n' >> events.log" }]
teardown = [{ run = "printf 'teardown\n' >> events.log" }]
teardown_policy = "always"
"#,
    );
    let _env = setup_suite_runner(&root, "#!/bin/sh\nprintf 'run\n' >> events.log\nexit 1\n");

    let err = run_builtin_err(root.clone(), "test", &[]);
    assert_builtin_test_non_zero(
        err,
        Some(vec![("root".to_owned(), Some(1))]),
        &["Test Results", "root", "exit=1"],
        &[],
    );
    assert_event_lines(&root, &["setup", "run", "teardown"]);
}

#[test]
fn run_manifest_task_builtin_test_skips_default_teardown_after_suite_failure() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-suite-lifecycle-on-success-skip");
    write_root_manifest(
        &root,
        r#"[test.suites.integration]
run = "suite-run"
setup = [{ run = "printf 'setup\n' >> events.log" }]
teardown = [{ run = "printf 'teardown\n' >> events.log" }]
"#,
    );
    let _env = setup_suite_runner(&root, "#!/bin/sh\nprintf 'run\n' >> events.log\nexit 1\n");

    let err = run_builtin_err(root.clone(), "test", &[]);
    assert_builtin_test_non_zero(
        err,
        Some(vec![("root".to_owned(), Some(1))]),
        &["Test Results", "root", "exit=1"],
        &[],
    );
    assert_event_lines(&root, &["setup", "run"]);
}

#[test]
fn run_manifest_task_builtin_test_runs_always_teardown_after_setup_failure_and_skips_suite() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-suite-lifecycle-setup-failure");
    write_root_manifest(
        &root,
        r#"[test.suites.integration]
run = "suite-run"
setup = [{ run = "printf 'setup\n' >> events.log && exit 2" }]
teardown = [{ run = "printf 'teardown\n' >> events.log" }]
teardown_policy = "always"
"#,
    );
    let _env = setup_suite_runner(&root, "#!/bin/sh\nprintf 'run\n' >> events.log\nexit 0\n");

    let err = run_builtin_err(root.clone(), "test", &[]);
    assert_builtin_test_non_zero(
        err,
        Some(vec![("root".to_owned(), Some(2))]),
        &["Test Results", "root", "exit=2"],
        &[],
    );
    assert_event_lines(&root, &["setup", "teardown"]);
}

#[test]
fn run_manifest_task_builtin_test_preserves_passthrough_args_for_lifecycle_enabled_suite() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-suite-lifecycle-passthrough");
    write_root_manifest(
        &root,
        r#"[test.suites.managed]
run = "suite-run"
env = { TEST_DATABASE_URL = "db://fixture" }
setup = [{ run = "printf 'setup\n' >> events.log" }]
teardown = [{ run = "printf 'teardown\n' >> events.log" }]
"#,
    );
    let _env = setup_suite_runner(
        &root,
        "#!/bin/sh\nprintf '%s\n' \"$*\" > args.log\nprintf 'run\n' >> events.log\nexit 0\n",
    );

    let out = run_builtin_ok(
        root.clone(),
        "test",
        &["managed", "--exact", "module_admin"],
    );
    assert!(out.contains("Test Results"));
    assert_eq!(
        read_file_text(&root.join("args.log")).trim(),
        "--exact module_admin"
    );
    assert_event_lines(&root, &["setup", "run", "teardown"]);
}
