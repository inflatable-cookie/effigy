pub(super) use super::super::prelude::*;

pub(super) fn setup_path_with_probes(
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

pub(super) fn assert_cargo_env_matches(marker: &Path, expected_home: &str, expected_target: &str) {
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

pub(super) fn assert_cargo_env_absent(marker: &Path) {
    let rendered = read_file_text(marker);
    assert_eq!(rendered, "|");
}
