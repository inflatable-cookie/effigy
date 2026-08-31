use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{build_help_group_payload_for_root, build_help_payload, build_help_payload_for_root};
use effigy_cli::{HelpGroup, HelpTopic};

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-help-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

#[test]
fn build_help_payload_sets_schema_and_topic() {
    let payload = build_help_payload(HelpTopic::Doctor);
    assert_eq!(payload["schema"], "effigy.help.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["binary"]["name"], "effigy");
    assert_eq!(
        payload["binary"]["active_version"],
        effigy_core::build_info::active_version()
    );
    assert_eq!(payload["topic"], "doctor");
    assert!(payload["text"]
        .as_str()
        .is_some_and(|text| text.contains("doctor")));
}

#[test]
fn build_help_payload_for_root_hides_explicitly_deferred_builtins() {
    let root = temp_workspace("help-hidden-deferred-builtin");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"release\"]\n",
    )
    .expect("write manifest");

    let payload = build_help_payload_for_root(HelpTopic::General, &root);
    let text = payload["text"].as_str().expect("help text");
    assert!(!text.contains("effigy release"), "got: {text}");
    assert!(text.contains("effigy doctor"), "got: {text}");
}

#[test]
fn build_help_payload_for_root_keeps_release_visible_when_explicit_deferral_owns_routing() {
    let root = temp_workspace("help-explicit-does-not-hide-release");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n",
    )
    .expect("write manifest");

    let payload = build_help_payload_for_root(HelpTopic::General, &root);
    let text = payload["text"].as_str().expect("help text");
    assert!(text.contains("effigy release"), "got: {text}");
}

#[test]
fn build_help_group_payload_sets_schema_and_group_topic() {
    let root = temp_workspace("help-group-schema");
    let payload = build_help_group_payload_for_root(HelpGroup::Repo, &root);
    assert_eq!(payload["schema"], "effigy.help.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["topic"], "repo");
    let text = payload["text"].as_str().expect("help text");
    assert!(text.contains("Repo Commands"), "got: {text}");
    assert!(text.contains("effigy graph"), "got: {text}");
}

#[test]
fn build_help_group_payload_hides_explicitly_deferred_builtins() {
    let root = temp_workspace("help-group-explicit-deferral");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"graph\"]\n",
    )
    .expect("write manifest");

    let group = build_help_group_payload_for_root(HelpGroup::Repo, &root);
    let group_text = group["text"].as_str().expect("group help text");
    assert!(!group_text.contains("effigy graph"), "got: {group_text}");
    assert!(group_text.contains("effigy docs"), "got: {group_text}");

    let general = build_help_payload_for_root(HelpTopic::General, &root);
    let general_text = general["text"].as_str().expect("general help text");
    assert!(
        !general_text.contains("effigy graph"),
        "got: {general_text}"
    );
}

#[test]
fn manifest_selector_shadowing_a_builtin_hides_it_from_general_and_group_help() {
    let root = temp_workspace("help-group-shadowed-builtin");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.docs]\nrun = \"printf docs\"\n",
    )
    .expect("write manifest");

    let group = build_help_group_payload_for_root(HelpGroup::Repo, &root);
    let group_text = group["text"].as_str().expect("group help text");
    assert!(!group_text.contains("effigy docs"), "got: {group_text}");
    assert!(group_text.contains("effigy graph"), "got: {group_text}");

    let general = build_help_payload_for_root(HelpTopic::General, &root);
    let general_text = general["text"].as_str().expect("general help text");
    assert!(!general_text.contains("effigy docs"), "got: {general_text}");
}

// The direct `effigy help docs` route is refused before rendering when a
// selector shadows the built-in; that boundary is proved in
// `cli::entrypoint::help_deferral_tests`.
