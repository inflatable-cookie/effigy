use effigy_manifest::load_task_manifest_with_inspection;

#[test]
fn decodelabs_bundle_resolves_defaults_and_allows_block_overrides() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = "decodelabs"
host = "contact-patch.legacy.test"
project_name = "contactpatch-dev"
database = "contactpatch"

[containers.web.services.db]
version = "11.0"

[tasks.seed]
workspace = "app"
run = [{ rhai = "infra/dev/seed-latest-db-dump.rhai" }]
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let manifest = loaded.manifest;

    let bundle = manifest.bundle.expect("bundle");
    assert_eq!(bundle.base.as_deref(), Some("decodelabs"));

    let containers = manifest.containers.expect("containers");
    assert_eq!(containers.default.as_deref(), Some("web"));
    let web = containers.environments.get("web").expect("web container");
    assert_eq!(web.project_name.as_deref(), Some("contactpatch-dev"));
    assert_eq!(web.working_dir.as_deref(), Some("/var/www/html"));

    let app = web.services.get("app").expect("app service");
    assert_eq!(app.catalog, "php-fpm");
    assert_eq!(
        app.params.get("version").and_then(|value| value.as_str()),
        Some("8.4")
    );
    assert_eq!(
        app.params
            .get("document_root")
            .and_then(|value| value.as_str()),
        Some(".")
    );

    let db = web.services.get("db").expect("db service");
    assert_eq!(db.catalog, "mariadb");
    assert_eq!(
        db.params.get("version").and_then(|value| value.as_str()),
        Some("11.0")
    );
    assert_eq!(
        db.params.get("database").and_then(|value| value.as_str()),
        Some("contactpatch")
    );

    let dns = web.dns.as_ref().expect("dns");
    let domains = dns
        .routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<Vec<_>>();
    assert!(domains.contains(&"contact-patch.legacy.test"));
    assert!(domains.contains(&"pma.contact-patch.legacy.test"));

    let systems = manifest.systems.expect("systems");
    assert_eq!(systems.default.as_deref(), Some("dev"));
    let dev = systems.systems.get("dev").expect("systems.dev");
    assert_eq!(dev.default_workspace.as_deref(), Some("app"));

    let task = manifest.tasks.get("seed").expect("seed task");
    assert_eq!(task.workspace.as_deref(), Some("app"));
}

#[test]
fn decodelabs_bundle_accepts_legacy_name_alias() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
name = "decodelabs"
host = "contact-patch.legacy.test"
project_name = "contactpatch-dev"
database = "contactpatch"
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let bundle = loaded.manifest.bundle.expect("bundle");
    assert_eq!(bundle.base.as_deref(), Some("decodelabs"));
}
