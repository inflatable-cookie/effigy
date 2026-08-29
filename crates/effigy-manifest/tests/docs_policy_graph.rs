use std::fs;
use std::path::Path;

use effigy_manifest::{
    load_docs_policy_graph_config, load_task_manifest, ManifestDocsPolicyGraphCardinality,
    ManifestDocsPolicyGraphCurrentnessClass, TASK_MANIFEST_FILE,
};

fn write_manifest(dir: &Path, body: &str) {
    fs::write(dir.join(TASK_MANIFEST_FILE), body).expect("write manifest");
}

fn load_err(dir: &Path) -> String {
    load_task_manifest(&dir.join(TASK_MANIFEST_FILE))
        .expect_err("expected graph validation failure")
        .to_string()
}

#[test]
fn missing_graph_selects_baseline_without_error() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
"#,
    );

    let manifest = load_task_manifest(&temp.path().join(TASK_MANIFEST_FILE)).expect("load");
    let policy = manifest.docs_policy.expect("docs policy");
    assert!(policy.graph.is_none());
    assert_eq!(
        load_docs_policy_graph_config(&temp.path().join(TASK_MANIFEST_FILE)).expect("graph"),
        None
    );
}

#[test]
fn arbitrary_field_kind_and_relation_tokens_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook", "README.md"]

[docs_policy.graph.fields.state]
labels = ["State"]
cardinality = "one"

[docs_policy.graph.fields.steward]
labels = ["Steward", "Owner-of-record"]
cardinality = "many"

[docs_policy.graph.currentness]
field = "state"
current = ["live", "Ready"]
historical = ["retired"]

[docs_policy.graph.kinds.playbook]
include = ["handbook/playbooks/*.md"]
authority = 80

[docs_policy.graph.kinds.bulletin]
include = ["handbook/bulletins/*.md"]
exclude = ["handbook/bulletins/drafts/*.md"]
authority = 20
default_currentness = "historical"

[docs_policy.graph.relations.see-also]
labels = ["See also"]
headings = ["See also"]
"#,
    );

    let manifest = load_task_manifest(&temp.path().join(TASK_MANIFEST_FILE)).expect("load");
    let graph = manifest
        .docs_policy
        .expect("docs policy")
        .graph
        .expect("graph");
    assert_eq!(graph.roots, ["handbook", "README.md"]);
    let state = graph.fields.get("state").expect("state field");
    assert_eq!(state.labels, ["State"]);
    assert_eq!(state.cardinality, ManifestDocsPolicyGraphCardinality::One);
    let steward = graph.fields.get("steward").expect("steward field");
    assert_eq!(
        steward.cardinality,
        ManifestDocsPolicyGraphCardinality::Many
    );
    assert_eq!(
        graph
            .kinds
            .get("bulletin")
            .expect("bulletin")
            .default_currentness,
        ManifestDocsPolicyGraphCurrentnessClass::Historical
    );
    assert!(graph.relations.contains_key("see-also"));
    let loaded = load_docs_policy_graph_config(&temp.path().join(TASK_MANIFEST_FILE))
        .expect("graph config")
        .expect("present");
    assert_eq!(loaded, graph);
}

#[test]
fn snake_case_aliases_parse_for_graph_fields() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.kinds.playbook]
include = ["handbook/*.md"]
default_currentness = "current"
"#,
    );

    let graph = load_docs_policy_graph_config(&temp.path().join(TASK_MANIFEST_FILE))
        .expect("graph config")
        .expect("present");
    assert_eq!(
        graph
            .kinds
            .get("playbook")
            .expect("playbook")
            .default_currentness,
        ManifestDocsPolicyGraphCurrentnessClass::Current
    );
}

#[test]
fn empty_roots_fail_deterministically() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = []
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("docs_policy.graph.roots") && error.contains("at least one"),
        "{error}"
    );
}

#[test]
fn escaped_root_fails_deterministically() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["../outside"]
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("docs_policy.graph.roots[0]") && error.contains("escapes"),
        "{error}"
    );
}

#[test]
fn absolute_root_fails_deterministically() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["/tmp/handbook"]
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("docs_policy.graph.roots[0]") && error.contains("absolute"),
        "{error}"
    );
}

#[test]
fn unknown_graph_key_fails_parse() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]
mystery = true
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("unknown field") && error.contains("mystery"),
        "{error}"
    );
}

#[test]
fn currentness_must_name_a_declared_field() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.currentness]
field = "state"
current = ["live"]
historical = ["retired"]
"#,
    );
    let error = load_err(temp.path());
    assert!(error.contains("undeclared field `state`"), "{error}");
}

#[test]
fn overlapping_currentness_values_fail() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.fields.state]
labels = ["State"]

[docs_policy.graph.currentness]
field = "state"
current = ["Live"]
historical = ["live"]
"#,
    );
    let error = load_err(temp.path());
    assert!(error.contains("both `current` and `historical`"), "{error}");
}

#[test]
fn kind_without_include_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.kinds.playbook]
include = []
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("docs_policy.graph.kinds.playbook.include")
            && error.contains("at least one glob"),
        "{error}"
    );
}

#[test]
fn authority_out_of_bounds_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.kinds.playbook]
include = ["handbook/*.md"]
authority = 101
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("authority") && error.contains("0 through 100"),
        "{error}"
    );
}

#[test]
fn relation_requires_a_selector() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.relations.see-also]
labels = []
headings = []
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("docs_policy.graph.relations.see-also")
            && error.contains("at least one label or heading"),
        "{error}"
    );
}

#[test]
fn field_requires_a_label() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_manifest(
        temp.path(),
        r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.fields.state]
labels = []
"#,
    );
    let error = load_err(temp.path());
    assert!(
        error.contains("docs_policy.graph.fields.state.labels") && error.contains("at least one"),
        "{error}"
    );
}

#[test]
fn cardinality_defaults_to_one() {
    let parsed: effigy_manifest::ManifestDocsPolicyGraphFieldConfig = toml::from_str(
        r#"
labels = ["State"]
"#,
    )
    .expect("parse field");
    assert_eq!(parsed.cardinality, ManifestDocsPolicyGraphCardinality::One);
}
