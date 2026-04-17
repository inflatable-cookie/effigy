use crate::contract_test_support::{lock_test, temp_workspace, EnvGuard};
use effigy_tasks::testing::{detect_test_runner, detect_test_runner_detailed, TestRunner};
use std::fs;
use std::os::unix::fs::PermissionsExt;

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
    assert_eq!(plan.command, "vitest run");
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
    assert_eq!(plan.command, "vitest run");
    assert!(plan
        .evidence
        .iter()
        .any(|line| line.contains("vitest.config.ts")));
}

#[test]
fn detect_test_runner_uses_nextest_when_available() {
    let _guard = lock_test();
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
    let _guard = lock_test();
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
    let _guard = lock_test();
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
    assert_eq!(plan.command, "vitest run");
}

#[test]
fn detect_test_runner_returns_none_without_known_markers() {
    let root = temp_workspace("test-detect-none");
    assert!(detect_test_runner(&root).is_none());
}

#[test]
fn detect_test_runner_detailed_includes_candidate_chain_with_rejections() {
    let _guard = lock_test();
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
