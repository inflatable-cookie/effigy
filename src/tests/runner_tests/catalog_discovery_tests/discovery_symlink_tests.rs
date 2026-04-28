use crate::runner::tests::prelude::{
    assert_builtin_ok_empty, assert_catalog_alias_conflict, create_workspace_dir,
    discover_catalogs, symlink, temp_workspace, write_catalog_tasks, write_manifest,
};

#[test]
fn discover_catalogs_includes_symlinked_catalog_directories() {
    let root = temp_workspace("catalog-symlink-discovery");
    let external = create_workspace_dir(&root, "external");
    let underlay_src = create_workspace_dir(&external, "underlay");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "acowtancy"
"#,
    );
    write_catalog_tasks(
        &underlay_src,
        Some("underlay"),
        &[("ping", "printf underlay")],
    );
    symlink(&underlay_src, root.join("underlay")).expect("symlink underlay");

    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert!(
        catalogs.iter().any(|catalog| catalog.alias == "underlay"),
        "symlinked underlay catalog should be discovered"
    );

    assert_builtin_ok_empty(root, "underlay/ping", &[]);
}

#[cfg(unix)]
#[test]
fn discover_catalogs_reports_alias_conflict_for_symlinked_catalog() {
    let root = temp_workspace("catalog-symlink-alias-conflict");
    let catalog_b = create_workspace_dir(&root, "catalog_b");
    let external = create_workspace_dir(&root, "external");
    let underlay_src = create_workspace_dir(&external, "underlay");

    write_manifest(&root.join("effigy.toml"), "");

    write_manifest(
        &catalog_b.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_b"
"#,
    );
    write_manifest(
        &underlay_src.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_b"
"#,
    );
    symlink(&underlay_src, root.join("underlay")).expect("symlink underlay");

    let err = discover_catalogs(&root).expect_err("expected alias conflict");
    assert_catalog_alias_conflict(err.into(), "catalog_b");
}

#[test]
fn discover_catalogs_requires_root_manifest_anchor_before_scanning_children() {
    let root = temp_workspace("catalog-discovery-root-anchor");
    let catalog_a = create_workspace_dir(&root, "catalog_a");

    write_manifest(
        &catalog_a.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_a"
"#,
    );

    let err = discover_catalogs(&root).expect_err("expected missing root manifest");
    match err {
        effigy_routing::RoutingError::TaskCatalogsMissing { root: missing_root } => {
            assert_eq!(missing_root, root);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn discover_catalogs_includes_system_mount_catalog_directories() {
    let root = temp_workspace("catalog-system-mount-discovery");
    let external = create_workspace_dir(&root, "external");
    let underlay_src = create_workspace_dir(&external, "underlay");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "acowtancy"

[systems.dev]
mounts = ["./external/underlay"]
"#,
    );
    write_manifest(
        &underlay_src.join("effigy.toml"),
        r#"[catalog]
alias = "underlay"

[tasks."check:exports"]
run = "printf underlay-checks"
"#,
    );

    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert!(
        catalogs.iter().any(|catalog| catalog.alias == "underlay"),
        "mounted underlay catalog should be discovered"
    );

    assert_builtin_ok_empty(root, "underlay/check:exports", &[]);
}

#[test]
fn discover_catalogs_includes_workspace_mount_catalog_directories() {
    let root = temp_workspace("catalog-workspace-mount-discovery");
    let external = create_workspace_dir(&root, "external");
    let underlay_src = create_workspace_dir(&external, "underlay");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "acowtancy"

[systems.dev.workspaces.app]
mounts = ["./external/underlay:/workspace-root/underlay"]
"#,
    );
    write_catalog_tasks(
        &underlay_src,
        Some("underlay"),
        &[("validate", "printf underlay-validate")],
    );

    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert!(
        catalogs.iter().any(|catalog| catalog.alias == "underlay"),
        "workspace-mounted underlay catalog should be discovered"
    );

    assert_builtin_ok_empty(root, "underlay/validate", &[]);
}

#[test]
fn discover_catalogs_skips_runtime_artifact_directories() {
    let root = temp_workspace("catalog-discovery-skips-runtime-artifacts");
    let runtime_catalog = create_workspace_dir(&root, ".effigy/runtime/fake-catalog");

    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &runtime_catalog.join("effigy.toml"),
        r#"[catalog]
alias = "fake-runtime"
"#,
    );

    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert!(
        catalogs
            .iter()
            .all(|catalog| catalog.alias != "fake-runtime"),
        "runtime artifact dirs under .effigy should not be discovered as catalogs"
    );
}
