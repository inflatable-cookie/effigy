use super::prelude::*;

#[test]
fn parse_task_selector_supports_prefixed_task() {
    let selector = parse_task_selector("farmyard/reset-db").expect("selector");
    assert_eq!(selector.prefix, Some("farmyard".to_owned()));
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

    let err = run_task(&root, "farmyard/reset-db", &[]).expect_err("unknown prefix");
    assert_catalog_prefix_not_found(err, "farmyard", &["root"]);
}

#[test]
fn run_manifest_task_verbose_root_includes_resolution_trace() {
    let _guard = lock_test();
    let _env = EnvGuard::set_many(&[("EFFIGY_COLOR", None), ("NO_COLOR", None)]);
    let root = temp_workspace("verbose-trace");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_root_manifest(&root, "[tasks.ping]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf farmyard\"\n",
    );

    let out = run_task(&root, "farmyard/ping", &["--verbose-root"]).expect("run");

    assert!(out.contains("Task Resolution"));
    assert!(out.contains("catalog-alias: farmyard"));
    assert!(out.contains("farmyard"));
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
