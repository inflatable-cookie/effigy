use std::path::Path;

use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestManagedRun, ManifestManagedRunStep,
};

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
base = { type = "path", dir = "bundles/acme" }
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
    assert!(matches!(
        bundle.base.as_ref(),
        Some(effigy_manifest::ManifestBundleBase::Path { dir }) if dir == "bundles/acme"
    ));

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
fn exported_workspace_app_bundle_can_be_used_as_base_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/workspace-app");
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/workspace-app-bundle");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture bundle");

    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "workspace-app-reference"
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
fn exported_workspace_app_bundle_honors_name_overrides() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/workspace-app");
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/workspace-app-bundle");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture bundle");

    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "workspace-app-reference"
databases = ["acme"]
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
fn exported_php_app_bundle_can_be_used_as_base_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/php-app");
    let fixture_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/php-app-bundle");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture bundle");

    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"[bundle]
base = { type = "path", dir = "bundles/php-app" }
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
    assert_eq!(web.working_dir.as_deref(), Some("/var/www/legacy"));
}

#[test]
fn exported_php_app_library_bundle_can_be_used_as_base_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = tmp.path().join("bundles/php-library");
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/php-library-bundle");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture bundle");
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
base = { type = "path", dir = "../../bundles/php-library" }
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
base = { type = "path", dir = "bundle" }
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
fn bundle_base_path_is_rejected_with_migration_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"
[bundle]
base_path = "bundle"
"#,
    )
    .expect("write manifest");

    let error =
        load_task_manifest_with_inspection(&tmp.path().join("effigy.toml")).expect_err("fail");
    assert!(
        error
            .to_string()
            .contains("`[bundle].base_path` has been removed"),
        "{error}"
    );
}

/// A bundle whose defaults carry step-shaped tables: a vault generator and a
/// task whose `run` names a Rhai executor plus step metadata.
fn step_override_bundle(dir: &Path) {
    let bundle_dir = dir.join("bundles/steps");
    std::fs::create_dir_all(&bundle_dir).expect("mkdir bundle");
    std::fs::write(
        bundle_dir.join("bundle.toml"),
        r#"
[bundle]
name = "steps"
"#,
    )
    .expect("write descriptor");
    std::fs::write(
        bundle_dir.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
generate = { rhai = "generate-dev-secrets.rhai", run_in = "host" }

[tasks.seed]
run = { rhai = "seed.rhai", timeout_ms = 1200, env = { SEED_PHASE = "default" } }
"#,
    )
    .expect("write defaults");
}

fn load_step_override_manifest(manifest: &str) -> effigy_manifest::TaskManifest {
    let tmp = tempfile::tempdir().expect("tempdir");
    step_override_bundle(tmp.path());
    std::fs::write(tmp.path().join("effigy.toml"), manifest).expect("write manifest");
    std::fs::create_dir(tmp.path().join(".git")).expect("git dir");
    load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect("load manifest")
        .manifest
}

fn first_step(manifest: &effigy_manifest::TaskManifest) -> &ManifestManagedRunStep {
    let task = manifest.tasks.get("seed").expect("seed task");
    let Some(ManifestManagedRun::Sequence(steps)) = task.run.as_ref() else {
        panic!("seed should use a managed sequence");
    };
    steps.first().expect("seed step")
}

#[test]
fn bundle_step_override_replaces_the_default_executor_group() {
    let manifest = load_step_override_manifest(
        r#"
[bundle]
base = { type = "path", dir = "bundles/steps" }

[secrets.vault]
generate = { task = "secrets generate" }

[tasks.seed]
run = { task = "db seed" }
"#,
    );

    let generate = manifest
        .secrets
        .as_ref()
        .expect("secrets")
        .vault
        .as_ref()
        .expect("vault")
        .generate
        .as_ref()
        .expect("generate");
    assert_eq!(generate.task.as_deref(), Some("secrets generate"));
    assert_eq!(
        generate.rhai, None,
        "a task override must not keep the bundle's rhai executor"
    );
    assert_eq!(
        generate.run_in,
        Some(effigy_manifest::ManifestTaskRunIn::Host),
        "unset step metadata still defaults from the bundle"
    );

    let ManifestManagedRunStep::Step(step) = first_step(&manifest) else {
        panic!("seed step should be a step table");
    };
    assert_eq!(step.task.as_deref(), Some("db seed"));
    assert_eq!(
        step.rhai, None,
        "a task override must not keep the bundle's rhai executor"
    );
    assert_eq!(
        step.timeout_ms,
        Some(1200),
        "unset step metadata still defaults from the bundle"
    );
    assert!(
        step.env.is_some(),
        "unset step metadata still defaults from the bundle"
    );
}

#[test]
fn bundle_step_override_keeps_explicit_metadata_over_bundle_defaults() {
    let manifest = load_step_override_manifest(
        r#"
[bundle]
base = { type = "path", dir = "bundles/steps" }

[tasks.seed]
run = { task = "db seed", timeout_ms = 50 }
"#,
    );

    let ManifestManagedRunStep::Step(step) = first_step(&manifest) else {
        panic!("seed step should be a step table");
    };
    assert_eq!(step.task.as_deref(), Some("db seed"));
    assert_eq!(step.timeout_ms, Some(50));
}

#[test]
fn bundle_step_override_never_rewrites_a_malformed_manifest_step() {
    // A manifest step that names two executors keeps both: composition must not
    // silently repair a step the exactly-one-executor validation has to refuse.
    let manifest = load_step_override_manifest(
        r#"
[bundle]
base = { type = "path", dir = "bundles/steps" }

[tasks.seed]
run = { task = "db seed", rhai = "seed.rhai" }
"#,
    );

    let ManifestManagedRunStep::Step(step) = first_step(&manifest) else {
        panic!("seed step should be a step table");
    };
    assert_eq!(step.task.as_deref(), Some("db seed"));
    assert_eq!(step.rhai.as_deref(), Some("seed.rhai"));
}

#[test]
fn bundle_step_executor_overrides_cover_every_kind() {
    for default_kind in ["run", "task", "draft", "rhai"] {
        for override_kind in ["run", "task", "rhai"] {
            let tmp = tempfile::tempdir().expect("tempdir");
            step_override_bundle(tmp.path());
            let defaults = format!(
                "[tasks.seed]\nrun = {{ {default_kind} = 'default', timeout_ms = 1200 }}\n"
            );
            std::fs::write(tmp.path().join("bundles/steps/effigy.toml"), defaults)
                .expect("write defaults");
            let consumer = format!(
                "[bundle]\nbase = {{ type = 'path', dir = 'bundles/steps' }}\n\
                 [tasks.seed]\nrun = {{ {override_kind} = 'consumer', id = 'chosen' }}\n"
            );
            std::fs::write(tmp.path().join("effigy.toml"), consumer).expect("write manifest");
            let manifest = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
                .expect("load overridden manifest")
                .manifest;
            let ManifestManagedRunStep::Step(step) = first_step(&manifest) else {
                panic!("seed step should be a step table");
            };
            for (kind, value) in [
                ("run", step.run.as_deref()),
                ("task", step.task.as_deref()),
                ("rhai", step.rhai.as_deref()),
            ] {
                assert_eq!(
                    value,
                    (kind == override_kind).then_some("consumer"),
                    "default {default_kind}, override {override_kind}, field {kind}"
                );
            }
            assert_eq!(step.timeout_ms, Some(1200));
            assert_eq!(step.id.as_deref(), Some("chosen"));
        }
    }
}

#[test]
fn bundle_step_override_preserves_retired_draft_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    step_override_bundle(tmp.path());
    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/steps" }
[tasks.seed]
run = { draft = "old" }
"#,
    )
    .expect("write manifest");
    let error = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect_err("draft must remain invalid");
    assert!(
        error
            .to_string()
            .contains("`draft` step reference was removed"),
        "{error}"
    );
}

#[test]
fn bundle_executor_names_in_task_and_env_maps_merge_as_defaults() {
    let tmp = tempfile::tempdir().expect("tempdir");
    step_override_bundle(tmp.path());
    std::fs::write(
        tmp.path().join("bundles/steps/effigy.toml"),
        r#"
[tasks]
run = "default command"
[tasks.seed]
run = { rhai = "default.rhai", env = { run = "default", shared = "default" } }
"#,
    )
    .expect("write defaults");
    std::fs::write(
        tmp.path().join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/steps" }
[tasks]
draft = "consumer command"
[tasks.seed]
run = { task = "consumer", env = { task = "consumer", shared = "consumer" } }
"#,
    )
    .expect("write manifest");
    let manifest = load_task_manifest_with_inspection(&tmp.path().join("effigy.toml"))
        .expect("load manifest")
        .manifest;
    assert!(manifest.tasks.contains_key("run"));
    assert!(manifest.tasks.contains_key("draft"));
    let ManifestManagedRunStep::Step(step) = first_step(&manifest) else {
        panic!("seed step should be a step table");
    };
    let Some(effigy_manifest::ManifestRunStepEnv::Inline(env)) = &step.env else {
        panic!("seed should have inline environment");
    };
    assert_eq!(env.get("run").map(String::as_str), Some("default"));
    assert_eq!(env.get("task").map(String::as_str), Some("consumer"));
    assert_eq!(env.get("shared").map(String::as_str), Some("consumer"));
}
