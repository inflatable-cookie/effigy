use crate::runner::tests::prelude::{
    assert_builtin_ok_empty, assert_catalog_alias_conflict, create_workspace_dir,
    load_effective_catalogs, symlink, temp_workspace, write_catalog_tasks, write_manifest,
};

#[test]
fn effective_catalogs_include_symlinked_member_directories() {
    let root = temp_workspace("catalog-symlink-discovery");
    let external = create_workspace_dir(&root, "external");
    let platform_src = create_workspace_dir(&external, "platform");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "acowtancy"

[catalog.members]
platform = "platform"
"#,
    );
    write_catalog_tasks(
        &platform_src,
        Some("platform"),
        &[("ping", "printf platform")],
    );
    symlink(&platform_src, root.join("platform")).expect("symlink platform");

    let catalogs = load_effective_catalogs(&root).expect("load catalogs");
    assert!(
        catalogs.iter().any(|catalog| catalog.alias == "platform"),
        "symlinked platform catalog should be discovered"
    );

    assert_builtin_ok_empty(root, "platform/ping", &[]);
}

#[cfg(unix)]
#[test]
fn effective_catalogs_report_alias_conflict_for_symlinked_catalog() {
    let root = temp_workspace("catalog-symlink-alias-conflict");
    let catalog_b = create_workspace_dir(&root, "catalog_b");
    let external = create_workspace_dir(&root, "external");
    let platform_src = create_workspace_dir(&external, "platform");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog.members]
catalog_b = "catalog_b"
platform = "platform"
"#,
    );

    write_manifest(
        &catalog_b.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_b"
"#,
    );
    write_manifest(
        &platform_src.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_b"
"#,
    );
    symlink(&platform_src, root.join("platform")).expect("symlink platform");

    let err = load_effective_catalogs(&root).expect_err("expected alias conflict");
    assert_catalog_alias_conflict(err.into(), "catalog_b");
}

#[test]
fn effective_catalogs_require_root_manifest_anchor() {
    let root = temp_workspace("catalog-discovery-root-anchor");
    let catalog_a = create_workspace_dir(&root, "catalog_a");

    write_manifest(
        &catalog_a.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_a"
"#,
    );

    let err = load_effective_catalogs(&root).expect_err("expected missing root manifest");
    match err {
        effigy_routing::RoutingError::TaskCatalogsMissing { root: missing_root } => {
            assert_eq!(missing_root, root);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn effective_catalogs_include_named_member_mount_directories() {
    let root = temp_workspace("catalog-system-mount-discovery");
    let external = create_workspace_dir(&root, "external");
    let platform_src = create_workspace_dir(&external, "platform");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "acowtancy"

[catalog.members]
platform = "external/platform"

[systems.dev]
mounts = [{ member = "platform" }]
"#,
    );
    write_manifest(
        &platform_src.join("effigy.toml"),
        r#"[catalog]
alias = "platform"

[tasks."check:exports"]
run = "printf platform-checks"
"#,
    );

    let catalogs = load_effective_catalogs(&root).expect("load catalogs");
    assert!(
        catalogs.iter().any(|catalog| catalog.alias == "platform"),
        "mounted platform catalog should be discovered"
    );

    assert_builtin_ok_empty(root, "platform/check:exports", &[]);
}

#[test]
fn effective_catalogs_include_inline_workspace_mount_directories() {
    let root = temp_workspace("catalog-workspace-mount-discovery");
    let external = create_workspace_dir(&root, "external");
    let platform_src = create_workspace_dir(&external, "platform");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "acowtancy"

[systems.dev.workspaces.app]
mounts = [{ source = "external/platform", target = "/workspace-root/platform", catalog = true }]
"#,
    );
    write_catalog_tasks(
        &platform_src,
        Some("platform"),
        &[("validate", "printf platform-validate")],
    );

    let catalogs = load_effective_catalogs(&root).expect("load catalogs");
    assert!(
        catalogs.iter().any(|catalog| catalog.alias == "platform"),
        "workspace-mounted platform catalog should be discovered"
    );

    assert_builtin_ok_empty(root, "platform/validate", &[]);
}

#[test]
fn effective_catalogs_ignore_undeclared_runtime_artifact_directories() {
    let root = temp_workspace("catalog-discovery-skips-runtime-artifacts");
    let runtime_catalog = create_workspace_dir(&root, ".effigy/runtime/fake-catalog");

    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &runtime_catalog.join("effigy.toml"),
        r#"[catalog]
alias = "fake-runtime"
"#,
    );

    let catalogs = load_effective_catalogs(&root).expect("load catalogs");
    assert!(
        catalogs
            .iter()
            .all(|catalog| catalog.alias != "fake-runtime"),
        "runtime artifact dirs under .effigy should not be discovered as catalogs"
    );
}

#[test]
fn effective_catalogs_ignore_undeclared_dependency_and_build_directories() {
    let root = temp_workspace("catalog-discovery-skips-dependency-build-dirs");
    let node_catalog = create_workspace_dir(&root, "node_modules/fake-package");
    let vendor_catalog = create_workspace_dir(&root, "vendor/fake-package");
    let target_catalog = create_workspace_dir(&root, "target/fake-crate");

    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &node_catalog.join("effigy.toml"),
        r#"[catalog]
alias = "fake-node"
"#,
    );
    write_manifest(
        &vendor_catalog.join("effigy.toml"),
        r#"[catalog]
alias = "fake-vendor"
"#,
    );
    write_manifest(
        &target_catalog.join("effigy.toml"),
        r#"[catalog]
alias = "fake-target"
"#,
    );

    let catalogs = load_effective_catalogs(&root).expect("load catalogs");
    assert!(
        catalogs.iter().all(|catalog| {
            !matches!(
                catalog.alias.as_str(),
                "fake-node" | "fake-vendor" | "fake-target"
            )
        }),
        "dependency/build dirs should not be discovered as catalogs"
    );
}
