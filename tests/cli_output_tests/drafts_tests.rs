//! Public/internal discovery and the one-release draft migration, through the CLI.
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn fixture(body: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fresh fixture directory");
    fs::create_dir(root.path().join("home")).expect("isolated HOME");
    fs::write(root.path().join("effigy.toml"), body).expect("fixture manifest");
    root
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(root)
        .env("HOME", root.join("home"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run CLI")
}

fn json(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON envelope")
}

const MANIFEST: &str = r#"
[tasks]
build = { run = "printf build-ok", group = "Development", visibility = "public" }
entry = [{ task = "step:helper" }]
"step:helper" = "printf step:helper-ok > helper.marker; printf step:helper-ok"
internal = { run = "printf internal-ok > internal.marker; printf internal-ok", visibility = "internal", group = "Proofs" }
[drafts.proof]
created = "2026-09-15"
expires = "2026-09-29"
purpose = "Temporary proof"
run = "printf migrated-ok > migrated.marker; printf migrated-ok"
[drafts.composed]
created = "2026-09-15"
purpose = "Temporary sequence"
run = [{ draft = "proof" }]
"#;

#[test]
fn visibility_default_is_grouped_and_all_matches_json_inventory() {
    let root = fixture(MANIFEST);
    let output = run(root.path(), &["tasks"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Development"), "{text}");
    assert!(text.contains("General"), "{text}");
    for hidden in ["internal-ok", "step:helper-ok", "migrated-ok"] {
        assert!(!text.contains(hidden), "{text}");
    }
    let payload = json(root.path(), &["tasks", "--json"]);
    let all_json = json(root.path(), &["tasks", "--all", "--json"]);
    assert_eq!(payload["result"], all_json["result"]);
    let all = run(root.path(), &["tasks", "--all"]);
    let text = String::from_utf8_lossy(&all.stdout);
    for row in payload["result"]["catalog_tasks"].as_array().expect("rows") {
        let name = row["task"].as_str().expect("task");
        assert!(text.contains(name), "missing {name}: {text}");
        assert!(row.get("visibility").is_some());
        assert!(row.get("visibility_reason").is_some());
        assert!(row.get("group").is_some());
    }
    assert!(String::from_utf8_lossy(&output.stderr).contains("one release"));
}

#[test]
fn internal_and_migrated_tasks_run_directly_and_legacy_draft_shares_identity() {
    let root = fixture(MANIFEST);
    for (task, stdout, marker) in [
        ("internal", "internal-ok", "internal.marker"),
        ("proof", "migrated-ok", "migrated.marker"),
        ("entry", "step:helper-ok", "helper.marker"),
        ("composed", "migrated-ok", "migrated.marker"),
    ] {
        let marker = root.path().join(marker);
        if marker.exists() {
            fs::remove_file(&marker).expect("reset this fixture's proof");
        }
        let output = run(root.path(), &["--json", task]);
        assert!(
            output.status.success(),
            "{task}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(stdout),
            "{task}"
        );
        assert_eq!(fs::read_to_string(marker).expect("task executed"), stdout);
    }
    let direct = json(root.path(), &["--json", "--plan", "proof"]);
    let legacy = json(root.path(), &["--json", "draft", "proof", "--plan"]);
    assert_eq!(direct["result"], legacy["result"]);
    assert!(legacy["result"].get("surface_identity").is_none());
    let status = json(root.path(), &["tasks", "status", "proof", "--json"]);
    let serialized = status["result"].to_string();
    assert!(serialized.contains("succeeded"), "{serialized}");
}

#[test]
fn completion_visibility_lists_public_only_after_migration() {
    let root = fixture(MANIFEST);
    let output = json(
        root.path(),
        &["config", "completion", "candidates", "--json"],
    );
    let candidates = output["result"]["candidates"]
        .as_array()
        .expect("candidates");
    assert!(candidates.iter().any(|name| name == "build"));
    for hidden in [
        "step:helper",
        "internal",
        "proof",
        "composed",
        "draft",
        "drafts",
    ] {
        assert!(!candidates.iter().any(|name| name == hidden), "{hidden}");
    }
}

#[test]
fn drafts_inventory_names_the_replacement() {
    let root = fixture(MANIFEST);
    let output = run(root.path(), &["drafts"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("effigy tasks --all"));
}

#[test]
fn draft_collision_is_actionable_and_unreferenced_files_stay_unloaded() {
    let root = fixture("[tasks]\nproof = 'echo public'\n[drafts.proof]\nrun = 'echo old'");
    let output = run(root.path(), &["tasks"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("both `[tasks]` and `[drafts]`"));
    fs::write(
        root.path().join("effigy.toml"),
        "[tasks]\nentry = 'echo entry'",
    )
    .expect("replace fixture");
    fs::write(
        root.path().join("unreferenced.toml"),
        "[drafts.orphan]\nrun = 'echo orphan'",
    )
    .expect("orphan fragment");
    let payload = json(root.path(), &["tasks", "--json"]);
    assert!(!payload["result"].to_string().contains("orphan"));
}
