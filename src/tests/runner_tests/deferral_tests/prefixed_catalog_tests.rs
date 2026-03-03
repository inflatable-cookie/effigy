use super::prelude::*;

#[test]
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let _guard = lock_test();
    let root = temp_workspace("defer-prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_defer_manifest(&root, "false");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[defer]\nrun = \"printf farmyard-deferred\"\n",
    );

    let out =
        run_task_in_workspace(&root, "farmyard/missing", &[]).expect("prefixed deferral should succeed");

    assert_eq!(out, "");
}
