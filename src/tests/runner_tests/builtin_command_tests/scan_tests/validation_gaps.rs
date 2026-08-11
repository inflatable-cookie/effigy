use super::*;

#[test]
fn run_manifest_task_builtin_scan_validation_gaps_reports_changed_owner_without_tests() {
    let root = setup_scan_workspace(
        "builtin-scan-validation-gaps-changed",
        Some(
            r#"[scan.validation_gaps]
doctor = false
hotspot_threshold = 1
"#,
        ),
        &["src/live", "src/orphan"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\npub mod orphan;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        "use crate::orphan::helper;\npub fn used() -> usize { helper() }\n",
    )
    .expect("write live");
    fs::write(
        root.join("src/orphan/mod.rs"),
        "pub fn helper() -> usize { 2 }\n",
    )
    .expect("write orphan");
    seed_graph_index(&root);

    let out = run_builtin_ok(
        root,
        "scan",
        &["validation-gaps", "--path", "src/orphan/mod.rs"],
    );
    assert_output_contains_all(
        &out,
        &[
            "Validation Gaps",
            "changed-owner-without-test-target",
            "src/orphan/mod.rs",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_validation_gaps_surfaces_likely_tests_for_changed_owner() {
    let root = setup_scan_workspace(
        "builtin-scan-validation-gaps-likely-tests",
        Some(
            r#"[scan.validation_gaps]
doctor = false
hotspot_threshold = 1

[test.suites]
rust-test = "cargo test"
"#,
        ),
        &["src", "tests"],
    );
    fs::write(root.join("src/lib.rs"), "pub fn helper() -> i32 { 1 }\n").expect("write lib");
    fs::write(
        root.join("tests/live_test.rs"),
        "use demo::helper;\n\n#[test]\nfn covers_helper() {\n    assert_eq!(helper(), 1);\n}\n",
    )
    .expect("write test");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["validation-gaps", "--path", "src/lib.rs"]);
    assert_output_contains_all(
        &out,
        &["Validation Gaps", "likely-tests:", "tests/live_test.rs"],
    );
    assert_output_excludes_all(&out, &["changed-owner-without-test-target"]);
}
