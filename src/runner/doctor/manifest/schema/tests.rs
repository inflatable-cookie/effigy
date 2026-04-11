use std::path::Path;

use toml::Value;

use super::validate_manifest_schema;
use crate::runner::doctor::report::DoctorState;

#[test]
fn validate_manifest_schema_accepts_docs_policy_bootstrap_and_release_sections() {
    let manifest: Value = toml::from_str(
        r###"
[catalog]
alias = "app"

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

[release]
version-file = "Cargo.toml"
changelog = "CHANGELOG.md"
tag-format = "v{version}"
sync-files = ["Cargo.lock"]

[release.gates]
qa = "cargo test"
smoke = { command = "printf ok", description = "smoke gate" }
"###,
    )
    .expect("parse manifest");

    let mut state = DoctorState::new();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut state);

    assert!(
        state.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        state.findings
    );
}

#[test]
fn validate_manifest_schema_accepts_current_repo_manifest() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("effigy.toml");
    let manifest_text = std::fs::read_to_string(&manifest_path).expect("read repo manifest");
    let manifest: Value = toml::from_str(&manifest_text).expect("parse repo manifest");

    let mut state = DoctorState::new();
    validate_manifest_schema(&manifest_path, &manifest, &mut state);

    assert!(
        state.findings.is_empty(),
        "expected repo manifest to validate cleanly, got: {:?}",
        state.findings
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

    let mut state = DoctorState::new();
    validate_manifest_schema(Path::new("/tmp/effigy.toml"), &manifest, &mut state);

    assert!(
        state.findings.is_empty(),
        "expected no schema findings, got: {:?}",
        state.findings
    );
}
