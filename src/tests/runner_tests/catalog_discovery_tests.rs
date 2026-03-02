use super::*;

#[test]
fn run_manifest_task_prefixed_uses_named_catalog() {
    let root = temp_workspace("prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf farmyard\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/ping".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("run");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_unprefixed_prefers_nearest_catalog_in_scope() {
    let root = temp_workspace("nearest");
    let farmyard = root.join("farmyard");
    let nested = farmyard.join("crates/api");
    fs::create_dir_all(&nested).expect("mkdir");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf farmyard\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "ping".to_owned(),
            args: Vec::new(),
        },
        nested,
    )
    .expect("run");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_unprefixed_reports_ambiguity_on_equal_shallow_depth() {
    let root = temp_workspace("ambiguous");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");

    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf farmyard\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf dairy\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "reset-db".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect_err("expected ambiguity");

    match err {
        RunnerError::TaskAmbiguous { name, candidates } => {
            assert_eq!(name, "reset-db");
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_relative_prefix_resolves_catalog_by_path() {
    let root = temp_workspace("relative-prefix-path");
    let dairy = root.join("dairy");
    let froyo = root.join("froyo");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    fs::create_dir_all(&froyo).expect("mkdir froyo");

    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.dev]\nrun = \"printf dairy\"\n",
    );
    write_manifest(
        &froyo.join("effigy.toml"),
        "[catalog]\nalias = \"froyo\"\n[tasks.validate]\nrun = \"printf froyo-validate\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "../froyo/validate".to_owned(),
            args: Vec::new(),
        },
        dairy,
    )
    .expect("relative path task should resolve");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_relative_prefix_prefers_alias_collision_over_path_resolution() {
    let root = temp_workspace("relative-prefix-alias-collision");
    let dairy = root.join("dairy");
    let alias_override = root.join("alias-override");
    let froyo = root.join("froyo");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    fs::create_dir_all(&alias_override).expect("mkdir alias-override");
    fs::create_dir_all(&froyo).expect("mkdir froyo");

    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.dev]\nrun = \"printf dairy\"\n",
    );
    write_manifest(
        &alias_override.join("effigy.toml"),
        "[catalog]\nalias = \"../froyo\"\n[tasks.validate]\nrun = \"printf alias\"\n",
    );
    write_manifest(
        &froyo.join("effigy.toml"),
        "[catalog]\nalias = \"froyo\"\n[tasks.validate]\nrun = \"printf froyo\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "../froyo/validate".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        },
        dairy,
    )
    .expect("relative prefix should resolve via alias first");

    assert!(out.contains("catalog-alias: ../froyo"));
    assert!(out.contains("selected catalog via explicit prefix `../froyo`"));
}

#[test]
fn run_manifest_task_relative_prefix_supports_multi_parent_traversal() {
    let root = temp_workspace("relative-prefix-multi-parent");
    let app = root.join("apps/web/src");
    let shared = root.join("shared");
    fs::create_dir_all(&app).expect("mkdir app");
    fs::create_dir_all(&shared).expect("mkdir shared");

    write_manifest(
        &shared.join("effigy.toml"),
        "[catalog]\nalias = \"shared\"\n[tasks.lint]\nrun = \"printf shared-lint\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "../../../shared/lint".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        },
        app,
    )
    .expect("multi-parent relative task should resolve");

    assert!(out.contains("catalog-alias: shared"));
    assert!(out.contains("relative prefix `../../../shared` -> `shared`"));
}

#[test]
fn discover_catalogs_includes_symlinked_catalog_directories() {
    let root = temp_workspace("catalog-symlink-discovery");
    let external = root.join("external");
    let underlay_src = external.join("underlay");
    fs::create_dir_all(&underlay_src).expect("mkdir underlay src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "acowtancy"
"#,
    );
    write_manifest(
        &underlay_src.join("effigy.toml"),
        r#"[catalog]
alias = "underlay"

[tasks.ping]
run = "printf underlay"
"#,
    );
    symlink(&underlay_src, root.join("underlay")).expect("symlink underlay");

    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert!(
        catalogs.iter().any(|catalog| catalog.alias == "underlay"),
        "symlinked underlay catalog should be discovered"
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "underlay/ping".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("run symlinked prefixed task");
    assert_eq!(out, "");
}

#[cfg(unix)]
#[test]
fn discover_catalogs_reports_alias_conflict_for_symlinked_catalog() {
    let root = temp_workspace("catalog-symlink-alias-conflict");
    let dairy = root.join("dairy");
    let external = root.join("external");
    let underlay_src = external.join("underlay");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    fs::create_dir_all(&underlay_src).expect("mkdir underlay src");

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
    match err {
        RunnerError::TaskCatalogAliasConflict {
            alias,
            first_path,
            second_path,
        } => {
            assert_eq!(alias, "dairy");
            assert!(first_path.ends_with("effigy.toml"));
            assert!(second_path.ends_with("effigy.toml"));
            assert_ne!(first_path, second_path);
        }
        other => panic!("unexpected error: {other}"),
    }
}
