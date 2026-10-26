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

/// One manifest the schema check must accept.
struct AcceptCase {
    name: &'static str,
    manifest: &'static str,
}

/// One manifest the schema check must reject, with the key paths its findings
/// must name, any remediation text they must carry, and the exact finding count
/// when the case asserts one.
struct RejectCase {
    name: &'static str,
    manifest: &'static str,
    evidence: &'static [&'static str],
    remediation: &'static [&'static str],
    findings: Option<usize>,
}

fn validate(manifest: &'static str) -> TestSink {
    let parsed: Value = toml::from_str(manifest).expect("parse manifest fixture");
    let mut sink = TestSink::default();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &parsed, &mut sink);
    sink
}

/// Every shape the schema accepts validates with no findings.
#[test]
fn manifest_schema_accepts_every_documented_shape() {
    for case in ACCEPT_CASES {
        let sink = validate(case.manifest);
        assert!(
            sink.findings.is_empty(),
            "{}: expected no schema findings, got {:?}",
            case.name,
            sink.findings
        );
    }
}

/// Every rejected shape reports a finding that names the offending key path.
#[test]
fn manifest_schema_rejects_each_bad_key_with_its_path() {
    for case in REJECT_CASES {
        let sink = validate(case.manifest);
        for expected in case.evidence {
            assert!(
                sink.findings
                    .iter()
                    .any(|finding| finding.evidence.contains(expected)),
                "{}: expected a finding naming `{expected}`, got {:?}",
                case.name,
                sink.findings
            );
        }
        for expected in case.remediation {
            assert!(
                sink.findings
                    .iter()
                    .any(|finding| finding.remediation.contains(expected)),
                "{}: expected remediation naming `{expected}`, got {:?}",
                case.name,
                sink.findings
            );
        }
        if let Some(expected) = case.findings {
            assert_eq!(
                sink.findings.len(),
                expected,
                "{}: expected {expected} finding(s), got {:?}",
                case.name,
                sink.findings
            );
        }
    }
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

static ACCEPT_CASES: &[AcceptCase] = &[
    AcceptCase {
        name: "docs-policy-bootstrap-container-release",
        manifest: r###"
[catalog]
alias = "app"

[task_defaults]
run_in = "either"

[tasks]
qa = "cargo test"

[docs_policy.indexes.topic]
file = "docs/knowledge/README.md"
dir = "docs/knowledge"
section = "Vision Artifacts"
exclude = ["history/**"]

[docs_policy.next_actions.topic]
index = "vision"
heading = "## Next move"
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
submodules = true

[sources.aura]
git = "git@github.com:inflatable-cookie/aura.git"
rev = "0123456789abcdef0123456789abcdef01234567"
path = "../aura"

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

[release]
version-file = "Cargo.toml"
changelog = "CHANGELOG.md"
tag-format = "v{version}"
initial-tag-current-version = true
sync-files = ["Cargo.lock"]
approve = ["publish"]

[release.hosted-evidence]
workflow = "ci.yml"

[release.gates]
qa = "cargo test"
smoke = { command = "printf ok", description = "smoke gate", reuse-hosted-evidence = true }

[release.identities.core]
version-file = "packages/core/package.json"
tag-format = "core-v{version}"

[release.artifacts.core]
task = "release:pack:core"
path = "dist/core-*.tgz"

[release.targets.npm]
kind = "task"
task = "release:publish:npm"
check = "release:check:npm"
verify = "release:verify:npm"
"###,
    },
    AcceptCase {
        name: "supported-test-suite-run-forms",
        manifest: r#"
[test.suites]
legacy = "cargo test"

[test.suites.integration]
default = false
run = [
  { task = "db:test:prepare" },
  "cargo nextest run --workspace",
]

[test.suites.single]
run = { run = "cargo test -p app" }
"#,
    },
    AcceptCase {
        name: "docs-policy-sources-membership",
        manifest: r#"
[docs_policy.sources]
share = true
front_doors = ["docs/README.md"]
skill_roots = [".agents/skills"]
"#,
    },
    AcceptCase {
        name: "secret-root-and-required-task-mode",
        manifest: r#"
[tasks.package]
run = "scripts/package.sh"
secrets = "required"

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.signing_identity]
required = true
targets = ["tasks"]
description = "Package signing identity"
"#,
    },
    AcceptCase {
        name: "managed-readiness-timeout",
        manifest: r#"
[tasks.dev]
mode = "tui"
health_wait = true
health_wait_timeout_secs = 90
concurrent = [{ name = "api", run = "cargo run" }]
"#,
    },
    AcceptCase {
        name: "task-scratch-opt-out",
        manifest: r#"
[tasks.probe]
run = "cargo test"
scratch = false

[tasks.build]
run = "cargo build"
scratch = true
"#,
    },
    AcceptCase {
        name: "compact-inline-rhai-task",
        manifest: r#"
[tasks]
script = { rhai = "scripts/ok.rhai", run_in = "host" }
qa = [
  { task = "docs check" },
]
"#,
    },
    AcceptCase {
        name: "catalog-backed-container-services",
        manifest: r#"
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
    },
    AcceptCase {
        name: "container-data-from",
        manifest: r#"
[containers]
default = "hybrid"

[containers.dev]
primary_service = "app"

[containers.hybrid]
primary_service = "app"
data_from = "dev"
"#,
    },
    AcceptCase {
        name: "container-dns-config",
        manifest: r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
base_domain = "acme.test"
routes = [{ domain = "clientname.test", tls = true, port = 4173 }]
"#,
    },
    AcceptCase {
        name: "systems-and-workspace-shortcut",
        manifest: r#"
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
    },
    AcceptCase {
        name: "working-dir-for-systems-and-workspaces",
        manifest: r#"
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
    },
    AcceptCase {
        name: "structured-external-host-mount",
        manifest: r#"
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
    },
    AcceptCase {
        name: "catalog-members-and-structured-system-mounts",
        manifest: r#"
[catalog.members]
underlay = "packages/underlay"

[systems.dev]
mounts = [
  { member = "underlay", target = "/workspace/underlay" },
  { source = "../shared", catalog = true, options = ["ro"] },
  "../legacy:/workspace/legacy",
]
"#,
    },
    AcceptCase {
        name: "manifest-include-section",
        manifest: r#"
[manifest]
include = [
  "effigy.tasks.toml",
  { path = "effigy.docs.toml", override = ["docs_policy.indexes.topic"] },
]

[tasks.dev]
run = "printf dev"
"#,
    },
    AcceptCase {
        name: "bootstrap-one-release-spellings",
        manifest: r#"
[bootstrap]
run = [
  { task = "secrets init" },
  { rhai = "scripts/bootstrap.rhai" },
]
start = [{ task = "container:up" }, { task = "dev" }]
submodules = "recursive"

[[bootstrap.children]]
path = "../book"
repo = "git@example.test:book.git"
"#,
    },
    AcceptCase {
        name: "contracts-section",
        manifest: r#"
[contracts]
schema_index = "docs/knowledge/contracts/json-schema-index.json"
selection_contract = "docs/knowledge/contracts/json-selection-contract.json"
"#,
    },
    AcceptCase {
        name: "task-description-on-every-shape",
        manifest: r#"
[tasks.dev]
run = "cargo run"
description = "Run the dev stack"

[tasks.capture]
rhai = "scripts/capture.rhai"
description = "Capture fixture output"

[tasks.proof]
task = "dev"
description = "Alias for the dev stack"
"#,
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-0",
        manifest: "[shell]\nrun = \"exec zsh -i\"",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-1",
        manifest: "[demos.checkout]\ntitle = \"Checkout\"\nrun = \"bun run demo\"",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-2",
        manifest: "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-3",
        manifest: "[scan.attention_markers]\nwarn = 1",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-4",
        manifest: "[scan.comment_ratio]\nwarn = 1",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-5",
        manifest: "[scan.generated_assets]\nwarn = 1",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-6",
        manifest: "[scan.generated_in_src]\nwarn = 1",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-7",
        manifest: "[scan.stale_suppressions]\nwarn = 1",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-8",
        manifest: "[scan.boundary_violations]\nwarn = 1",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-9",
        manifest: "[scan.dead_code]\nwarn = 1",
    },
    // A one-release retired section that changes nothing else: doctor must not flag
    // it, and the manifest's own deprecation notice names it on stderr.
    AcceptCase {
        name: "retired-section-10",
        manifest: "[scan.validation_gaps]\nwarn = 1",
    },
    // `durable` is a boolean on every task shape.
    AcceptCase {
        name: "task-durable-true",
        manifest: "[tasks.probe]\nrun = \"true\"\ndurable = true\n",
    },
    AcceptCase {
        name: "task-durable-false",
        manifest: "[tasks.probe]\nrun = \"true\"\ndurable = false\n",
    },
];

static REJECT_CASES: &[RejectCase] = &[
    RejectCase {
        name: "unknown-docs-policy-graph-key",
        manifest: r#"
[docs_policy.graph]
roots = ["handbook"]
mystery = true
"#,
        evidence: &["docs_policy.graph.mystery"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "retired-catalog-pack-support-policy",
        manifest: r#"
[release]
version-file = "Cargo.toml"
sync-catalog-pack-support-policy = "support/catalog-pack-update.toml"
"#,
        evidence: &["release.sync-catalog-pack-support-policy"],
        remediation: &["catalog-pack runtime"],
        findings: None,
    },
    RejectCase {
        name: "malformed-test-suite-runs",
        manifest: r#"
[test.suites.empty_command]
run = "  "

[test.suites.empty_array]
run = []

[test.suites.missing_run]
default = false

[test.suites.bad_step]
run = ["printf ok", 42]

[test.suites.missing_route]
run = [{}]

[test.suites.multiple_routes]
run = [{ run = "true", task = "validate" }]

[test.suites.empty_route]
run = [{ task = "" }]
"#,
        evidence: &[
            "test.suites.empty_command.run",
            "test.suites.empty_array.run",
            "test.suites.missing_run.run",
            "test.suites.bad_step.run[1]",
            "test.suites.missing_route.run[0]",
            "test.suites.multiple_routes.run[0]",
            "test.suites.empty_route.run[0].task",
        ],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "docs-policy-sources-typo",
        manifest: r#"
[docs_policy.sources]
share = "yes"
front_door = ["docs/README.md"]
"#,
        evidence: &[],
        remediation: &[],
        findings: Some(2),
    },
    RejectCase {
        name: "unknown-task-secret-mode",
        manifest: r#"
[tasks.dev]
run = "cargo run"
secrets = "optional"
"#,
        evidence: &["tasks.dev.secrets"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "non-boolean-task-scratch",
        manifest: r#"
[tasks.probe]
run = "cargo test"
scratch = "never"
"#,
        evidence: &["tasks.probe.scratch"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "unknown-compact-inline-rhai-key",
        manifest: r#"
[tasks.script]
rhai = "scripts/ok.rhai"
mode = "tui"
"#,
        evidence: &["tasks.script.mode"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "unknown-release-identity-and-target-values",
        manifest: r#"
[release.identities.core]
kind = "calendar"

[release.targets.npm]
kind = "npm"

[release.gates.qa]
task = "qa"
stage = "afterwards"
"#,
        evidence: &[
            "release.identities.core.kind",
            "release.targets.npm.kind",
            "release.gates.qa.stage",
        ],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "non-boolean-initial-tag-current-version",
        manifest: r#"
[release]
initial-tag-current-version = "yes"
"#,
        evidence: &["release.initial_tag_current_version"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "removed-ambient-catalog-config",
        manifest: r###"
[catalog]
alias = "app"

[catalog.discovery]
enabled = true
ignore = ["tests", "fixtures"]
"###,
        evidence: &["catalog.discovery"],
        remediation: &["[catalog.members]"],
        findings: None,
    },
    RejectCase {
        name: "container-data-from-type",
        manifest: r#"
[containers]
default = "hybrid"

[containers.hybrid]
primary_service = "app"
data_from = 7
"#,
        evidence: &["containers.hybrid.data_from"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "legacy-container-context-field",
        manifest: r#"
[containers]
default = "web"

[containers.web]
context = "dev"
primary_service = "app"
"#,
        evidence: &["containers.web.context"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "non-integer-dns-port",
        manifest: r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "clientname.test", port = "web" }]
"#,
        evidence: &["containers.web.dns.routes[0].port"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "non-string-dns-base-domain",
        manifest: r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
base_domain = 42
"#,
        evidence: &["containers.web.dns.base_domain"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "ambiguous-structured-system-mount",
        manifest: r#"
[systems.dev]
mounts = [{ member = "underlay", source = "../underlay", catalog = true }]
"#,
        evidence: &["systems.dev.mounts[0]"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "unknown-structured-host-mount-field",
        manifest: r#"
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
        evidence: &["containers.web.host.mounts[0].bogus_field"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "non-boolean-host-mount-external",
        manifest: r#"
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
        evidence: &["containers.web.host.mounts[0].external"],
        remediation: &[],
        findings: None,
    },
    RejectCase {
        name: "unknown-contracts-keys",
        manifest: "[contracts]\nschema_index = \"index.json\"\nartifact = \"selected.json\"\n",
        evidence: &["contracts.artifact"],
        remediation: &[],
        findings: None,
    },
    // The retired draft namespace is named in the remediation, in either spelling.
    RejectCase {
        name: "retired-draft-table",
        manifest: "[drafts.proof]\nrun = \"echo old\"",
        evidence: &[],
        remediation: &["draft namespace was removed in v0.15"],
        findings: None,
    },
    RejectCase {
        name: "retired-draft-step",
        manifest: "[tasks]\nentry = [{ draft = \"proof\" }]",
        evidence: &[],
        remediation: &["draft namespace was removed in v0.15"],
        findings: None,
    },
    RejectCase {
        name: "non-boolean-task-durable",
        manifest: "[tasks.probe]\nrun = \"true\"\ndurable = \"yes\"",
        evidence: &["tasks.probe.durable"],
        remediation: &[],
        findings: None,
    },
];
