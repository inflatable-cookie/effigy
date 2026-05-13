use std::path::Path;

use effigy_manifest::config_sections::{ManifestJsPackageManager, ManifestWorkspaceContainerRef};
use effigy_manifest::load_task_manifest;
use effigy_manifest::load_task_manifest_with_inspection;
use effigy_manifest::{ManifestManagedRun, ManifestManagedRunStep};
use effigy_manifest::{ManifestSecretsBackend, ManifestSecretsUnlockPolicy, ManifestTaskRunIn};

fn underlay_fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/underlay-bundle")
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

fn setup_underlay_path_bundle(root: &Path) -> std::path::PathBuf {
    let bundle_dir = root.join("bundles/underlay");
    copy_dir_all(&underlay_fixture_dir(), &bundle_dir).expect("copy fixture bundle");
    bundle_dir
}

#[test]
fn underlay_bundle_resolves_defaults_and_allows_repo_overrides() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
databases = ["acme"]

[bundle.dirs]
docs = "acme-docs"

[systems.dev]
mounts = ["../underlay", "../poodle"]

[tasks.seed]
run_in = "host"
run = "printf seed"

[secrets.keys.ai_router_api_key]
required = false
targets = ["tasks", "containers", "rhai"]
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
    let bundle_root = loaded.bundle_root.clone().expect("bundle root");
    assert_eq!(bundle_root, tmp.path().join("bundles/underlay"));
    let setup_script = bundle_root.join("scripts/dev/ui-setup.rhai");
    let setup_script_source = std::fs::read_to_string(&setup_script).expect("setup script");
    assert!(
        setup_script_source.contains("unsupported ui setup target"),
        "underlay bundle should expose its Rhai assets at {}",
        setup_script.display()
    );
    let bootstrap_env_script = bundle_root.join("scripts/generate-dev-secrets.rhai");
    let bootstrap_env_source =
        std::fs::read_to_string(&bootstrap_env_script).expect("dev secret generator script");
    assert!(
        bootstrap_env_source.contains("local dev secret generation complete"),
        "underlay bundle should expose local secret generator helpers at {}",
        bootstrap_env_script.display()
    );
    let error_reporting_script = bundle_root.join("scripts/error-reporting.rhai");
    let error_reporting_source =
        std::fs::read_to_string(&error_reporting_script).expect("error reporting script");
    assert!(
        error_reporting_source.contains("smoke:error-logging"),
        "underlay bundle should expose error-reporting helpers at {}",
        error_reporting_script.display()
    );
    let manifest = loaded.manifest;

    let bundle = manifest.bundle.expect("bundle");
    assert!(matches!(
        bundle.base.as_ref(),
        Some(effigy_manifest::ManifestBundleBase::Path { dir }) if dir.ends_with("bundles/underlay")
    ));

    let package_manager = manifest.package_manager.expect("package manager");
    assert_eq!(package_manager.js, Some(ManifestJsPackageManager::Bun));

    let secrets = manifest.secrets.expect("secrets");
    assert_eq!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault));
    let vault = secrets.vault.expect("secrets vault");
    assert_eq!(vault.path.as_deref(), Some(".effigy/secrets/local.vault"));
    assert_eq!(vault.unlock, Some(ManifestSecretsUnlockPolicy::Passphrase));
    assert!(secrets.keys.contains_key("auth_jwt_private_key"));
    assert!(secrets.keys.contains_key("ai_router_api_key"));

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
        "s3.acme.test",
        "dbgate.acme.test",
        "mailpit.acme.test",
        "minio.acme.test",
    ] {
        assert!(
            domains.contains(&expected),
            "bundle should publish `{expected}`; got {domains:?}"
        );
    }

    let bootstrap = manifest.bootstrap.as_ref().expect("bootstrap");
    let bootstrap_task = bootstrap
        .run
        .as_ref()
        .expect("bootstrap run")
        .as_manifest_task();
    let run = bootstrap_task.run.as_ref().expect("bootstrap task run");
    let ManifestManagedRun::Sequence(steps) = run else {
        panic!("underlay bootstrap should use a managed sequence");
    };
    assert_eq!(
        steps.len(),
        4,
        "expected env helper + container up + minio bootstrap + deps sync"
    );
    let Some(ManifestManagedRunStep::Step(sync_step)) = steps.get(3) else {
        panic!("expected bootstrap deps sync step");
    };
    assert_eq!(
        sync_step.task.as_deref(),
        Some("bootstrap deps sync ../underlay app-api app-client app-ui app-front app-admin")
    );
    assert_eq!(
        bootstrap
            .start
            .as_ref()
            .map(|start| start.to_owned_selectors()),
        Some(vec!["dev".to_owned()])
    );

    let seed = manifest.tasks.get("seed").expect("seed task");
    assert_eq!(seed.run_in(), ManifestTaskRunIn::Host);

    let health = manifest.tasks.get("health").expect("health task");
    let ManifestManagedRun::Sequence(health_steps) = health.run.as_ref().expect("health run")
    else {
        panic!("health should use a managed sequence");
    };
    let health_tasks: Vec<&str> = health_steps
        .iter()
        .map(|step| match step {
            ManifestManagedRunStep::Step(step) => step.task.as_deref().expect("health step task"),
            ManifestManagedRunStep::Command(_) => {
                panic!("health should not use bare command steps")
            }
        })
        .collect();
    assert_eq!(
        health_tasks,
        vec![
            "acme-docs/health",
            "app-api/health",
            "app-client/health",
            "app-admin/health",
            "app-front/health",
        ]
    );

    let validate = manifest.tasks.get("validate").expect("validate task");
    let ManifestManagedRun::Sequence(validate_steps) = validate.run.as_ref().expect("validate run")
    else {
        panic!("validate should use a managed sequence");
    };
    let validate_tasks: Vec<&str> = validate_steps
        .iter()
        .map(|step| match step {
            ManifestManagedRunStep::Step(step) => step.task.as_deref().expect("validate step task"),
            ManifestManagedRunStep::Command(_) => {
                panic!("validate should not use bare command steps")
            }
        })
        .collect();
    assert_eq!(
        validate_tasks,
        vec![
            "underlay/validate",
            "acme-docs/validate",
            "app-api/validate",
            "app-client/validate",
            "app-admin/validate",
            "app-front/validate",
        ]
    );

    let qa = manifest.tasks.get("qa").expect("qa task");
    let ManifestManagedRun::Sequence(qa_steps) = qa.run.as_ref().expect("qa run") else {
        panic!("qa should use a managed sequence");
    };
    let qa_tasks: Vec<&str> = qa_steps
        .iter()
        .map(|step| match step {
            ManifestManagedRunStep::Step(step) => step.task.as_deref().expect("qa step task"),
            ManifestManagedRunStep::Command(_) => panic!("qa should not use bare command steps"),
        })
        .collect();
    assert_eq!(
        qa_tasks,
        vec![
            "health",
            "validate",
            "acme-docs/qa:docs",
            "acme-docs/qa:northstar",
        ]
    );

    let dev_task = manifest.tasks.get("dev").expect("dev task");
    assert_eq!(dev_task.mode.as_deref(), Some("tui"));
    assert_eq!(dev_task.container_lifecycle, Some(true));
    assert_eq!(dev_task.gateway, Some(true));
    assert_eq!(dev_task.health_wait, Some(true));
    assert_eq!(dev_task.concurrent.len(), 6);
    let concurrent_tasks: Vec<Option<&str>> = dev_task
        .concurrent
        .iter()
        .map(|entry| entry.task.as_deref())
        .collect();
    assert_eq!(
        concurrent_tasks,
        vec![
            Some("app-front/dev"),
            Some("app-admin/dev"),
            Some("app-api/api"),
            Some("app-api/jobs"),
            None,
            None,
        ]
    );

    for task_name in [
        "smoke:error-logging",
        "metrics:error-log",
        "validate:error-reporting",
    ] {
        let task = manifest.tasks.get(task_name).expect("error reporting task");
        assert_eq!(
            task.run_in(),
            ManifestTaskRunIn::Host,
            "{task_name} should run on host"
        );
        let ManifestManagedRun::Sequence(steps) = task.run.as_ref().expect("run steps") else {
            panic!("{task_name} should use a Rhai run step");
        };
        let Some(ManifestManagedRunStep::Step(step)) = steps.first() else {
            panic!("{task_name} should contain a Rhai step");
        };
        assert!(
            step.rhai
                .as_deref()
                .is_some_and(|path| path.ends_with("/scripts/error-reporting.rhai")),
            "{task_name} should reference error-reporting.rhai"
        );
    }
}

#[test]
fn underlay_bundle_renames_system_container_and_workspace_service() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
databases = ["acme"]
system_name = "stage"
container_name = "infra"
workspace_service_name = "dev-shell"
default_workspace = "rust"
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let manifest = loaded.manifest;

    let systems = manifest.systems.expect("systems");
    assert_eq!(systems.default.as_deref(), Some("stage"));
    let stage = systems.systems.get("stage").expect("systems.stage");
    assert_eq!(stage.default_workspace.as_deref(), Some("rust"));
    assert!(stage.workspaces.contains_key("rust"));
    match stage.container.as_ref().expect("stage container") {
        ManifestWorkspaceContainerRef::Named(name) => assert_eq!(name, "infra"),
        other => panic!("expected named container ref, got {other:?}"),
    }

    let containers = manifest.containers.expect("containers");
    assert_eq!(containers.default.as_deref(), Some("infra"));
    let infra = containers.environments.get("infra").expect("infra");
    assert_eq!(infra.primary_service.as_deref(), Some("dev-shell"));
    assert!(infra.services.contains_key("dev-shell"));
    assert!(!infra.services.contains_key("workspace"));
    // non-workspace services keep their catalog-driven names
    assert!(infra.services.contains_key("postgres"));
    assert!(infra.services.contains_key("dbgate"));

    let dns = infra.dns.as_ref().expect("dns");
    let http_routes: Vec<(&str, Option<&str>)> = dns
        .routes
        .iter()
        .map(|route| (route.domain.as_str(), route.service.as_deref()))
        .collect();
    assert!(
        http_routes.contains(&("acme.test", Some("dev-shell"))),
        "expected workspace route to point at dev-shell; got {http_routes:?}"
    );
    assert!(
        http_routes.contains(&("admin.acme.test", Some("dev-shell"))),
        "expected admin route to point at dev-shell; got {http_routes:?}"
    );
    // dbgate keeps its own service target, independent of workspace rename
    assert!(
        http_routes.contains(&("dbgate.acme.test", Some("dbgate"))),
        "got {http_routes:?}"
    );
}

#[test]
fn underlay_bundle_uses_client_and_ui_dirs_for_bootstrap_sync() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
databases = ["acme"]

[bundle.dirs]
api = "acme-api"
client = "acme-client"
ui = "acme-ui"
front = "acme-front"
admin = "acme-admin"

[bundle.sources]
underlay = "../../underlay"
poodle = "../../poodle"
"#,
    )
    .expect("write manifest");

    let manifest = load_task_manifest(&manifest_path).expect("load manifest");
    let bootstrap = manifest.bootstrap.as_ref().expect("bootstrap");
    let bootstrap_task = bootstrap
        .run
        .as_ref()
        .expect("bootstrap run")
        .as_manifest_task();
    let run = bootstrap_task.run.as_ref().expect("bootstrap task run");
    let effigy_manifest::ManifestManagedRun::Sequence(steps) = run else {
        panic!("underlay bootstrap should use a managed sequence");
    };
    let Some(effigy_manifest::ManifestManagedRunStep::Step(sync_step)) = steps.get(3) else {
        panic!("expected bootstrap deps sync step");
    };
    assert_eq!(
        sync_step.task.as_deref(),
        Some(
            "bootstrap deps sync ../../underlay acme-api acme-client acme-ui acme-front acme-admin"
        )
    );

    let children = &manifest.bootstrap.as_ref().expect("bootstrap").children;
    let child_paths: Vec<&str> = children.iter().map(|child| child.path.as_str()).collect();
    assert_eq!(child_paths, vec!["../../underlay", "../../poodle"]);
}

#[test]
fn underlay_bundle_infers_sources_from_system_mounts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
databases = ["acme"]

[bundle.dirs]
api = "acme-api"
client = "acme-client"
ui = "acme-ui"
front = "acme-front"
admin = "acme-admin"

[systems.dev]
mounts = ["../../underlay", "../../poodle"]
"#,
    )
    .expect("write manifest");

    let manifest = load_task_manifest(&manifest_path).expect("load manifest");
    let bootstrap = manifest.bootstrap.as_ref().expect("bootstrap");
    let bootstrap_task = bootstrap
        .run
        .as_ref()
        .expect("bootstrap run")
        .as_manifest_task();
    let run = bootstrap_task.run.as_ref().expect("bootstrap task run");
    let effigy_manifest::ManifestManagedRun::Sequence(steps) = run else {
        panic!("underlay bootstrap should use a managed sequence");
    };
    let Some(effigy_manifest::ManifestManagedRunStep::Step(sync_step)) = steps.get(3) else {
        panic!("expected bootstrap deps sync step");
    };
    assert_eq!(
        sync_step.task.as_deref(),
        Some("bootstrap deps sync ../underlay acme-api acme-client acme-ui acme-front acme-admin")
    );

    let children = &manifest.bootstrap.as_ref().expect("bootstrap").children;
    let child_paths: Vec<&str> = children.iter().map(|child| child.path.as_str()).collect();
    assert_eq!(child_paths, vec!["../underlay", "../poodle"]);
}

#[test]
fn underlay_bundle_merges_repo_bootstrap_children_with_bundle_children() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
databases = ["acme"]

[[bootstrap.children]]
path = "../ledger"
repo = "git@github.com:acme/ledger.git"
"#,
    )
    .expect("write manifest");

    let manifest = load_task_manifest(&manifest_path).expect("load manifest");
    let children = &manifest.bootstrap.as_ref().expect("bootstrap").children;
    let child_paths: Vec<&str> = children.iter().map(|child| child.path.as_str()).collect();
    assert_eq!(child_paths, vec!["../underlay", "../poodle", "../ledger"]);
}

#[test]
fn underlay_bundle_hydrates_primary_database_from_databases_list() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
databases = ["acme", "acme_test"]
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let postgres = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("stack"))
        .and_then(|stack| stack.services.get("postgres"))
        .expect("postgres service");

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
}

#[test]
fn underlay_bundle_emits_per_subproject_volume_dirs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
databases = ["acme"]

[bundle.dirs]
api = "acme-api"
client = "acme-client"
ui = "acme-ui"
front = "acme-front"
admin = "acme-admin"
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let workspace = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("stack"))
        .and_then(|stack| stack.services.get("workspace"))
        .expect("workspace service");

    let isolated_dirs: Vec<&str> = workspace
        .params
        .get("isolated_dirs")
        .and_then(|value| value.as_array())
        .expect("isolated_dirs")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(
        isolated_dirs,
        vec![
            "acme-api/target",
            "acme-client/node_modules",
            "acme-ui/node_modules",
            "acme-front/node_modules",
            "acme-admin/node_modules",
        ],
        "underlay bundle should expose each isolated writable dir through one shared list"
    );
}

#[test]
fn underlay_bundle_volume_dirs_default_to_app_star() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "app.test"
project_name = "underlay-app-dev"
workspace_subdir = "underlay-app"
databases = ["acme"]
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let workspace = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("stack"))
        .and_then(|stack| stack.services.get("workspace"))
        .expect("workspace service");

    let isolated_dirs: Vec<&str> = workspace
        .params
        .get("isolated_dirs")
        .and_then(|value| value.as_array())
        .expect("isolated_dirs")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(
        isolated_dirs,
        vec![
            "app-api/target",
            "app-client/node_modules",
            "app-ui/node_modules",
            "app-front/node_modules",
            "app-admin/node_modules",
        ]
    );
}

#[test]
fn underlay_bundle_uses_route_label_overrides() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acowtancy.test"
project_name = "acowtancy-dev"
workspace_subdir = "acowtancy"
databases = ["acowtancy"]

[bundle.routes]
front = "cream"
admin = "dairy"
api = "farmyard"
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let stack = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("stack"))
        .expect("stack");
    let domains = stack
        .dns
        .as_ref()
        .expect("dns")
        .routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<Vec<_>>();

    for expected in [
        "cream.acowtancy.test",
        "dairy.acowtancy.test",
        "farmyard.acowtancy.test",
    ] {
        assert!(domains.contains(&expected), "got {domains:?}");
    }
    assert!(
        !domains.contains(&"acowtancy.test"),
        "front route should move off the bare host when bundle.routes.front is set: {domains:?}"
    );
}

#[test]
fn underlay_bundle_s3_route_override_flows_from_bundle_input_without_rust_changes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    setup_underlay_path_bundle(tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/underlay" }
host = "acowtancy.test"
project_name = "acowtancy-dev"
workspace_subdir = "acowtancy"
databases = ["acowtancy"]

[bundle.routes]
s3 = "blob"
"#,
    )
    .expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("load manifest");
    let stack = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get("stack"))
        .expect("stack");
    let domains = stack
        .dns
        .as_ref()
        .expect("dns")
        .routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<Vec<_>>();

    assert!(domains.contains(&"blob.acowtancy.test"), "got {domains:?}");
    assert!(
        !domains.contains(&"s3.acowtancy.test"),
        "s3 route should honor bundle-defined label changes without rust-side route wiring: {domains:?}"
    );
}
