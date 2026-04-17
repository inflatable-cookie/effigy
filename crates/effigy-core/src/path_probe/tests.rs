use std::fs;
use std::path::PathBuf;

use super::*;

fn make_temp_dir(test_name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    dir.push(format!("effigy-path-probe-{test_name}-{now}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn command_available_in_path_with_finds_binary_in_given_path_list() {
    let temp = make_temp_dir("find-binary");
    let bin = temp.join("mytool");
    fs::write(&bin, "#!/bin/sh\nexit 0\n").expect("write test binary");
    let raw = std::env::join_paths([temp.as_path()]).expect("join path");

    assert!(command_available_in_path_with("mytool", Some(raw)));

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn command_available_in_path_with_returns_false_for_missing_binary() {
    let temp = make_temp_dir("missing-binary");
    let raw = std::env::join_paths([temp.as_path()]).expect("join path");

    assert!(!command_available_in_path_with("missing-tool", Some(raw)));

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn command_available_in_path_with_returns_false_without_path_env() {
    assert!(!command_available_in_path_with("cargo", None));
}
