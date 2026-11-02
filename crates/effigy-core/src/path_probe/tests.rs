use std::fs;
use std::path::PathBuf;

use super::*;

fn make_temp_dir(test_name: &str) -> PathBuf {
    effigy_test_support::Fixture::process_lifetime(&format!("effigy-path-probe-{test_name}-"))
        .expect("fixture directory")
}

#[test]
fn command_available_in_path_with_finds_builtins_and_refuses_missing_or_unset_path() {
    let temp = make_temp_dir("find-binary");
    let bin = temp.join("mytool");
    fs::write(&bin, "#!/bin/sh\nexit 0\n").expect("write test binary");
    let raw = std::env::join_paths([temp.as_path()]).expect("join path");

    assert!(
        command_available_in_path_with("mytool", Some(raw.clone())),
        "a binary on the given path list is available"
    );
    assert!(
        !command_available_in_path_with("missing-tool", Some(raw)),
        "a missing binary is unavailable"
    );
    assert!(
        !command_available_in_path_with("cargo", None),
        "no PATH env means unavailable"
    );

    let _ = fs::remove_dir_all(temp);
}
