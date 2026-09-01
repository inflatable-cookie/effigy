//! Prove `effigy service list` text and JSON advertise only callable bundled
//! fragments — first-level directories that carry `service.toml`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

fn bare_repo(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    write(&repo.join("effigy.toml"), "[catalog]\nalias = \"demo\"\n");
    repo
}

struct Run {
    stdout: String,
    stderr: String,
    success: bool,
}

fn effigy(home: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .env("HOME", home)
        .env("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run effigy");
    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    }
}

const EXPECTED_BUNDLED: &[&str] = &[
    "dbgate",
    "elasticsearch",
    "mailpit",
    "mariadb",
    "memcached",
    "minio",
    "nginx",
    "node",
    "pgweb",
    "php-fpm",
    "phpmyadmin",
    "postgres",
    "redis",
    "workspace-rust-bun",
];

#[test]
fn service_list_text_exposes_only_bundled_service_manifests() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let repo = bare_repo(temp.path());
    let repo_arg = repo.to_str().expect("utf-8 path");

    let run = effigy(&home, &["service", "list", "--repo", repo_arg]);
    assert!(run.success, "service list failed: {}", run.stderr);
    assert!(
        run.stdout
            .contains(&format!("[service] {} fragments", EXPECTED_BUNDLED.len())),
        "count should match service.toml parents:\n{}",
        run.stdout
    );
    for name in EXPECTED_BUNDLED {
        assert!(
            run.stdout.contains(&format!("{name} [bundled]")),
            "missing {name}:\n{}",
            run.stdout
        );
    }
    assert!(
        !run.stdout.contains("README.md") && !run.stdout.contains("compose.override.example.yml"),
        "root catalog assets leaked into text output:\n{}",
        run.stdout
    );
}

#[test]
fn service_list_json_exposes_only_bundled_service_manifests() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let repo = bare_repo(temp.path());
    let repo_arg = repo.to_str().expect("utf-8 path");

    let run = effigy(&home, &["--json", "service", "list", "--repo", repo_arg]);
    assert!(run.success, "json service list failed: {}", run.stderr);
    let envelope: serde_json::Value =
        serde_json::from_str(run.stdout.trim()).expect("stdout is one envelope");
    assert_eq!(envelope["schema"], "effigy.command.v1");
    assert_eq!(envelope["result"]["schema"], "effigy.service.list.v1");
    let fragments = envelope["result"]["fragments"]
        .as_array()
        .expect("fragments array");
    let names: Vec<&str> = fragments
        .iter()
        .map(|fragment| fragment["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, EXPECTED_BUNDLED);
    assert!(fragments
        .iter()
        .all(|fragment| fragment["source"] == "bundled"));
    assert!(
        !names.contains(&"README.md") && !names.contains(&"compose.override.example.yml"),
        "root catalog assets leaked into JSON output: {names:?}"
    );
}
