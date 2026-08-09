use super::support::{parse_stdout_json, temp_workspace};
use std::fs;
use std::process::Command;

fn queue(title: &str) -> String {
    format!(
        "# Papercuts\n\n## Open\n\n### [ ] {title} — 2026-08-09\n- Friction: slow\n- Impact: repeat\n- Possible fix: improve\n- Surface: test\n"
    )
}

#[test]
fn papercuts_json_discovers_sibling_projects_without_a_collection_manifest() {
    let root = temp_workspace("papercuts-collection");
    for project in ["alpha", "beta"] {
        let project = root.join(project);
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("effigy.toml"), "[tasks]\n").unwrap();
        fs::write(
            project.join("PAPERCUTS.md"),
            queue(project.file_name().unwrap().to_str().unwrap()),
        )
        .unwrap();
    }
    let nested_template = root.join("alpha/skills/template");
    fs::create_dir_all(&nested_template).unwrap();
    fs::write(nested_template.join("PAPERCUTS.md"), queue("template")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["--json", "papercuts"])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["schema"], "effigy.command.v1");
    assert_eq!(payload["result"]["schema"], "effigy.papercuts.v1");
    assert_eq!(payload["result"]["summary"]["projects_scanned"], 2);
    assert_eq!(payload["result"]["entries"].as_array().unwrap().len(), 2);
}

#[test]
fn papercuts_add_creates_a_queue_and_returns_normalized_json() {
    let root = temp_workspace("papercuts-add");
    fs::write(root.join("effigy.toml"), "[tasks]\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args([
            "--json",
            "papercuts",
            "add",
            "Noisy graph",
            "--friction",
            "large stale output",
            "--impact",
            "repeat orientation",
            "--fix",
            "refresh once",
            "--surface",
            "Effigy graph",
        ])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["schema"], "effigy.papercuts.add.v1");
    assert_eq!(payload["result"]["entry"]["title"], "Noisy graph");
    let written = fs::read_to_string(root.join("PAPERCUTS.md")).unwrap();
    assert!(written.contains("### [ ] Noisy graph —"));
    assert!(written.contains("- Surface: Effigy graph"));
}
