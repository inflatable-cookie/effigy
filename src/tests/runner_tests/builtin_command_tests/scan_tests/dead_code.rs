use super::*;

#[test]
fn run_manifest_task_builtin_scan_dead_code_reports_isolated_and_unreferenced_findings() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-findings",
        Some(
            r#"[scan.dead_code]
doctor = false
allow_paths = ["src/bin/**"]
"#,
        ),
        &["src/bin", "src/live", "src/orphan"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\npub mod orphan;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        "use crate::orphan::helper;\npub fn used() -> usize { helper() }\n",
    )
    .expect("write live");
    fs::write(
        root.join("src/orphan/mod.rs"),
        "pub fn lonely() -> usize { 1 }\npub fn helper() -> usize { 2 }\n",
    )
    .expect("write orphan");
    fs::write(
        root.join("src/bin/tool.rs"),
        "pub fn main() -> usize { 1 }\n",
    )
    .expect("write bin");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_contains_all(
        &out,
        &[
            "Dead Code",
            "isolated-file",
            "src/orphan/mod.rs",
            "unreferenced-symbol",
            "used (function)",
        ],
    );
    assert_output_excludes_all(&out, &["src/bin/tool.rs"]);
}

#[test]
fn run_manifest_task_builtin_scan_dead_code_respects_symbol_allowlist() {
    let root = setup_scan_workspace(
        "builtin-scan-dead-code-allow-symbol",
        Some(
            r#"[scan.dead_code]
doctor = false
allow_symbols = ["crate::live::lonely"]
"#,
        ),
        &["src/live"],
    );
    fs::write(root.join("src/lib.rs"), "pub mod live;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        "pub fn lonely() -> usize { 1 }\n",
    )
    .expect("write live");
    seed_graph_index(&root);

    let out = run_builtin_ok(root, "scan", &["dead-code"]);
    assert_output_excludes_all(&out, &["lonely (function)"]);
}
