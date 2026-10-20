use super::*;
use std::fs;

fn write_fragment(root: &Path, name: &str) {
    let dir = root.join(name);
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(
        dir.join("service.toml"),
        format!("[service]\nname = \"{name}\"\n"),
    )
    .expect("service.toml");
    fs::write(dir.join("compose.fragment.yml"), "image: fixture\n").expect("compose");
}

#[test]
fn catalog_list_reports_baseline_fragments() {
    let root = tempfile::tempdir().expect("tempdir");
    let resolver = CatalogResolver::new(None);
    let rendered = run_service_list(root.path(), &resolver, false).expect("list");
    assert!(rendered.contains("[service]"), "{rendered}");
    assert!(rendered.contains("(sources: project, baseline)"), "{rendered}");
    assert!(rendered.contains("php-fpm [baseline]"), "{rendered}");
}

#[test]
fn catalog_list_names_sources_paths_and_shadows() {
    let root = tempfile::tempdir().expect("tempdir");
    let project = root.path().join("infra/dev/catalog");
    let extra = root.path().join("extra");
    write_fragment(&project, "redis");
    write_fragment(&extra, "queue");
    let resolver = CatalogResolver::new(Some(project))
        .with_declared_source("acme", extra)
        .expect("source");

    let text = run_service_list(root.path(), &resolver, false).expect("list");
    assert!(
        text.contains("redis [project: infra/dev/catalog/redis] shadows baseline"),
        "{text}"
    );
    assert!(text.contains("queue [acme: extra/queue]"), "{text}");

    let payload: serde_json::Value =
        serde_json::from_str(&run_service_list(root.path(), &resolver, true).expect("list"))
            .expect("json");
    assert_eq!(payload["schema"], "effigy.service.list.v2");
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(
        payload["sources"],
        serde_json::json!(["project", "acme", "baseline"])
    );
    let fragments = payload["fragments"].as_array().expect("fragments");
    let redis = fragments
        .iter()
        .find(|fragment| fragment["name"] == "redis")
        .expect("redis");
    assert_eq!(redis["qualified"], "project/redis");
    assert_eq!(redis["layer"], "project");
    assert_eq!(redis["path"], "infra/dev/catalog/redis");
    assert_eq!(redis["shadows"][0]["source"], "baseline");
    assert!(redis["shadows"][0]["path"].is_null());
    let queue = fragments
        .iter()
        .find(|fragment| fragment["name"] == "queue")
        .expect("queue");
    assert_eq!(queue["qualified"], "acme/queue");
    assert_eq!(queue["layer"], "declared");
    let postgres = fragments
        .iter()
        .find(|fragment| fragment["name"] == "postgres")
        .expect("postgres");
    assert_eq!(postgres["source"], "baseline");
    assert_eq!(postgres["shadows"], serde_json::json!([]));
}

#[test]
fn catalog_extract_defaults_to_project_catalog_dir() {
    let root = tempfile::tempdir().expect("tempdir");
    let resolver = CatalogResolver::new(None);
    let rendered =
        run_service_extract(root.path(), &resolver, "nginx", None, false).expect("extract");

    assert!(rendered.contains("infra/dev/catalog/nginx"));
    assert!(root
        .path()
        .join("infra/dev/catalog/nginx/service.toml")
        .exists());
    assert!(root
        .path()
        .join("infra/dev/catalog/nginx/compose.fragment.yml")
        .exists());
}

#[test]
fn catalog_extract_accepts_only_the_baseline_qualifier() {
    let root = tempfile::tempdir().expect("tempdir");
    let resolver = CatalogResolver::new(None);
    run_service_extract(root.path(), &resolver, "baseline/redis", None, false).expect("extract");
    assert!(root.path().join("infra/dev/catalog/redis").is_dir());
    assert!(run_service_extract(root.path(), &resolver, "acme/redis", None, false).is_err());
}
