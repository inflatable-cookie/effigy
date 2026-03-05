use super::prelude::*;

#[test]
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let _guard = lock_test();
    let root = workspace_with_optional_defer_manifest("defer-prefixed", Some("false"));
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[defer]\nrun = \"printf farmyard-deferred\"\n",
    );

    run_task_expect_empty_output(
        &root,
        "farmyard/missing",
        &[],
        "prefixed deferral should succeed",
    );
}
