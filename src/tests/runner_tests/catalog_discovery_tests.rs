use super::*;

fn create_workspace_dir(root: &PathBuf, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).expect("mkdir workspace dir");
    dir
}

fn write_catalog_tasks(dir: &PathBuf, alias: Option<&str>, tasks: &[(&str, &str)]) {
    let mut manifest = String::new();
    if let Some(alias) = alias {
        manifest.push_str(&format!("[catalog]\nalias = \"{}\"\n", alias));
    }
    for (task, run) in tasks {
        manifest.push_str(&format!("[tasks.{}]\nrun = \"{}\"\n", task, run));
    }
    write_manifest(&dir.join("effigy.toml"), &manifest);
}

fn assert_task_ambiguous_reset_db(err: RunnerError) {
    match err {
        RunnerError::TaskAmbiguous { name, candidates } => {
            assert_eq!(name, "reset-db");
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_catalog_alias_conflict(err: RunnerError, expected_alias: &str) {
    match err {
        RunnerError::TaskCatalogAliasConflict {
            alias,
            first_path,
            second_path,
        } => {
            assert_eq!(alias, expected_alias);
            assert!(first_path.ends_with("effigy.toml"));
            assert!(second_path.ends_with("effigy.toml"));
            assert_ne!(first_path, second_path);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_prefixed_uses_named_catalog() {
    let root = temp_workspace("prefixed");
    let farmyard = create_workspace_dir(&root, "farmyard");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_catalog_tasks(&farmyard, Some("farmyard"), &[("ping", "printf farmyard")]);

    let out = run_builtin_ok(root.clone(), "farmyard/ping", &[]);
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
    write_catalog_tasks(&farmyard, Some("farmyard"), &[("ping", "printf farmyard")]);

    let out = run_builtin_ok(nested, "ping", &[]);
    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_unprefixed_reports_ambiguity_on_equal_shallow_depth() {
    let root = temp_workspace("ambiguous");
    let farmyard = create_workspace_dir(&root, "farmyard");
    let dairy = create_workspace_dir(&root, "dairy");

    write_catalog_tasks(
        &farmyard,
        Some("farmyard"),
        &[("reset-db", "printf farmyard")],
    );
    write_catalog_tasks(&dairy, Some("dairy"), &[("reset-db", "printf dairy")]);

    let err = run_builtin_err(root.clone(), "reset-db", &[]);

    assert_task_ambiguous_reset_db(err);
}

#[test]
fn run_manifest_task_relative_prefix_resolves_catalog_by_path() {
    let root = temp_workspace("relative-prefix-path");
    let dairy = create_workspace_dir(&root, "dairy");
    let froyo = create_workspace_dir(&root, "froyo");

    write_catalog_tasks(&dairy, Some("dairy"), &[("dev", "printf dairy")]);
    write_catalog_tasks(
        &froyo,
        Some("froyo"),
        &[("validate", "printf froyo-validate")],
    );

    let out = run_builtin_ok(dairy, "../froyo/validate", &[]);
    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_relative_prefix_prefers_alias_collision_over_path_resolution() {
    let root = temp_workspace("relative-prefix-alias-collision");
    let dairy = create_workspace_dir(&root, "dairy");
    let alias_override = create_workspace_dir(&root, "alias-override");
    let froyo = create_workspace_dir(&root, "froyo");

    write_catalog_tasks(&dairy, Some("dairy"), &[("dev", "printf dairy")]);
    write_catalog_tasks(
        &alias_override,
        Some("../froyo"),
        &[("validate", "printf alias")],
    );
    write_catalog_tasks(&froyo, Some("froyo"), &[("validate", "printf froyo")]);

    let out = run_builtin_ok(dairy, "../froyo/validate", &["--verbose-root"]);

    assert_contains_all(
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
    let app = root.join("apps/web/src");
    let shared = root.join("shared");
    fs::create_dir_all(&app).expect("mkdir app");
    fs::create_dir_all(&shared).expect("mkdir shared");

    write_catalog_tasks(&shared, Some("shared"), &[("lint", "printf shared-lint")]);

    let out = run_builtin_ok(app, "../../../shared/lint", &["--verbose-root"]);

    assert_contains_all(
        &out,
        &[
            "catalog-alias: shared",
            "relative prefix `../../../shared` -> `shared`",
        ],
    );
}

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

    let out = run_builtin_ok(root, "underlay/ping", &[]);
    assert_eq!(out, "");
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
