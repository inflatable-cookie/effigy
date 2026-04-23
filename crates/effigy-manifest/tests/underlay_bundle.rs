use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::load_task_manifest_with_inspection;

#[test]
fn underlay_bundle_resolves_defaults_and_allows_repo_overrides() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = "underlay"
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
database = "acme"

[systems.dev]
mounts = ["../underlay", "../poodle"]

[tasks.seed]
host = true
run = "printf seed"
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");
    let stale_bundle_root = tmp
        .path()
        .join(".effigy/runtime/bundles/underlay/stale-hash/scripts/dev");
    std::fs::create_dir_all(&stale_bundle_root).expect("stale cache dir");
    std::fs::write(stale_bundle_root.join("ui-setup.rhai"), "stale").expect("stale asset");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".gitignore")).expect("gitignore"),
        ".effigy\n"
    );
    let bundle_root = loaded.bundle_root.clone().expect("bundle root");
    assert!(bundle_root.starts_with(tmp.path().join(".effigy/runtime/bundles/underlay")));
    assert!(
        !tmp.path()
            .join(".effigy/runtime/bundles/underlay/stale-hash")
            .exists(),
        "loading the bundle should prune stale materialized asset roots"
    );
    let setup_script = bundle_root.join("scripts/dev/ui-setup.rhai");
    let setup_script_source = std::fs::read_to_string(&setup_script).expect("setup script");
    assert!(
        setup_script_source.contains("unsupported ui setup target"),
        "underlay bundle should materialize its Rhai assets at {}",
        setup_script.display()
    );
    let manifest = loaded.manifest;

    let bundle = manifest.bundle.expect("bundle");
    assert_eq!(bundle.base.as_deref(), Some("underlay"));

    let package_manager = manifest.package_manager.expect("package manager");
    assert_eq!(package_manager.js, Some(ManifestJsPackageManager::Bun));

    let systems = manifest.systems.expect("systems");
    assert_eq!(systems.default.as_deref(), Some("dev"));
    let dev = systems.systems.get("dev").expect("systems.dev");
    assert_eq!(
        dev.working_dir.as_deref(),
        Some("/workspace-root/underlay-reference")
    );
    assert_eq!(dev.user.as_deref(), Some("dev"));
    assert_eq!(dev.home.as_deref(), Some("/home/dev"));
    assert_eq!(dev.mounts, vec!["../underlay", "../poodle"]);
    assert!(dev.workspaces.contains_key("app"));

    let containers = manifest.containers.expect("containers");
    assert_eq!(containers.default.as_deref(), Some("stack"));
    let stack = containers.environments.get("stack").expect("stack");
    assert_eq!(
        stack.project_name.as_deref(),
        Some("underlay-reference-dev")
    );
    assert_eq!(stack.primary_service.as_deref(), Some("workspace"));

    let workspace = stack.services.get("workspace").expect("workspace service");
    assert_eq!(workspace.catalog, "workspace-rust-bun");
    assert_eq!(
        workspace
            .params
            .get("working_subdir")
            .and_then(|value| value.as_str()),
        Some("underlay-reference")
    );
    let host_ports = workspace
        .params
        .get("host_ports")
        .and_then(|value| value.as_array())
        .expect("workspace host_ports");
    let rendered_ports: Vec<&str> = host_ports
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(
        rendered_ports,
        vec!["41001:41001", "41002:41002", "41003:41003"]
    );

    let postgres = stack.services.get("postgres").expect("postgres service");
    assert_eq!(postgres.catalog, "postgres");
    assert_eq!(
        postgres
            .params
            .get("database")
            .and_then(|value| value.as_str()),
        Some("acme")
    );
    let dbgate = stack.services.get("dbgate").expect("dbgate service");
    assert_eq!(dbgate.catalog, "dbgate");
    assert_eq!(
        dbgate
            .params
            .get("database_host")
            .and_then(|value| value.as_str()),
        Some("postgres")
    );

    let dns = stack.dns.as_ref().expect("dns");
    let domains = dns
        .routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "acme.test",
        "admin.acme.test",
        "api.acme.test",
        "dbgate.acme.test",
        "mailpit.acme.test",
        "minio.acme.test",
    ] {
        assert!(
            domains.contains(&expected),
            "bundle should publish `{expected}`; got {domains:?}"
        );
    }

    let seed = manifest.tasks.get("seed").expect("seed task");
    assert_eq!(seed.host, Some(true));
}
