use crate::runner::tests::prelude::{
    assert_builtin_ok_empty, assert_output_contains_all, create_workspace_dir,
    create_workspace_path, run_builtin_ok, setup_root_and_catalog_a_ping, temp_workspace,
    write_catalog_tasks, write_root_manifest,
};

#[test]
fn run_manifest_task_prefixed_uses_named_catalog() {
    let (root, _) = setup_root_and_catalog_a_ping("prefixed");
    assert_builtin_ok_empty(root, "catalog_a/ping", &[]);
}

#[test]
fn run_manifest_task_unprefixed_prefers_nearest_catalog_in_scope() {
    let (root, _) = setup_root_and_catalog_a_ping("nearest");
    let nested = create_workspace_path(&root, "catalog_a/crates/api");
    assert_builtin_ok_empty(nested, "ping", &[]);
}

#[test]
fn run_manifest_task_relative_prefix_resolves_catalog_by_path() {
    let root = temp_workspace("relative-prefix-path");
    let catalog_b = create_workspace_dir(&root, "catalog_b");
    let froyo = create_workspace_dir(&root, "froyo");
    write_root_manifest(&root, "");

    write_catalog_tasks(
        &catalog_b,
        Some("catalog_b"),
        &[("dev", "printf catalog_b")],
    );
    write_catalog_tasks(
        &froyo,
        Some("froyo"),
        &[("validate", "printf froyo-validate")],
    );

    assert_builtin_ok_empty(catalog_b, "../froyo/validate", &[]);
}

#[test]
fn run_manifest_task_relative_prefix_prefers_alias_collision_over_path_resolution() {
    let root = temp_workspace("relative-prefix-alias-collision");
    let catalog_b = create_workspace_dir(&root, "catalog_b");
    let alias_override = create_workspace_dir(&root, "alias-override");
    let froyo = create_workspace_dir(&root, "froyo");
    write_root_manifest(&root, "");

    write_catalog_tasks(
        &catalog_b,
        Some("catalog_b"),
        &[("dev", "printf catalog_b")],
    );
    write_catalog_tasks(
        &alias_override,
        Some("../froyo"),
        &[("validate", "printf alias")],
    );
    write_catalog_tasks(&froyo, Some("froyo"), &[("validate", "printf froyo")]);

    let out = run_builtin_ok(catalog_b, "../froyo/validate", &["--verbose-root"]);

    assert_output_contains_all(
        &out,
        &[
            "catalog-alias: ../froyo",
            "selected catalog via explicit prefix `../froyo`",
        ],
    );
}

#[test]
fn run_manifest_task_relative_prefix_supports_multi_parent_traversal() {
    let root = temp_workspace("relative-prefix-multi-parent");
    let app = create_workspace_path(&root, "apps/web/src");
    let shared = create_workspace_path(&root, "shared");
    write_root_manifest(&root, "");

    write_catalog_tasks(&shared, Some("shared"), &[("lint", "printf shared-lint")]);

    let out = run_builtin_ok(app, "../../../shared/lint", &["--verbose-root"]);

    assert_output_contains_all(
        &out,
        &[
            "catalog-alias: shared",
            "relative prefix `../../../shared` -> `shared`",
        ],
    );
}
