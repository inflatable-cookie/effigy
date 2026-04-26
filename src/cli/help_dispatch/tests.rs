use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{build_help_payload, build_help_payload_for_root};
use effigy_cli::HelpTopic;

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
