use std::collections::BTreeSet;
use std::path::PathBuf;

use effigy_manifest::{LoadedCatalog, TaskManifest};

use super::heavy_health_paths;

fn catalog(manifest_body: &str) -> LoadedCatalog {
    let root = PathBuf::from("/workspace");
    LoadedCatalog {
        alias: "root".to_owned(),
        catalog_root: root.clone(),
        manifest_path: root.join("effigy.toml"),
        bundle_root: None,
        manifest: toml::from_str::<TaskManifest>(manifest_body).expect("parse manifest"),
        defer_run: None,
        deferred_builtins: BTreeSet::new(),
        depth: 0,
    }
}

#[test]
fn finds_direct_and_transitive_qa_references() {
    let direct = catalog(
        r#"
        [tasks]
        health = [{ task = "qa" }]
        qa = "printf qa"
        "#,
    );
    assert_eq!(heavy_health_paths(&[direct]), vec!["root/health -> qa"]);

    let transitive = catalog(
        r#"
        [tasks]
        health = [{ task = "baseline" }]
        baseline = [{ task = "validate" }]
        validate = [{ task = "qa" }]
        qa = "printf qa"
        "#,
    );
    assert_eq!(
        heavy_health_paths(&[transitive]),
        vec!["root/health -> baseline -> validate -> qa"]
    );
}

#[test]
fn finds_builtin_and_command_full_test_suites() {
    let manifest = catalog(
        r#"
        [tasks]
        health = [{ task = "test" }, { task = "checks" }]
        checks = ["cargo nextest run --workspace", "effigy qa"]
        "#,
    );

    assert_eq!(
        heavy_health_paths(&[manifest]),
        vec![
            "root/health -> checks -> full test command `cargo nextest run --workspace`",
            "root/health -> checks -> full test command `effigy qa`",
            "root/health -> test",
        ]
    );
}

#[test]
fn allows_cheap_checks_and_terminates_reference_cycles() {
    let manifest = catalog(
        r#"
        [tasks]
        health = [{ task = "baseline" }]
        baseline = ["cargo fmt --all -- --check", { task = "cycle" }]
        cycle = [{ task = "baseline" }]
        "#,
    );

    assert!(heavy_health_paths(&[manifest]).is_empty());
}
