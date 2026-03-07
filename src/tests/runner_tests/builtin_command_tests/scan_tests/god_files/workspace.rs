use super::*;

#[test]
fn run_manifest_task_builtin_scan_god_files_ignores_parent_gitignore_above_scan_root() {
    let root = temp_workspace("builtin-scan-god-files-parent-ignore");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_large_rust_file(&farmyard.join("src/lib.rs"), 12);

    let out = run_builtin_ok(
        farmyard,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "src/lib.rs", "12 code lines"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_root_fans_out_across_child_catalogs() {
    let root = temp_workspace("builtin-scan-god-files-root-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_large_rust_file(&farmyard.join("src/lib.rs"), 12);

    let out = run_builtin_ok(
        root,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(
        &out,
        &["findings: 1", "farmyard/src/lib.rs", "12 code lines"],
    );
}
