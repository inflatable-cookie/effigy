use super::prelude::{
    assert_catalog_prefix_not_found, assert_output_contains_all, assert_run_task_ok_empty, fs,
    lock_test, parse_task_selector, run_task, temp_workspace, write_executable, write_manifest,
    write_root_manifest, EnvGuard,
};

#[test]
fn parse_task_selector_supports_prefixed_task() {
    let selector = parse_task_selector("catalog_a/reset-db").expect("selector");
    assert_eq!(selector.prefix, Some("catalog_a".to_owned()));
    assert_eq!(selector.task_name, "reset-db");
}

#[test]
fn parse_task_selector_supports_relative_prefixed_task() {
    let selector = parse_task_selector("../froyo/validate").expect("selector");
    assert_eq!(selector.prefix, Some("../froyo".to_owned()));
    assert_eq!(selector.task_name, "validate");
}

#[test]
fn run_manifest_task_unknown_prefix_returns_catalog_error() {
    let root = temp_workspace("unknown-prefix");
    write_root_manifest(&root, "[tasks.reset-db]\nrun = \"printf root\"\n");

    let err = run_task(&root, "catalog_a/reset-db", &[]).expect_err("unknown prefix");
    assert_catalog_prefix_not_found(err, "catalog_a", &["root"]);
}

#[test]
fn run_manifest_task_verbose_root_includes_resolution_trace() {
    let _guard = lock_test();
    let _env = EnvGuard::set_many(&[("EFFIGY_COLOR", None), ("NO_COLOR", None)]);
    let root = temp_workspace("verbose-trace");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir");
    write_root_manifest(&root, "[tasks.ping]\nrun = \"printf root\"\n");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf catalog_a\"\n",
    );

    let out = run_task(&root, "catalog_a/ping", &["--verbose-root"]).expect("run");
    assert_output_contains_all(
        &out,
        &["Task Resolution", "catalog-alias: catalog_a", "catalog_a"],
    );
}

#[test]
fn run_manifest_task_includes_local_node_modules_bin_in_path() {
    let root = temp_workspace("local-node-bin-path");
    write_root_manifest(&root, "[tasks.local]\nrun = \"local-tool\"\n");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    write_executable(&local_bin.join("local-tool"), "#!/bin/sh\nexit 0\n");

    assert_run_task_ok_empty(&root, "local", &[]);
}
