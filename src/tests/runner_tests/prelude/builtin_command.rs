use super::harness::{
    run_builtin_ok, write_executable, write_manifest, write_package_json_with_test_script,
    write_root_manifest, EnvGuard,
};
use super::output::{assert_output_contains_all, read_file_text};
use super::runtime::{fs, Path, PathBuf, RunnerError};

pub(in crate::runner::tests) fn setup_fanout_catalog_repo(root: &Path) -> (PathBuf, PathBuf) {
    let catalog_a = root.join("catalog_a");
    let catalog_b = root.join("catalog_b");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    fs::create_dir_all(&catalog_b).expect("mkdir catalog_b");
    write_root_manifest(
        root,
        "[catalog.members]\ncatalog_a = \"catalog_a\"\ncatalog_b = \"catalog_b\"\n\n[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &catalog_b.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_b\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_package_json_with_test_script(&catalog_a);
    write_package_json_with_test_script(&catalog_b);
    (catalog_a, catalog_b)
}

pub(in crate::runner::tests) fn assert_builtin_test_non_zero(
    err: RunnerError,
    expected_failures: Option<Vec<(String, Option<i32>)>>,
    expected_rendered_snippets: &[&str],
    unexpected_rendered_snippets: &[&str],
) {
    match err {
        RunnerError::BuiltinTestNonZero { failures, rendered } => {
            if let Some(expected) = expected_failures {
                assert_eq!(failures, expected);
            }
            assert_output_contains_all(&rendered, expected_rendered_snippets);
            super::output::assert_output_excludes_all(&rendered, unexpected_rendered_snippets);
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn setup_path_with_probes(
    root: &Path,
    probe_scripts: &[(&str, &str)],
    marker: &Path,
    clear_cargo_env: bool,
) -> EnvGuard {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    for (name, script) in probe_scripts {
        write_executable(&bin_dir.join(name), script);
    }

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);

    let mut env_entries = vec![
        ("PATH", Some(path)),
        (
            "EFFIGY_TEST_CARGO_ENV_FILE",
            Some(marker.display().to_string()),
        ),
    ];
    if clear_cargo_env {
        env_entries.push(("CARGO_HOME", None));
        env_entries.push(("CARGO_TARGET_DIR", None));
    }
    EnvGuard::set_many(&env_entries)
}

pub(in crate::runner::tests) fn assert_cargo_env_matches(
    marker: &Path,
    expected_home: &str,
    expected_target: &str,
) {
    let rendered = read_file_text(marker);
    let parts = rendered.split('|').collect::<Vec<&str>>();
    assert_eq!(
        parts.len(),
        2,
        "expected cargo env marker format `home|target`"
    );
    assert!(parts[0].ends_with(expected_home));
    assert!(parts[1].ends_with(expected_target));
}

pub(in crate::runner::tests) fn assert_cargo_env_absent(marker: &Path) {
    let rendered = read_file_text(marker);
    assert_eq!(rendered, "|");
}

pub(in crate::runner::tests) const CARGO_ENV_PROBE_SCRIPT: &str =
    "#!/bin/sh\nprintf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"$EFFIGY_TEST_CARGO_ENV_FILE\"\n";

pub(in crate::runner::tests) fn assert_cargo_env_applied(
    root: &Path,
    expected_home: &str,
    expected_target: &str,
) {
    let out = run_builtin_ok(root.to_path_buf(), "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_cargo_env_matches(&root.join("cargo-env.log"), expected_home, expected_target);
}
