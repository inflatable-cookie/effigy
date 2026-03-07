use super::support::{
    cache_env, cache_workspace, read_marker, run_task, write_cache_input, write_cached_manifest,
    write_non_cached_manifest,
};
use crate::contract_test_support::lock_test;
use std::fs;

#[test]
fn task_cache_hit_skips_unchanged_rerun() {
    let _guard = lock_test();
    let root = cache_workspace("cache-hit-skip");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    write_cache_input(&root, "alpha\n");

    let _env = cache_env("A");

    run_task(&root, "build");
    let second = run_task(&root, "build");

    assert!(second.contains("cache hit"));
    assert_eq!(read_marker(&marker), "run");
}

#[test]
fn task_cache_invalidates_on_input_change() {
    let _guard = lock_test();
    let root = cache_workspace("cache-input-change");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    write_cache_input(&root, "alpha\n");
    let _env = cache_env("A");

    run_task(&root, "build");
    write_cache_input(&root, "beta\n");
    run_task(&root, "build");

    assert_eq!(read_marker(&marker), "runrun");
}

#[test]
fn task_cache_invalidates_on_selected_env_change() {
    let _guard = lock_test();
    let root = cache_workspace("cache-env-change");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    write_cache_input(&root, "alpha\n");

    let env = cache_env("A");
    run_task(&root, "build");
    drop(env);

    let _env = cache_env("B");
    run_task(&root, "build");

    assert_eq!(read_marker(&marker), "runrun");
}

#[test]
fn task_cache_invalidates_on_command_change() {
    let _guard = lock_test();
    let root = cache_workspace("cache-command-change");
    let marker = root.join("runs.log");
    write_cache_input(&root, "alpha\n");
    let _env = cache_env("A");

    write_cached_manifest(&root, &marker, "printf one");
    run_task(&root, "build");

    write_cached_manifest(&root, &marker, "printf two");
    run_task(&root, "build");

    assert_eq!(read_marker(&marker), "onetwo");
}

#[test]
fn task_cache_invalidates_when_declared_output_is_missing() {
    let _guard = lock_test();
    let root = cache_workspace("cache-missing-output");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    write_cache_input(&root, "alpha\n");
    let _env = cache_env("A");

    run_task(&root, "build");
    fs::remove_file(root.join("out/result.txt")).expect("remove output");
    run_task(&root, "build");

    assert_eq!(read_marker(&marker), "runrun");
}

#[test]
fn non_opt_in_task_always_executes() {
    let _guard = lock_test();
    let root = cache_workspace("cache-non-opt-in");
    let marker = root.join("runs.log");
    write_non_cached_manifest(&root, &marker);

    run_task(&root, "build");
    run_task(&root, "build");

    assert_eq!(read_marker(&marker), "runrun");
}
