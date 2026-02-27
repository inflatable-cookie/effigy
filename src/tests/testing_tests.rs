use super::{detect_test_runner, detect_test_runner_detailed, TestRunner};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn detect_test_runner_prefers_vitest_when_package_json_references_it() {
    let root = temp_workspace("test-detect-vitest-pkg");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let plan = detect_test_runner(&root).expect("plan");
    assert_eq!(plan.runner, TestRunner::Vitest);
    assert_eq!(plan.command, "vitest");
    assert!(plan
        .evidence
        .iter()
        .any(|line| line.contains("package.json")));
}

#[test]
fn detect_test_runner_detects_vitest_from_config_file() {
    let root = temp_workspace("test-detect-vitest-config");
    fs::write(root.join("vitest.config.ts"), "export default {};\n").expect("write config");

    let plan = detect_test_runner(&root).expect("plan");
    assert_eq!(plan.runner, TestRunner::Vitest);
    assert_eq!(plan.command, "vitest");
    assert!(plan
        .evidence
        .iter()
        .any(|line| line.contains("vitest.config.ts")));
}

#[test]
fn detect_test_runner_uses_nextest_when_available() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("test-detect-nextest");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let nextest = bin_dir.join("cargo-nextest");
    fs::write(&nextest, "#!/bin/sh\nexit 0\n").expect("write nextest");
    let mut perms = fs::metadata(&nextest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&nextest, perms).expect("chmod");

    let old_path = std::env::var("PATH").ok().unwrap_or_default();
    let _env = EnvGuard::set_many(&[("PATH", Some(format!("{}:{old_path}", bin_dir.display())))]);

    let plan = detect_test_runner(&root).expect("plan");
    assert_eq!(plan.runner, TestRunner::CargoNextest);
    assert_eq!(plan.command, "cargo nextest run");
}

#[test]
fn detect_test_runner_falls_back_to_cargo_test_when_nextest_missing() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("test-detect-cargo-fallback");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
    let empty_bin = root.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("mkdir empty");
    let _env = EnvGuard::set_many(&[("PATH", Some(empty_bin.display().to_string()))]);

    let plan = detect_test_runner(&root).expect("plan");
    assert_eq!(plan.runner, TestRunner::CargoTest);
    assert_eq!(plan.command, "cargo test");
    assert!(plan
        .evidence
        .iter()
        .any(|line| line.contains("falling back to `cargo test`")));
}

#[test]
fn detect_test_runner_prefers_vitest_when_js_and_rust_markers_both_exist() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("test-detect-prefers-vitest");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");

    let empty_bin = root.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("mkdir empty");
    let _env = EnvGuard::set_many(&[("PATH", Some(empty_bin.display().to_string()))]);

    let plan = detect_test_runner(&root).expect("plan");
    assert_eq!(plan.runner, TestRunner::Vitest);
    assert_eq!(plan.command, "vitest");
}

#[test]
fn detect_test_runner_returns_none_without_known_markers() {
    let root = temp_workspace("test-detect-none");
    assert!(detect_test_runner(&root).is_none());
}

#[test]
fn detect_test_runner_detailed_includes_candidate_chain_with_rejections() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("test-detect-detailed-chain");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
    let empty_bin = root.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("mkdir empty");
    let _env = EnvGuard::set_many(&[("PATH", Some(empty_bin.display().to_string()))]);

    let report = detect_test_runner_detailed(&root);
    let selected = report.selected.expect("selected plan");
    assert_eq!(selected.runner, TestRunner::CargoTest);
    assert_eq!(report.candidates.len(), 3);
    assert_eq!(report.candidates[0].runner, TestRunner::Vitest);
    assert!(!report.candidates[0].available);
    assert_eq!(report.candidates[1].runner, TestRunner::CargoNextest);
    assert!(!report.candidates[1].available);
    assert_eq!(report.candidates[2].runner, TestRunner::CargoTest);
    assert!(report.candidates[2].available);
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-testing-{name}-{ts}"))
}

fn temp_workspace(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    original: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn set_many(entries: &[(&str, Option<String>)]) -> Self {
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push(((*key).to_owned(), std::env::var(key).ok()));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.original {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
