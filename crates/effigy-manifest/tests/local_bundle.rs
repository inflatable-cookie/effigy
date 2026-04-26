use effigy_manifest::{load_task_manifest_with_inspection, ManifestManagedRun};

#[test]
fn local_bundle_base_path_resolves_templated_defaults() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/acme");
    std::fs::create_dir_all(&bundle_dir).expect("mkdir bundle");
    std::fs::write(
        bundle_dir.join("bundle.toml"),
        r#"
[bundle]
name = "acme"
description = "Local Acme defaults."

[[inputs]]
name = "host"
type = "string"
required = true
description = "Primary host."

[[inputs]]
name = "port"
type = "integer"
default = 4100
description = "API port."
"#,
    )
    .expect("write descriptor");
    std::fs::write(
        bundle_dir.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[containers.stack]
project_name = "{{ inputs.host | replace('.', '-') }}-dev"
primary_service = "api"

[containers.stack.dns]
routes = [
  { domain = "{{ inputs.host }}", port = {{ inputs.port }}, service = "api" },
]

[tasks.dev]
run = "serve {{ inputs.host }}:{{ inputs.port }}"

[tasks.bundle_path]
run = "bundle {{ bundle.name }} {{ bundle.root }}/scripts/setup.rhai"
"#,
    )
    .expect("write defaults");
    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"
[bundle]
base_path = "bundles/acme"
host = "acme.test"
"#,
    )
    .expect("write manifest");

    let loaded =
        load_task_manifest_with_inspection(&tmp.path().join("effigy.toml")).expect("load manifest");
    let container_source = loaded
        .value_sources
        .iter()
        .find(|source| source.path == "containers.stack.project_name")
        .expect("bundle source");
    assert!(container_source
        .source
        .ends_with("bundles/acme/effigy.toml"));
    let manifest = loaded.manifest;

    let bundle = manifest.bundle.expect("bundle");
    assert_eq!(bundle.base_path.as_deref(), Some("bundles/acme"));

    let containers = manifest.containers.expect("containers");
    let stack = containers.environments.get("stack").expect("stack");
    assert_eq!(stack.project_name.as_deref(), Some("acme-test-dev"));
    let dns = stack.dns.as_ref().expect("dns");
    assert_eq!(dns.routes[0].domain, "acme.test");
    assert_eq!(dns.routes[0].port, Some(4100));

    let task = manifest.tasks.get("dev").expect("dev task");
    assert!(matches!(
        task.run.as_ref().expect("run"),
        ManifestManagedRun::Command(command) if command == "serve acme.test:4100"
    ));
    let bundle_path_task = manifest.tasks.get("bundle_path").expect("bundle path task");
    let expected_bundle_path = format!("bundle acme {}/scripts/setup.rhai", bundle_dir.display());
    assert!(matches!(
        bundle_path_task.run.as_ref().expect("run"),
        ManifestManagedRun::Command(command) if command == &expected_bundle_path
    ));
}

#[test]
fn exported_underlay_bundle_can_be_used_as_base_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/underlay");
    effigy_manifest::export_bundle("underlay", &bundle_dir).expect("export underlay bundle");

    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"[bundle]
base_path = "bundles/underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "underlay-reference"
databases = ["acme", "acme_test"]
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect("exported bundle should load");
    let stack = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("stack"))
        .expect("stack container");
    let postgres = stack.services.get("postgres").expect("postgres service");
    assert_eq!(
        postgres
            .params
            .get("database")
            .and_then(|value| value.as_str()),
        Some("acme")
    );
    assert_eq!(
        postgres
            .params
            .get("databases")
            .and_then(|value| value.as_array())
            .expect("databases list")
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec!["acme", "acme_test"]
    );
    assert_eq!(
        stack
            .services
            .get("dbgate")
            .expect("dbgate service")
            .catalog,
        "dbgate"
    );
    let domains = stack
        .dns
        .as_ref()
        .expect("dns")
        .routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<Vec<_>>();
    assert!(domains.contains(&"dbgate.acme.test"), "got {domains:?}");
    assert!(bundle_dir.join("scripts/dev/ui-setup.rhai").exists());
}

#[test]
fn exported_underlay_bundle_honors_name_overrides() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/underlay");
    effigy_manifest::export_bundle("underlay", &bundle_dir).expect("export underlay bundle");

    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"[bundle]
base_path = "bundles/underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "underlay-reference"
database = "acme"
system_name = "stage"
container_name = "infra"
workspace_service_name = "dev-shell"
default_workspace = "rust"
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect("exported bundle should load with renamed system/container/workspace");
    let containers = loaded.manifest.containers.as_ref().expect("containers");
    assert_eq!(containers.default.as_deref(), Some("infra"));
    let infra = containers
        .environments
        .get("infra")
        .expect("infra container");
    assert_eq!(infra.primary_service.as_deref(), Some("dev-shell"));
    assert!(infra.services.contains_key("dev-shell"));
    assert!(infra.services.contains_key("dbgate"));

    let systems = loaded.manifest.systems.as_ref().expect("systems");
    assert_eq!(systems.default.as_deref(), Some("stage"));
    let stage = systems.systems.get("stage").expect("systems.stage");
    assert_eq!(stage.default_workspace.as_deref(), Some("rust"));
    assert!(stage.workspaces.contains_key("rust"));
}

#[test]
fn exported_decodelabs_bundle_can_be_used_as_base_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/decodelabs");
    effigy_manifest::export_bundle("decodelabs", &bundle_dir).expect("export decodelabs bundle");

    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"[bundle]
base_path = "bundles/decodelabs"
host = "legacy.test"
project_name = "legacy-dev"
databases = ["legacy", "legacy_test"]
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect("exported bundle should load");
    let web = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .expect("web container");
    assert_eq!(
        web.services.get("pma").expect("pma service").catalog,
        "phpmyadmin"
    );
    assert_eq!(
        web.services.get("db").expect("db service").catalog,
        "mariadb"
    );
    assert_eq!(
        web.services
            .get("db")
            .expect("db service")
            .params
            .get("database")
            .and_then(|value| value.as_str()),
        Some("legacy")
    );
    let domains = web
        .dns
        .as_ref()
        .expect("dns")
        .routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<Vec<_>>();
    assert!(domains.contains(&"pma.legacy.test"), "got {domains:?}");
    assert!(bundle_dir.join("scripts/seed-latest-db-dump.rhai").exists());
    let seed_task = loaded.manifest.tasks.get("seed").expect("seed task");
    assert_eq!(
        seed_task.run_in,
        Some(effigy_manifest::ManifestTaskRunIn::Container)
    );
    assert_eq!(seed_task.stay_in_shell, Some(true));
    let release_task = loaded.manifest.tasks.get("release").expect("release task");
    assert!(matches!(
        release_task.run.as_ref().expect("release run"),
        effigy_manifest::ManifestManagedRun::Command(command)
            if command == "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" release"
    ));
    let defer = loaded.manifest.defer.as_ref().expect("bundle defer");
    assert_eq!(
        defer.run,
        "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" {request} {args}"
    );
    assert_eq!(
        defer.run_in,
        Some(effigy_manifest::ManifestTaskRunIn::Container)
    );
}

#[test]
fn exported_decodelabs_library_bundle_can_be_used_as_base_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/decodelabs-library");
    effigy_manifest::export_bundle("decodelabs-library", &bundle_dir)
        .expect("export decodelabs-library bundle");
    let shared_root = tmp.path().join("libraries");
    let repo_root = shared_root.join("clockwork");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");
    let shared_root_path = shared_root
        .canonicalize()
        .unwrap_or_else(|_| shared_root.to_path_buf());
    std::fs::create_dir(repo_root.join(".git")).expect("git dir");
    std::fs::write(
        repo_root.join("effigy.toml"),
        r#"[bundle]
base_path = "../../bundles/decodelabs-library"
"#,
    )
    .expect("write manifest");

    let loaded = load_task_manifest_with_inspection(&repo_root.join("effigy.toml"))
        .expect("exported bundle should load");
    let web = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("web"))
        .expect("web container");
    assert_eq!(
        web.services
            .get("app")
            .expect("app service")
            .params
            .get("mount_source")
            .and_then(|value| value.as_str()),
        Some(shared_root_path.to_str().expect("shared root str"))
    );
    let systems = loaded.manifest.systems.as_ref().expect("systems");
    let dev = systems.systems.get("dev").expect("systems.dev");
    assert_eq!(
        dev.working_dir.as_deref(),
        Some("/workspace-root/clockwork")
    );
    let defer = loaded.manifest.defer.as_ref().expect("bundle defer");
    assert_eq!(
        defer.run,
        "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" {request} {args}"
    );
    assert_eq!(
        defer.run_in,
        Some(effigy_manifest::ManifestTaskRunIn::Container)
    );
}

#[test]
fn local_bundle_rejects_unknown_inputs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundle");
    std::fs::create_dir_all(&bundle_dir).expect("mkdir bundle");
    std::fs::write(
        bundle_dir.join("bundle.toml"),
        r#"
[bundle]
name = "strict"

[[inputs]]
name = "host"
type = "string"
required = true
"#,
    )
    .expect("write descriptor");
    std::fs::write(
        bundle_dir.join("effigy.toml"),
        "[tasks.dev]\nrun = \"ok\"\n",
    )
    .expect("write defaults");
    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"
[bundle]
base_path = "bundle"
host = "acme.test"
typo = "nope"
"#,
    )
    .expect("write manifest");

    let error = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect_err("unknown input should fail");
    assert!(
        error
            .to_string()
            .contains("local bundle `strict` does not declare input `typo`"),
        "{error}"
    );
}

#[test]
fn bundle_selectors_are_mutually_exclusive() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"
[bundle]
base = "underlay"
base_path = "bundle"
"#,
    )
    .expect("write manifest");

    let error = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect_err("mixed selectors should fail");
    assert!(
        error
            .to_string()
            .contains("cannot set both `base` and `base_path`"),
        "{error}"
    );
}
