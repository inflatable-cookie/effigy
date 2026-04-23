//! Composition proof for the bundled Underlay starter shape.
//!
//! The starter files live under
//! `crates/effigy-catalog/starters/underlay/`. They are authored as
//! reference source for manual adoption (no starter-init CLI yet). This
//! test verifies that the composed manifest:
//!
//! - parses cleanly via `[manifest].include`
//! - resolves the shipped `underlay` bundle into the expected
//!   `systems.dev` + `containers.stack` shape
//! - declares the catalog-sourced services (workspace + postgres + dbgate +
//!   mailpit + minio)
//! - wires the managed `dev` task with the `lifecycle` + `shell` roles
//!   Effigy's dev runtime expects
//! - carries `bootstrap.setup = ["bootstrap:deps"]`

use std::path::{Path, PathBuf};

use effigy_manifest::{load_task_manifest, ManifestWorkspaceContainerRef};

fn starter_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("..")
        .join("effigy-catalog")
        .join("starters")
        .join("underlay")
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), dst_path).unwrap();
        }
    }
}

/// Materialise the starter into a temp dir and load it as a composed
/// manifest.
fn load_starter_manifest() -> (tempfile::TempDir, effigy_manifest::TaskManifest) {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir_recursive(&starter_dir(), tmp.path());
    let manifest_path = tmp.path().join("effigy.toml");
    let manifest = load_task_manifest(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to load starter manifest: {err}"));
    (tmp, manifest)
}

#[test]
fn starter_composes_into_single_manifest() {
    let (_tmp, manifest) = load_starter_manifest();

    let bundle = manifest.bundle.expect("starter root carries [bundle]");
    assert_eq!(bundle.base.as_deref(), Some("underlay"));

    // Catalog alias is declared in the root.
    let catalog = manifest.catalog.expect("starter root carries [catalog]");
    assert_eq!(catalog.alias.as_deref(), Some("underlay-app"));

    // `[bootstrap]` merged in from effigy.bootstrap.toml.
    let bootstrap = manifest
        .bootstrap
        .expect("starter declares [bootstrap] via bootstrap fragment");
    assert_eq!(bootstrap.setup, vec!["bootstrap:deps".to_string()]);
    assert_eq!(bootstrap.start.as_deref(), Some("dev"));
}

#[test]
fn starter_declares_dev_system_bound_to_stack_container() {
    let (_tmp, manifest) = load_starter_manifest();

    let systems = manifest.systems.expect("starter declares [systems]");
    assert_eq!(systems.default.as_deref(), Some("dev"));
    let dev = systems
        .systems
        .get("dev")
        .expect("starter declares systems.dev");
    assert_eq!(dev.default_workspace.as_deref(), Some("app"));
    assert_eq!(
        dev.working_dir.as_deref(),
        Some("/workspace-root/underlay-app")
    );
    assert_eq!(dev.user.as_deref(), Some("dev"));
    assert_eq!(dev.home.as_deref(), Some("/home/dev"));
    assert!(dev.mounts.is_empty(), "starter root defaults mounts to []");

    match dev
        .container
        .as_ref()
        .expect("systems.dev binds a container")
    {
        ManifestWorkspaceContainerRef::Named(name) => {
            assert_eq!(
                name, "stack",
                "systems.dev should bind the `stack` container"
            );
        }
        ManifestWorkspaceContainerRef::Inline(_) => {
            panic!("starter should use a named container reference, not inline")
        }
    }

    assert!(
        dev.workspaces.contains_key("app"),
        "systems.dev must declare the `app` workspace"
    );
}

#[test]
fn starter_generates_services_via_bundled_catalog() {
    let (_tmp, manifest) = load_starter_manifest();

    let containers = manifest.containers.expect("starter declares [containers]");
    let stack = containers
        .environments
        .get("stack")
        .expect("starter declares containers.stack");

    // No direct compose_file — the stack is fully catalog-sourced.
    assert!(
        stack.compose_file.is_none(),
        "starter must not carry a direct compose_file pointer; services should come from the catalog"
    );
    assert_eq!(stack.primary_service.as_deref(), Some("workspace"));
    assert_eq!(stack.project_name.as_deref(), Some("underlay-app-dev"));

    // All four expected services resolve to bundled fragments.
    let expected: &[(&str, &str)] = &[
        ("workspace", "workspace-rust-bun"),
        ("postgres", "postgres"),
        ("dbgate", "dbgate"),
        ("mailpit", "mailpit"),
        ("minio", "minio"),
    ];
    for (service_name, catalog) in expected {
        let service = stack.services.get(*service_name).unwrap_or_else(|| {
            panic!(
                "starter stack missing service `{service_name}`; available: {:?}",
                stack.services.keys().collect::<Vec<_>>()
            )
        });
        assert_eq!(
            service.catalog, *catalog,
            "service `{service_name}` should resolve catalog `{catalog}`"
        );
    }

    // Workspace service carries repo-owned host port bindings through
    // the fragment's `host_ports` parameter.
    let workspace = stack.services.get("workspace").unwrap();
    let host_ports = workspace
        .params
        .get("host_ports")
        .expect("workspace service declares host_ports param")
        .as_array()
        .expect("host_ports is a list");
    let port_strings: Vec<&str> = host_ports.iter().filter_map(|v| v.as_str()).collect();
    assert!(port_strings.contains(&"41001:41001"));
    assert!(port_strings.contains(&"41002:41002"));
    assert!(port_strings.contains(&"41003:41003"));

    // DNS routes publish the gateway-visible domains.
    let dns = stack
        .dns
        .as_ref()
        .expect("starter declares containers.stack.dns");
    let domains: Vec<&str> = dns.routes.iter().map(|r| r.domain.as_str()).collect();
    for expected in &[
        "app.test",
        "admin.app.test",
        "api.app.test",
        "dbgate.app.test",
        "mailpit.app.test",
        "minio.app.test",
    ] {
        assert!(
            domains.contains(expected),
            "starter DNS routes should publish `{expected}`; got {domains:?}"
        );
    }

    let published_ports: Vec<u16> = dns.routes.iter().filter_map(|route| route.port).collect();
    assert!(published_ports.contains(&41001));
    assert!(published_ports.contains(&41002));
    assert!(published_ports.contains(&41003));
}

#[test]
fn starter_managed_dev_task_wires_lifecycle_and_shell_roles() {
    let (_tmp, manifest) = load_starter_manifest();

    let dev = manifest
        .tasks
        .get("dev")
        .expect("starter declares tasks.dev");
    assert_eq!(dev.mode.as_deref(), Some("tui"));
    assert_eq!(dev.container_lifecycle, Some(true));
    assert_eq!(dev.gateway, Some(true));
    assert_eq!(dev.health_wait, Some(true));

    // Presence of the two runtime-contract roles.
    let roles: Vec<Option<&str>> = dev
        .concurrent
        .iter()
        .map(|entry| entry.role.as_deref())
        .collect();
    assert!(
        roles.contains(&Some("lifecycle")),
        "managed dev should include a lifecycle role entry; got {roles:?}"
    );
    assert!(
        roles.contains(&Some("shell")),
        "managed dev should include a shell role entry; got {roles:?}"
    );

    // The shell role must target the workspace service.
    let shell_entry = dev
        .concurrent
        .iter()
        .find(|entry| entry.role.as_deref() == Some("shell"))
        .unwrap();
    assert_eq!(shell_entry.service.as_deref(), Some("workspace"));
}

#[test]
fn starter_health_validate_qa_aggregators_exist() {
    let (_tmp, manifest) = load_starter_manifest();

    for name in &["health", "validate", "qa"] {
        assert!(
            manifest.tasks.contains_key(*name),
            "starter should declare aggregator task `{name}`"
        );
    }

    let bootstrap_deps = manifest
        .tasks
        .get("bootstrap:deps")
        .expect("starter declares bootstrap:deps task");
    assert_eq!(
        bootstrap_deps.run_in(),
        effigy_manifest::ManifestTaskRunIn::Host
    );
}
