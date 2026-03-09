use super::prelude::{
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
    assert_catalog_alias_conflict(err, "catalog_b");
}
