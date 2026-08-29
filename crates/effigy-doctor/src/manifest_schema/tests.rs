use std::path::Path;

use toml::Value;

use super::validate_manifest_schema;
use crate::{DoctorFinding, DoctorSeverity, FindingSink};

#[derive(Default)]
struct TestSink {
    findings: Vec<DoctorFinding>,
}

impl FindingSink for TestSink {
    fn add_check_error(&mut self, check_id: &str, evidence: String, remediation: String) {
        self.findings.push(DoctorFinding {
            check_id: check_id.to_owned(),
            severity: DoctorSeverity::Error,
            evidence,
            remediation,
            fixable: false,
        });
    }
}

#[test]
fn validate_manifest_schema_accepts_docs_policy_bootstrap_container_distribution_and_release_sections(
) {
    let manifest: Value = toml::from_str(
        r###"
[catalog]
alias = "app"

[task_defaults]
run_in = "either"

[tasks]
qa = "cargo test"

[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
section = "Vision Artifacts"
exclude = ["history/**"]

[docs_policy.next_actions.vision]
index = "vision"
heading = "## Next Task"
allowlist_file = "docs/policy/vision-next-task-verbs.txt"

[docs_policy.graph]
roots = ["handbook", "README.md"]

[docs_policy.graph.fields.state]
labels = ["State"]
cardinality = "one"

[docs_policy.graph.currentness]
field = "state"
current = ["live"]
historical = ["retired"]

[docs_policy.graph.kinds.playbook]
include = ["handbook/playbooks/*.md"]
authority = 80
default_currentness = "unknown"

[docs_policy.graph.relations.see-also]
labels = ["See also"]
headings = ["See also"]

[bootstrap]
setup = ["bootstrap:local", "doctor"]
start = "dev"
submodules = "recursive"

[[bootstrap.children]]
path = "aura"
repo = "git@github.com:inflatable-cookie/aura.git"
branch = "main"
setup = ["install"]
required = true

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "effigy"
compose_file = "infra/dev/docker-compose.yml"
project_name = "effigy-web-dev"
primary_service = "app"
working_dir = "/workspace"

[containers.web.dns]
routes = [{ domain = "effigy.test", tls = true, port = 8080, service = "app" }]

[containers.web.aliases]
mysql = "db"

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 10

[containers.web.health]
check = "http://localhost:8080/health"
timeout_secs = 60

[containers.web.host]
ports = ["8080:80", "3306:3306"]
mounts = ["./:/workspace"]

[distribution.package]
name = "effigy"
repo-url = "https://github.com/inflatable-cookie/effigy.git"
brew-formula = "inflatable-cookie/effigy/effigy"

[distribution.publish]
binary-name = "effigy"
registry-label = "crates.io"

[distribution.preflight]
docs-task = "qa:docs"
smoke-task = "dist:preflight:smoke"

[distribution.metadata]
required-docs = ["docs/guides/010-path-installation-and-release.md"]
required-files = [".github/workflows/release-binaries.yml"]

[distribution.closeout]
owner = "release"
related = "docs/roadmaps/backlog/distribution-channels.md"
next-step = "Review evidence and publish release sign-off notes."

[release]
version-file = "Cargo.toml"
changelog = "CHANGELOG.md"
tag-format = "v{version}"
initial-tag-current-version = true
sync-files = ["Cargo.lock"]

[release.gates]
qa = "cargo test"
smoke = { command = "printf ok", description = "smoke gate" }
"###,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_unknown_docs_policy_graph_keys() {
    let manifest: Value = toml::from_str(
        r#"
[docs_policy.graph]
roots = ["handbook"]
mystery = true
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings
            .iter()
            .any(|finding| finding.evidence.contains("docs_policy.graph.mystery")),
        "expected unknown graph key to be flagged, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_current_repo_manifest() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("effigy.toml");
    let manifest_text = std::fs::read_to_string(&manifest_path).expect("read repo manifest");
    let manifest: Value = toml::from_str(&manifest_text).expect("parse repo manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(&manifest_path, &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected repo manifest to validate cleanly, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_secret_root_and_required_task_mode() {
    let manifest: Value = toml::from_str(
        r#"
[tasks.package]
run = "scripts/package.sh"
secrets = "required"

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.signing_identity]
required = true
targets = ["tasks"]
description = "Package signing identity"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected secret declarations to validate cleanly, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_unknown_task_secret_mode() {
    let manifest: Value = toml::from_str(
        r#"
[tasks.dev]
run = "cargo run"
secrets = "optional"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings
            .iter()
            .any(|finding| finding.evidence.contains("tasks.dev.secrets")),
        "expected invalid task secret mode to be flagged, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_managed_readiness_timeout() {
    let manifest: Value = toml::from_str(
        r#"
[tasks.dev]
mode = "tui"
health_wait = true
health_wait_timeout_secs = 90
concurrent = [{ name = "api", run = "cargo run" }]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected managed readiness timeout to validate, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_compact_inline_rhai_task() {
    let manifest: Value = toml::from_str(
        r#"
[tasks]
script = { rhai = "scripts/ok.rhai", run_in = "host" }
qa = [
  { task = "docs check" },
]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected compact rhai and docs sequence steps to validate, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_unknown_key_on_compact_inline_rhai_task() {
    let manifest: Value = toml::from_str(
        r#"
[tasks.script]
rhai = "scripts/ok.rhai"
mode = "tui"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings
            .iter()
            .any(|finding| finding.evidence.contains("tasks.script.mode")),
        "expected unknown compact-task key to be flagged, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_non_boolean_initial_tag_current_version() {
    let manifest: Value = toml::from_str(
        r#"
[release]
initial-tag-current-version = "yes"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.iter().any(|finding| finding
            .evidence
            .contains("release.initial_tag_current_version")),
        "expected invalid initial tag mode to be flagged, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_removed_ambient_catalog_config() {
    let manifest: Value = toml::from_str(
        r###"
[catalog]
alias = "app"

[catalog.discovery]
enabled = true
ignore = ["tests", "fixtures"]
"###,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings
            .iter()
            .any(|finding| finding.evidence.contains("catalog.discovery")),
        "expected removed catalog.discovery table to be flagged, got: {:?}",
        sink.findings
    );
    assert!(
        sink.findings
            .iter()
            .any(|finding| finding.remediation.contains("[catalog.members]")),
        "expected explicit membership remediation, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_catalog_backed_container_services() {
    let manifest: Value = toml::from_str(
        r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
extensions = ["pdo_mysql", "redis"]

[containers.web.services.web]
catalog = "nginx"
variant = "laravel"

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"
shared = true
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_legacy_container_context_field() {
    let manifest: Value = toml::from_str(
        r#"
[containers]
default = "web"

[containers.web]
context = "dev"
primary_service = "app"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings
            .iter()
            .any(|finding| finding.evidence.contains("containers.web.context")),
        "expected legacy context field finding, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_container_dns_config() {
    let manifest: Value = toml::from_str(
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "clientname.test", tls = true, port = 4173 }]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_systems_and_workspace_shortcut() {
    let manifest: Value = toml::from_str(
        r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
working_dir = "/workspace"

[tasks.dev]
workspace = "app"
run = "npm run dev"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_working_dir_for_systems_and_workspaces() {
    let manifest: Value = toml::from_str(
        r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"
working_dir = "/workspace"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
working_dir = "/workspace/app"

[tasks.dev]
workspace = "app"
run = "npm run dev"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_non_integer_container_dns_port() {
    let manifest: Value = toml::from_str(
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "clientname.test", port = "web" }]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.iter().any(|finding| finding
            .evidence
            .contains("containers.web.dns.routes[0].port")),
        "expected dns port finding, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_structured_external_host_mount() {
    let manifest: Value = toml::from_str(
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.host]
mounts = [
  "./:/workspace",
  { host = "${PERSONAL_SSH_CONFIG}",
    container = "/home/dev/.ssh/config",
    external = true,
    options = ["ro"] },
]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "structured external mount should be accepted; findings: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_catalog_members_and_structured_system_mounts() {
    let manifest: Value = toml::from_str(
        r#"
[catalog.members]
underlay = "packages/underlay"

[systems.dev]
mounts = [
  { member = "underlay", target = "/workspace/underlay" },
  { source = "../shared", catalog = true, options = ["ro"] },
  "../legacy:/workspace/legacy",
]
"#,
    )
    .expect("parse manifest");
    let mut sink = TestSink::default();

    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(sink.findings.is_empty(), "findings: {:?}", sink.findings);
}

#[test]
fn validate_manifest_schema_rejects_ambiguous_structured_system_mount() {
    let manifest: Value = toml::from_str(
        r#"
[systems.dev]
mounts = [{ member = "underlay", source = "../underlay", catalog = true }]
"#,
    )
    .expect("parse manifest");
    let mut sink = TestSink::default();

    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings
            .iter()
            .any(|finding| finding.evidence.contains("systems.dev.mounts[0]")),
        "expected structured mount finding: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_unknown_field_on_structured_host_mount() {
    let manifest: Value = toml::from_str(
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.host]
mounts = [
  { host = "./", container = "/workspace", bogus_field = "x" },
]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.iter().any(|finding| finding
            .evidence
            .contains("containers.web.host.mounts[0].bogus_field")),
        "expected unknown-field finding, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_rejects_non_boolean_external_on_host_mount() {
    let manifest: Value = toml::from_str(
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.host]
mounts = [
  { host = "./", container = "/workspace", external = "yes" },
]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.iter().any(|finding| finding
            .evidence
            .contains("containers.web.host.mounts[0].external")),
        "expected external-type finding, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_manifest_include_section() {
    let manifest: Value = toml::from_str(
        r#"
[manifest]
include = [
  "effigy.tasks.toml",
  { path = "effigy.docs.toml", override = ["docs_policy.indexes.vision"] },
]

[tasks.dev]
run = "printf dev"
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        sink.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_demo_registry_section() {
    let manifest: Value = toml::from_str(
        r#"
[demos.login-smoke]
title = "Login Smoke"
summary = "Proves local login works."
proof = "Verify the default local login journey succeeds."
owner = "auth"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
tags = ["auth", "smoke"]
receipt = "demos/receipts/login-smoke.receipt.json"
artifacts = ["demos/receipts/login-smoke.view.html"]
task = "demo:login-smoke"
prerequisites = ["api", "db"]
dependencies = ["auth/session-baseline"]
"#,
    )
    .expect("parse manifest");

    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut sink);

    assert!(
        sink.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        sink.findings
    );
}
