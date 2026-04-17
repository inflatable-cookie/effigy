use crate::runner::tests::prelude::{
    fs, lock_test, run_task_expect_empty_output, workspace_with_optional_defer_manifest,
    write_manifest,
};

#[test]
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let _guard = lock_test();
    let root = workspace_with_optional_defer_manifest("defer-prefixed", Some("false"));
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[defer]\nrun = \"printf catalog_a-deferred\"\n",
    );

    run_task_expect_empty_output(
        &root,
        "catalog_a/missing",
        &[],
        "prefixed deferral should succeed",
    );
}
