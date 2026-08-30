use super::*;

#[test]
fn graph_manifest_indexer_emits_effigy_domain_relations() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("scripts")).expect("mkdir scripts");
    fs::create_dir_all(temp.path().join("bundles/workspace-app")).expect("mkdir bundle");
    fs::write(
        temp.path().join("bundles/workspace-app/bundle.toml"),
        r#"
[bundle]
name = "workspace-app"
description = "Workspace app bundle."
"#,
    )
    .expect("write bundle descriptor");
    fs::write(temp.path().join("bundles/workspace-app/effigy.toml"), "")
        .expect("write bundle defaults manifest");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[manifest]
include = ["overlay.toml"]

[tasks.release]
run = [{ task = "build:api" }, { rhai = "scripts/release.rhai" }]
system = "dev"
workspace = "api"

[bundle]
base = { type = "path", dir = "bundles/workspace-app" }

[docs_policy.indexes.vision]
file = "docs/README.md"
dir = "docs"

[docs_policy.next_actions.vision]
index = "vision"
heading = "Next"
allowlist_file = "docs/allowlist.txt"

[test]
max_parallel = 4
cargo_env_match = "prefix-aware"

[test.runners.nextest]
command = "cargo nextest run"

[test.suites.qa]
run = "cargo test"
setup = [{ task = "db:up" }]
teardown = [{ rhai = "scripts/cleanup.rhai" }]

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets.vault"
identity = "ssh-agent"
unlock = "external"

[secrets.keys.deploy_token]
required = true
targets = ["deploy", "tasks"]

[state.uat]
schema = "effigy.state-stack.v1"
environment = "uat"

[[state.uat.layers]]
key = "baseline"
role = "baseline-seed"
source = "db:seed"
apply_mode = "task"
environment_policy = "all"
hook = { rhai = "state/apply-baseline.rhai", run_in = "host" }

[state.uat.captures.media]
role = "full-capture"
source_env = "legacy"
source = ".effigy/state/captures/media"
ref = "oci://ghcr.io/acme/media:{key}"
task = { rhai = "state/capture-media.rhai", run_in = "host" }
"#,
    )
    .expect("write root manifest");
    fs::write(
        temp.path().join("overlay.toml"),
        r#"
[systems.dev]
default_workspace = "api"

[systems.dev.workspaces.api]
container = "app"

[containers.app]
primary_service = "web"

[containers.app.services.web]
catalog = "php-fpm"
variant = "laravel"
config = "configs/web.conf"

[distribution.preflight]
docs-task = "docs:check"
smoke-task = "test:smoke"

[deploy.providers.render]
source = { type = "path", dir = "../external/providers/render" }

[deploy.uat]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme"
artifact_policy = "digest-preferred"

[deploy.uat.provider]
adapter = "render"
project_id = "render-project"
environment_id = "render-env"
services = { front = "front-service" }
"#,
    )
    .expect("write overlay manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert_eq!(report.counts.diagnostics, 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "overlay.toml"));

    let search = query_search(temp.path(), "release", Some(20)).expect("search");
    let task = search
        .matches
        .iter()
        .find(|entry| entry.name.as_deref() == Some("task::release"))
        .expect("release task");
    let task_node = node(temp.path(), &task.record_id).expect("task node");
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-step-task" && edge.unresolved_target.as_deref() == Some("build:api")
    }));
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-step-rhai"
            && edge.unresolved_target.as_deref() == Some("scripts/release.rhai")
    }));
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-system" && edge.unresolved_target.as_deref() == Some("dev")
    }));
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-workspace" && edge.unresolved_target.as_deref() == Some("api")
    }));

    let store = GraphStore::open(temp.path()).expect("store");
    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "includes-manifest"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "file:overlay.toml")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "bundle-base-path"
            && edge.unresolved_target.as_deref() == Some("bundles/workspace-app")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "workspace-container-ref" && edge.unresolved_target.as_deref() == Some("app")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "service-catalog" && edge.unresolved_target.as_deref() == Some("php-fpm")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "distribution-docs-task"
            && edge.unresolved_target.as_deref() == Some("docs:check")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "docs-policy-file"
            && edge.unresolved_target.as_deref() == Some("docs/README.md")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "test-runner-command"
            && edge.unresolved_target.as_deref() == Some("cargo nextest run")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "secret-key-target" && edge.unresolved_target.as_deref() == Some("deploy")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "state-layer-source" && edge.unresolved_target.as_deref() == Some("db:seed")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "state-capture-ref"
            && edge.unresolved_target.as_deref() == Some("oci://ghcr.io/acme/media:{key}")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "deploy-provider-source-path"
            && edge.unresolved_target.as_deref() == Some("../external/providers/render")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "deploy-target-provider"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str().contains("deploy:provider:render"))
    }));
}

#[test]
fn graph_manifest_indexer_emits_bootstrap_task_selector_entrypoints() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[tasks.dev]
run = "cargo run"

[bootstrap]
start = "dev"
"#,
    )
    .expect("write manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "task-selector" && symbol.canonical_name == "selector::dev"
    }));

    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "entrypoint-task"
            && edge.from_id.as_str().contains("bootstrap:start:dev")
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str().contains("task:dev"))
    }));
}

#[test]
fn graph_manifest_semantic_failures_fall_back_to_structural_indexing() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("broken.toml"),
        r#"
[tasks.release]
run = "cargo test"

[bundle]
base = { type = "path", dir = "bundles/missing-bundle" }
"#,
    )
    .expect("write manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert_eq!(report.counts.diagnostics, 1);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "broken.toml"));

    let search = query_search(temp.path(), "release", Some(10)).expect("search");
    assert!(search
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("task::release")));

    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("failed to compose manifest broken.toml")
    }));
}

#[test]
fn graph_manifest_indexer_structurally_indexes_template_rich_toml_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = fs::read_to_string(format!(
        "{}/../effigy-manifest/tests/fixtures/workspace-app-bundle/export.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read export fixture");
    fs::write(temp.path().join("export.toml"), fixture).expect("write export fixture");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "export.toml"));
}

#[test]
fn graph_manifest_indexer_structurally_indexes_templates_with_embedded_quotes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = fs::read_to_string(format!(
        "{}/../effigy-manifest/tests/fixtures/php-app-bundle/export.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read export fixture");
    fs::write(temp.path().join("export.toml"), fixture).expect("write export fixture");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "export.toml"));
}

#[test]
fn graph_manifest_indexer_skips_blank_unresolved_targets() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[manifest]
root = true

[deploy.production]
state = "production"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "smoke"
artifact_policy = "digest-preferred"

[deploy.production.provider]
adapter = "render"
project_id = ""
environment_id = ""
services = { front = "" }
"#,
    )
    .expect("write manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics.iter().all(|diagnostic| {
        !diagnostic
            .message
            .contains("edge unresolved target must not be empty")
    }));
}
