use effigy_manifest::config_sections::ManifestWorkspaceContainerRef;
use effigy_manifest::load_task_manifest_with_inspection;
use effigy_manifest::{ManifestManagedRun, ManifestManagedRunStep, ManifestTaskRunIn};

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
databases = ["contactpatch"]

[containers.web.services.db]
version = "11.0"
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let bundle_root = loaded.bundle_root.clone().expect("bundle root");
    let seed_script = bundle_root.join("scripts/seed-latest-db-dump.rhai");
    let seed_script_source = std::fs::read_to_string(&seed_script).expect("seed script");
    assert!(
        seed_script_source.contains("reset and seeded database"),
        "decodelabs bundle should materialize its seed helper at {}",
        seed_script.display()
    );
    assert!(
        seed_script_source.contains(r#"process::run("mysql", reset_args)"#)
            && seed_script_source.contains("stdin_file: dump_repo_path")
            && !seed_script_source.contains("container::exec("),
        "decodelabs seed helper should import dumps locally inside the container task at {}",
        seed_script.display()
    );
    let env_script = bundle_root.join("scripts/write-env-if-present.rhai");
    let env_script_source = std::fs::read_to_string(&env_script).expect("env script");
    assert!(
        env_script_source.contains("bootstrap .env template not present"),
        "decodelabs bundle should materialize its env helper at {}",
        env_script.display()
    );
    let manifest = loaded.manifest;

    let bundle = manifest.bundle.expect("bundle");
    assert_eq!(bundle.base.as_deref(), Some("decodelabs"));

    let containers = manifest.containers.expect("containers");
    assert_eq!(containers.default.as_deref(), Some("web"));
    let web = containers.environments.get("web").expect("web container");
    assert_eq!(web.project_name.as_deref(), Some("contactpatch-dev"));
    assert_eq!(web.working_dir.as_deref(), Some("/var/www/contact-patch"));

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
    assert_eq!(
        app.params
            .get("mount_host_ssh_dir")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        app.params
            .get("isolated_dirs")
            .and_then(|value| value.as_array())
            .expect("isolated_dirs")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec!["vendor", "node_modules"]
    );
    assert_eq!(
        app.params
            .get("extensions")
            .and_then(|value| value.as_array())
            .expect("extensions")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec![
            "bcmath",
            "apcu",
            "bz2",
            "calendar",
            "curl",
            "gmp",
            "imagick",
            "mbstring",
            "pcntl",
            "exif",
            "gd",
            "intl",
            "memcached",
            "mysqli",
            "opcache",
            "pdo_mysql",
            "readline",
            "redis",
            "sockets",
            "sqlite3",
            "xml",
            "zip",
            "event",
        ]
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

    let web_service = web.services.get("web").expect("web service");
    assert_eq!(web_service.catalog, "nginx");
    assert_eq!(web_service.variant.as_deref(), Some("decodelabs"));

    let mail = web.services.get("mail").expect("mail service");
    assert_eq!(mail.catalog, "mailpit");

    let dns = web.dns.as_ref().expect("dns");
    let domains = dns
        .routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<Vec<_>>();
    assert!(domains.contains(&"contact-patch.legacy.test"));
    assert!(domains.contains(&"pma.contact-patch.legacy.test"));
    assert!(domains.contains(&"mailpit.contact-patch.legacy.test"));

    let systems = manifest.systems.expect("systems");
    assert_eq!(systems.default.as_deref(), Some("dev"));
    let dev = systems.systems.get("dev").expect("systems.dev");
    assert_eq!(dev.default_workspace.as_deref(), Some("app"));

    let bootstrap = manifest.bootstrap.expect("bootstrap");
    assert!(matches!(
        bootstrap.run.as_ref().expect("bootstrap run"),
        ManifestManagedRun::Sequence(steps)
            if matches!(
                steps.as_slice(),
                [ManifestManagedRunStep::Step(step)]
                    if step
                        .rhai
                        .as_deref()
                        .is_some_and(|path| path.ends_with("/scripts/write-env-if-present.rhai"))
            )
    ));
    let start = bootstrap.start.expect("bootstrap start");
    assert_eq!(start.to_owned_selectors(), vec!["dev".to_owned()]);

    let task = manifest.tasks.get("seed").expect("seed task");
    assert_eq!(task.workspace.as_deref(), Some("app"));
    assert_eq!(task.run_in, Some(ManifestTaskRunIn::Container));
    assert_eq!(task.stay_in_shell, Some(true));
    assert!(matches!(
        task.run.as_ref().expect("seed run"),
        ManifestManagedRun::Sequence(steps)
            if matches!(
                steps.as_slice(),
                [ManifestManagedRunStep::Step(step)]
                    if step
                        .rhai
                        .as_deref()
                        .is_some_and(|path| path.ends_with("/scripts/seed-latest-db-dump.rhai"))
            )
    ));

    let bootstrap_seed_task = manifest
        .tasks
        .get("bootstrap:db-seed")
        .expect("bootstrap db seed task");
    assert_eq!(bootstrap_seed_task.workspace.as_deref(), Some("app"));
    assert_eq!(
        bootstrap_seed_task.run_in,
        Some(ManifestTaskRunIn::Container)
    );
    assert!(matches!(
        bootstrap_seed_task.run.as_ref().expect("bootstrap db seed run"),
        ManifestManagedRun::Sequence(steps)
            if matches!(
                steps.as_slice(),
                [ManifestManagedRunStep::Step(step)]
                    if step
                        .rhai
                        .as_deref()
                        .is_some_and(|path| path.ends_with("/scripts/seed-latest-db-dump.rhai"))
            )
    ));

    let release_task = manifest.tasks.get("release").expect("release task");
    assert_eq!(release_task.workspace.as_deref(), None);
    assert!(matches!(
        release_task.run.as_ref().expect("release run"),
        ManifestManagedRun::Command(command)
            if command == "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" release"
    ));
    let defer = manifest.defer.as_ref().expect("bundle defer");
    assert_eq!(
        defer.run,
        "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" {request} {args}"
    );
    assert_eq!(defer.run_in, Some(ManifestTaskRunIn::Container));
}

#[test]
fn decodelabs_bundle_renames_system_container_and_workspace_service() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = "decodelabs"
host = "contact-patch.legacy.test"
project_name = "contactpatch-dev"
databases = ["contactpatch"]
system_name = "stage"
container_name = "shop"
workspace_service_name = "php"
default_workspace = "frontend"
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let manifest = loaded.manifest;

    let containers = manifest.containers.expect("containers");
    assert_eq!(containers.default.as_deref(), Some("shop"));
    let shop = containers.environments.get("shop").expect("shop container");
    assert_eq!(shop.primary_service.as_deref(), Some("php"));
    assert!(shop.services.contains_key("php"));
    assert!(!shop.services.contains_key("app"));

    let composer_alias = shop.aliases.get("composer").expect("composer alias");
    assert_eq!(composer_alias.service(), "php");
    let php_alias = shop.aliases.get("php").expect("php alias");
    assert_eq!(php_alias.service(), "php");

    let systems = manifest.systems.expect("systems");
    assert_eq!(systems.default.as_deref(), Some("stage"));
    let stage = systems.systems.get("stage").expect("systems.stage");
    assert_eq!(stage.default_workspace.as_deref(), Some("frontend"));
    assert!(stage.workspaces.contains_key("frontend"));

    let stage_frontend = stage
        .workspaces
        .get("frontend")
        .expect("frontend workspace");
    match stage_frontend
        .container
        .as_ref()
        .expect("frontend container")
    {
        ManifestWorkspaceContainerRef::Named(name) => assert_eq!(name, "shop"),
        other => panic!("expected named container ref, got {other:?}"),
    }

    let dev_task = manifest.tasks.get("dev").expect("dev task");
    assert_eq!(dev_task.workspace.as_deref(), Some("frontend"));
}

#[test]
fn decodelabs_bundle_derives_working_dir_from_host_label() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = "decodelabs"
host = "cbs.legacy.test"
project_name = "cbs-dev"
databases = ["cbs"]
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let web = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .expect("web container");

    assert_eq!(web.working_dir.as_deref(), Some("/var/www/cbs"));
}

#[test]
fn decodelabs_bundle_rejects_legacy_name_key() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
name = "decodelabs"
host = "contact-patch.legacy.test"
project_name = "contactpatch-dev"
databases = ["contactpatch"]
"#,
    )
    .expect("write manifest");

    let error =
        load_task_manifest_with_inspection(&manifest_path).expect_err("legacy name should fail");
    assert!(
        error
            .to_string()
            .contains("must set either `base` for a shipped bundle or `base_path` for a local bundle directory"),
        "got: {error}"
    );
}

#[test]
fn decodelabs_bundle_hydrates_primary_database_from_databases_list() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = "decodelabs"
host = "contact-patch.legacy.test"
project_name = "contactpatch-dev"
databases = ["contactpatch", "contactpatch_test"]
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let db = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .and_then(|web| web.services.get("db"))
        .expect("db service");

    assert_eq!(
        db.params.get("database").and_then(|value| value.as_str()),
        Some("contactpatch")
    );
    assert_eq!(
        db.params
            .get("databases")
            .and_then(|value| value.as_array())
            .expect("databases list")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec!["contactpatch", "contactpatch_test"]
    );
}

#[test]
fn decodelabs_bundle_can_publish_optional_zest_route() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = "decodelabs"
host = "gideon.legacy.test"
project_name = "gideon-dev"
databases = ["gideon"]
zest_port = 8938
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let web = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .expect("web container");
    let app = web.services.get("app").expect("app service");
    let host_ports = app
        .params
        .get("host_ports")
        .and_then(|value| value.as_array())
        .expect("host_ports");
    assert_eq!(
        host_ports
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec!["8938:8938"]
    );

    let dns = web.dns.as_ref().expect("dns");
    let routes = dns
        .routes
        .iter()
        .map(|route| {
            (
                route.domain.as_str(),
                route.service.as_deref(),
                route.port,
                route.tls,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        routes,
        vec![
            ("gideon.legacy.test", Some("web"), None, Some(true)),
            ("pma.gideon.legacy.test", Some("pma"), None, Some(true)),
            (
                "mailpit.gideon.legacy.test",
                Some("mail"),
                Some(8025),
                Some(true)
            ),
            (
                "zest.gideon.legacy.test",
                Some("app"),
                Some(8938),
                Some(true)
            ),
        ]
    );
}

#[test]
fn decodelabs_bundle_can_extend_bundle_provided_dns_routes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[manifest]
extend = ["containers.web.dns.routes"]

[bundle]
base = "decodelabs"
host = "cbs.legacy.test"
project_name = "cbs-dev"
databases = ["cbs"]

[containers.web.dns]
routes = [
  { domain = "borderway.legacy.test", tls = true, service = "web" },
]
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let web = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .expect("web container");
    let dns = web.dns.as_ref().expect("dns");
    let routes = dns
        .routes
        .iter()
        .map(|route| (route.domain.as_str(), route.service.as_deref(), route.tls))
        .collect::<Vec<_>>();

    assert_eq!(
        routes,
        vec![
            ("cbs.legacy.test", Some("web"), Some(true)),
            ("pma.cbs.legacy.test", Some("pma"), Some(true)),
            ("mailpit.cbs.legacy.test", Some("mail"), Some(true)),
            ("borderway.legacy.test", Some("web"), Some(true)),
        ]
    );
}

#[test]
fn decodelabs_bundle_in_imported_fragment_honors_child_manifest_extend() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root_path = tmp.path().join("effigy.toml");
    let local_path = tmp.path().join("effigy.local.toml");
    std::fs::write(
        &root_path,
        r#"
[tasks]
deploy = "true"
"#,
    )
    .expect("write root manifest");
    std::fs::write(
        &local_path,
        r#"
[manifest]
extend = ["containers.web.dns.routes"]

[bundle]
base = "decodelabs"
host = "cbs.legacy.test"
project_name = "cbs-dev"
databases = ["cbs"]

[containers.web.dns]
routes = [
  { domain = "borderway.legacy.test", tls = true, service = "web" },
]
"#,
    )
    .expect("write local manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&root_path).expect("load manifest");
    let web = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .expect("web container");
    let dns = web.dns.as_ref().expect("dns");
    let routes = dns
        .routes
        .iter()
        .map(|route| (route.domain.as_str(), route.service.as_deref(), route.tls))
        .collect::<Vec<_>>();

    assert_eq!(
        routes,
        vec![
            ("cbs.legacy.test", Some("web"), Some(true)),
            ("pma.cbs.legacy.test", Some("pma"), Some(true)),
            ("mailpit.cbs.legacy.test", Some("mail"), Some(true)),
            ("borderway.legacy.test", Some("web"), Some(true)),
        ]
    );
}

#[test]
fn decodelabs_bundle_in_imported_fragment_ignores_unrelated_child_extend_paths() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root_path = tmp.path().join("effigy.toml");
    let local_path = tmp.path().join("effigy.local.toml");
    std::fs::write(
        &root_path,
        r#"
[tasks]
deploy = "true"
"#,
    )
    .expect("write root manifest");
    std::fs::write(
        &local_path,
        r#"
[manifest]
extend = ["containers.web.dns.routes", "containers.web.dns.domains"]

[bundle]
base = "decodelabs"
host = "cbs.legacy.test"
project_name = "cbs-dev"
databases = ["cbs"]

[containers.web.dns]
domains = ["admin.cbs.legacy.test"]
domain_defaults = { tls = true, service = "web" }
routes = [
  { domain = "borderway.legacy.test", tls = true, service = "web" },
]
"#,
    )
    .expect("write local manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&root_path).expect("load manifest");
    let web = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .expect("web container");
    let dns = web.dns.as_ref().expect("dns");
    let routes = dns
        .routes
        .iter()
        .map(|route| (route.domain.as_str(), route.service.as_deref(), route.tls))
        .collect::<Vec<_>>();

    assert_eq!(
        routes,
        vec![
            ("cbs.legacy.test", Some("web"), Some(true)),
            ("pma.cbs.legacy.test", Some("pma"), Some(true)),
            ("mailpit.cbs.legacy.test", Some("mail"), Some(true)),
            ("borderway.legacy.test", Some("web"), Some(true)),
        ]
    );
}

#[test]
fn decodelabs_library_bundle_derives_shared_workspace_runtime() {
    let shared_root = tempfile::tempdir().expect("shared root tempdir");
    let shared_root_path = shared_root
        .path()
        .canonicalize()
        .unwrap_or_else(|_| shared_root.path().to_path_buf());
    let repo_root = shared_root.path().join("collections");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");
    std::fs::create_dir(repo_root.join(".git")).expect("git dir");
    let manifest_path = repo_root.join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = "decodelabs-library"
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let manifest = loaded.manifest;
    let bundle = manifest.bundle.expect("bundle");
    assert_eq!(bundle.base.as_deref(), Some("decodelabs-library"));

    let containers = manifest.containers.expect("containers");
    let web = containers.environments.get("web").expect("web container");
    assert_eq!(
        web.project_name.as_deref(),
        Some("decodelabs-library-collections-dev")
    );
    assert_eq!(web.working_dir.as_deref(), Some("/workspace-root"));
    let app = web.services.get("app").expect("app service");
    assert_eq!(app.catalog, "php-fpm");
    assert_eq!(
        app.params
            .get("working_dir")
            .and_then(|value| value.as_str()),
        Some("/workspace-root/collections")
    );
    assert_eq!(
        app.params
            .get("node_version")
            .and_then(|value| value.as_str()),
        Some("20")
    );
    assert_eq!(
        app.params
            .get("node_global_packages")
            .and_then(|value| value.as_array())
            .expect("node globals")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec!["eclint"]
    );
    assert_eq!(
        app.params
            .get("mount_source")
            .and_then(|value| value.as_str()),
        Some(shared_root_path.to_str().expect("shared root str"))
    );
    assert_eq!(
        app.params
            .get("isolated_dirs")
            .and_then(|value| value.as_array())
            .expect("isolated_dirs")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec!["vendor"]
    );
    assert_eq!(
        app.params
            .get("extensions")
            .and_then(|value| value.as_array())
            .expect("extensions")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec![
            "bcmath",
            "apcu",
            "bz2",
            "calendar",
            "curl",
            "gmp",
            "imagick",
            "mbstring",
            "pcntl",
            "exif",
            "gd",
            "intl",
            "memcached",
            "mysqli",
            "opcache",
            "pdo_mysql",
            "readline",
            "redis",
            "sockets",
            "sqlite3",
            "xml",
            "zip",
            "event",
        ]
    );

    let systems = manifest.systems.expect("systems");
    let dev = systems.systems.get("dev").expect("systems.dev");
    assert_eq!(
        dev.working_dir.as_deref(),
        Some("/workspace-root/collections")
    );
    assert_eq!(dev.user.as_deref(), Some("dev"));
    assert_eq!(dev.home.as_deref(), Some("/home/dev"));

    let defer = manifest.defer.as_ref().expect("bundle defer");
    assert_eq!(
        defer.run,
        "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" {request} {args}"
    );
    assert_eq!(defer.run_in, Some(ManifestTaskRunIn::Container));
    assert!(manifest.tasks.get("seed").is_none());
}
