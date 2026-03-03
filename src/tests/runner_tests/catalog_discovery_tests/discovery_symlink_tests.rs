use super::prelude::*;

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
    let dairy = create_workspace_dir(&root, "dairy");
    let external = create_workspace_dir(&root, "external");
    let underlay_src = create_workspace_dir(&external, "underlay");

    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
"#,
    );
    write_manifest(
        &underlay_src.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
"#,
    );
    symlink(&underlay_src, root.join("underlay")).expect("symlink underlay");

    let err = discover_catalogs(&root).expect_err("expected alias conflict");
    assert_catalog_alias_conflict(err, "dairy");
}
