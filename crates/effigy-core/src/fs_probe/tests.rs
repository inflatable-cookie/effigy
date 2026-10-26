use std::fs;
use std::path::PathBuf;

use super::*;

fn make_temp_dir(test_name: &str) -> PathBuf {    effigy_test_support::Fixture::process_lifetime(&format!("effigy-fs-probe-{test_name}-"))
        .expect("fixture directory")
}

#[test]
fn child_exists_and_is_file_checks_are_stable() {
    let root = make_temp_dir("exists-file");
    let file_path = root.join("package.json");
    let dir_path = root.join(".git");
    fs::write(&file_path, "{}\n").expect("write package json");
    fs::create_dir_all(&dir_path).expect("create .git dir");

    let mut probe = PathPresenceCache::new();
    assert!(probe.child_exists(&root, "package.json"));
    assert!(probe.child_is_file(&root, "package.json"));
    assert!(probe.child_exists(&root, ".git"));
    assert!(!probe.child_is_file(&root, ".git"));
    assert!(!probe.child_exists(&root, "Cargo.toml"));

    let _ = fs::remove_dir_all(root);
}
