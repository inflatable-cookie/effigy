use std::path::Path;

use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestManagedRun, ManifestManagedRunStep,
};

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
run = { draft = 'old' }
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
draft = 'consumer command'
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
