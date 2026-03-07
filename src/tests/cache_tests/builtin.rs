use super::support::{
    assert_cache_task_invocation, cache_env, cache_workspace, run_cache_builtin, run_task,
    write_cache_input, write_cached_manifest,
};
use crate::contract_test_support::{lock_test, write_manifest};

#[test]
fn cache_builtin_inspect_and_invalidate_paths_are_available() {
    let _guard = lock_test();
    let root = cache_workspace("cache-builtin-paths");
    let marker = root.join("runs.log");
    write_cached_manifest(&root, &marker, "printf run");
    write_cache_input(&root, "alpha\n");
    let _env = cache_env("A");

    run_task(&root, "build");

    let inspect_present = run_cache_builtin(&root, &["inspect", "build"])
        .expect("inspect should succeed");
    assert!(inspect_present.contains("status: present"));

    let invalidate = run_cache_builtin(&root, &["invalidate", "build"])
        .expect("invalidate should succeed");
    assert!(invalidate.contains("removed: 1"));

    let inspect_missing = run_cache_builtin(&root, &["inspect", "build"])
        .expect("inspect should succeed");
    assert!(inspect_missing.contains("status: missing"));
}

#[test]
fn cache_builtin_requires_subcommand() {
    let _guard = lock_test();
    let root = cache_workspace("cache-builtin-requires-subcommand");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_cache_builtin(&root, &[]).expect_err("cache should require a subcommand");
    assert_cache_task_invocation(
        err,
        "`cache` requires a subcommand: `inspect` or `invalidate`",
    );
}

#[test]
fn cache_builtin_rejects_unknown_subcommand() {
    let _guard = lock_test();
    let root = cache_workspace("cache-builtin-unknown-subcommand");
    write_manifest(&root.join("effigy.toml"), "");

    let err =
        run_cache_builtin(&root, &["drop"]).expect_err("cache should reject unknown subcommand");
    assert_cache_task_invocation(
        err,
        "unknown cache subcommand `drop` (expected `inspect` or `invalidate`)",
    );
}

#[test]
fn cache_builtin_inspect_rejects_invalid_flags() {
    let _guard = lock_test();
    let root = cache_workspace("cache-builtin-inspect-invalid-flags");
    write_manifest(&root.join("effigy.toml"), "");

    let unknown_flag =
        run_cache_builtin(&root, &["inspect", "--wat"]).expect_err("inspect should reject --wat");
    assert_cache_task_invocation(
        unknown_flag,
        "unknown argument(s) for built-in `cache`: --wat",
    );

    let all_flag =
        run_cache_builtin(&root, &["inspect", "--all"]).expect_err("inspect should reject --all");
    assert_cache_task_invocation(
        all_flag,
        "`cache inspect` does not support `--all`; use `cache invalidate --all`",
    );

    let too_many_selectors = run_cache_builtin(&root, &["inspect", "build", "test"])
        .expect_err("inspect should reject multiple selectors");
    assert_cache_task_invocation(
        too_many_selectors,
        "`cache inspect` accepts at most one selector",
    );
}

#[test]
fn cache_builtin_invalidate_rejects_invalid_selector_combinations_and_flags() {
    let _guard = lock_test();
    let root = cache_workspace("cache-builtin-invalidate-invalid-combinations");
    write_manifest(&root.join("effigy.toml"), "");

    let missing_selector =
        run_cache_builtin(&root, &["invalidate"]).expect_err("invalidate should require selector");
    assert_cache_task_invocation(
        missing_selector,
        "`cache invalidate` requires one or more selectors (or `--all`)",
    );

    let conflicting =
        run_cache_builtin(&root, &["invalidate", "--all", "build"]).expect_err("invalid combo");
    assert_cache_task_invocation(
        conflicting,
        "`cache invalidate` accepts either `--all` or selectors, not both",
    );

    let unknown_flag = run_cache_builtin(&root, &["invalidate", "--wat"])
        .expect_err("invalidate should reject unknown flags");
    assert_cache_task_invocation(
        unknown_flag,
        "unknown argument(s) for built-in `cache`: --wat",
    );
}
