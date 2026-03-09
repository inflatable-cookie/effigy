use super::*;

#[test]
fn run_manifest_task_builtin_scan_god_files_ignores_parent_gitignore_above_scan_root() {
    let root = temp_workspace("builtin-scan-god-files-parent-ignore");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("src")).expect("mkdir catalog_a src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    write_large_rust_file(&catalog_a.join("src/lib.rs"), 12);

    let out = run_builtin_ok(
        catalog_a,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "src/lib.rs", "12 code lines"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_root_fans_out_across_child_catalogs() {
    let root = temp_workspace("builtin-scan-god-files-root-fanout");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(catalog_a.join("src")).expect("mkdir catalog_a src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n",
    );
    write_large_rust_file(&catalog_a.join("src/lib.rs"), 12);

    let out = run_builtin_ok(
        root,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(
        &out,
        &["findings: 1", "catalog_a/src/lib.rs", "12 code lines"],
    );
}
