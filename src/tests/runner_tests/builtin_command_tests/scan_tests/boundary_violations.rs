use super::*;

#[test]
fn run_manifest_task_builtin_scan_boundary_violations_reports_disallowed_edges() {
    let root = temp_workspace("builtin-scan-boundary-violations");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[scan.boundary_violations]
doctor = false

[scan.boundary_violations.layers.app]
paths = ["src/app/**"]
may_depend_on = ["domain", "shared"]

[scan.boundary_violations.layers.domain]
paths = ["src/domain/**"]
may_depend_on = ["shared"]

[scan.boundary_violations.layers.shared]
paths = ["src/shared/**"]
"#,
    );
    fs::create_dir_all(root.join("src/app")).expect("mkdir app");
    fs::create_dir_all(root.join("src/domain")).expect("mkdir domain");
    fs::create_dir_all(root.join("src/shared")).expect("mkdir shared");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod app;\npub mod domain;\npub mod shared;\n",
    )
    .expect("write lib");
    fs::write(root.join("src/shared/mod.rs"), "pub fn shared() {}\n").expect("write shared");
    fs::write(root.join("src/app/mod.rs"), "pub fn page() {}\n").expect("write app");
    fs::write(
        root.join("src/domain/mod.rs"),
        "use crate::app::page;\nuse crate::shared::shared;\npub fn service() { page(); shared(); }\n",
    )
    .expect("write domain");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["boundary-violations"]);

    assert_output_contains_all(
        &out,
        &[
            "Boundary Violations",
            "configured-layers: 3",
            "checked-edges:",
            "high  domain -> app",
            "src/domain/mod.rs",
            "src/app/mod.rs",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_boundary_violations_without_rules_is_clean() {
    let root = temp_workspace("builtin-scan-boundary-violations-no-rules");
    write_root_manifest(&root, "");

    let out = run_builtin_ok(root, "scan", &["boundary-violations"]);

    assert_text_scan_is_clean(
        &out,
        "Boundary Violations",
        "configured-layers: 0",
        &["Findings"],
    );
    assert_output_contains_all(&out, &["No boundary rules configured."]);
}
