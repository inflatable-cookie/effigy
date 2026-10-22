//! Retired `[bootstrap]` keys keep a v0.14.1 project loading.
//!
//! Every fixture under `tests/fixtures/retired-bootstrap/` is copied from a
//! real manifest that main refused after effigy#262 (acowtancy,
//! contact-patch, longhorn, cbs and jal-accounts from the DecodeLabs legacy
//! sites, plus figmatic and the underlay bundle). They load, list their tasks,
//! and only `effigy bootstrap` refuses the retired-only declarations.

use std::path::{Path, PathBuf};

use effigy_manifest::{
    load_task_manifest, load_task_manifest_with_inspection, ManifestTaskRunIn,
    ManifestTaskVisibility, TaskManifest,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/retired-bootstrap")
        .join(name)
}

fn load(name: &str) -> TaskManifest {
    load_task_manifest(&fixture(name)).expect("a v0.14.1 manifest keeps loading")
}

fn setup(manifest: &TaskManifest) -> Vec<String> {
    manifest
        .bootstrap
        .as_ref()
        .map(|bootstrap| bootstrap.setup.clone())
        .unwrap_or_default()
}

#[test]
fn acowtancy_run_steps_become_setup_selectors_and_children_refuse_in_bootstrap() {
    let manifest = load("acowtancy.toml");
    assert_eq!(setup(&manifest), vec!["secrets init", "bootstrap:state"]);
    assert!(manifest.tasks.contains_key("bootstrap:state"));
    let message = manifest
        .retired_bootstrap
        .refusal_message()
        .expect("children stay a bootstrap refusal");
    assert!(message.contains("[bootstrap].children"), "{message}");
    assert!(message.contains("[sources.<name>]"), "{message}");
    assert!(message.contains("rev_file"), "{message}");
    let notice = manifest
        .retired_bootstrap
        .notice(&fixture("acowtancy.toml"))
        .expect("one deprecation notice");
    assert!(
        notice.contains("`[bootstrap].run` is read as `setup`"),
        "{notice}"
    );
    assert!(notice.contains("children"), "{notice}");
}

#[test]
fn longhorn_task_table_setup_entries_are_read_as_selectors() {
    let manifest = load("longhorn.toml");
    assert_eq!(setup(&manifest), vec!["bootstrap:deps"]);
    let deps = manifest.tasks.get("bootstrap:deps").expect("declared task");
    assert!(deps.run.is_some());
    assert!(manifest.retired_bootstrap.refusals().is_empty());
}

#[test]
fn contact_patch_children_load_and_only_bootstrap_refuses_them() {
    let manifest = load("contact-patch.toml");
    assert!(manifest.tasks.contains_key("book:meta:test"));
    assert!(setup(&manifest).is_empty());
    assert_eq!(
        manifest
            .bootstrap
            .as_ref()
            .and_then(|bootstrap| bootstrap.start.as_deref()),
        None
    );
    assert_eq!(manifest.retired_bootstrap.refusals().len(), 1);
    assert!(manifest.retired_bootstrap.refusals()[0].contains("[sources.<name>]"));
}

#[test]
fn cbs_bundle_run_and_local_children_load_together() {
    let manifest = load("cbs.toml");
    assert_eq!(setup(&manifest), vec!["bootstrap:run:1"]);
    assert_eq!(
        manifest
            .bootstrap
            .as_ref()
            .and_then(|bootstrap| bootstrap.start.as_deref()),
        Some("dev")
    );
    let step = manifest
        .tasks
        .get("bootstrap:run:1")
        .expect("synthesised task");
    assert_eq!(step.visibility, Some(ManifestTaskVisibility::Internal));
    assert!(manifest.retired_bootstrap.refusals()[0].contains("[bootstrap].children"));
}

#[test]
fn jal_accounts_bundle_run_start_and_submodule_policy_load() {
    let manifest = load("jal-accounts.toml");
    assert_eq!(setup(&manifest), vec!["bootstrap:run:1"]);
    let bootstrap = manifest.bootstrap.as_ref().expect("bootstrap contract");
    assert_eq!(bootstrap.start.as_deref(), Some("dev"));
    assert!(bootstrap.submodules);
    assert!(manifest.retired_bootstrap.refusals().is_empty());
}

#[test]
fn figmatic_compact_run_and_required_children_load() {
    let manifest = load("figmatic.toml");
    assert_eq!(setup(&manifest), vec!["setup"]);
    assert!(manifest.retired_bootstrap.refusals()[0].contains("[bootstrap].children"));
}

#[test]
fn underlay_bundle_inline_steps_run_as_tasks_and_deps_sync_refuses_in_bootstrap() {
    let manifest = load("underlay-bundle.toml");
    assert_eq!(
        setup(&manifest),
        vec![
            "secrets init",
            "bootstrap:run:2",
            "bootstrap deps sync ../underlay app-api",
            "bootstrap:run:4",
        ]
    );
    let rhai = manifest
        .tasks
        .get("bootstrap:run:2")
        .expect("synthesised task");
    assert_eq!(rhai.visibility, Some(ManifestTaskVisibility::Internal));
    let command = manifest
        .tasks
        .get("bootstrap:run:4")
        .expect("synthesised task");
    assert!(command.run.is_some());
    let message = manifest
        .retired_bootstrap
        .refusal_message()
        .expect("deps sync stays a bootstrap refusal");
    assert!(message.contains("bootstrap deps sync"), "{message}");
    assert!(message.contains("[bootstrap].setup"), "{message}");
}

#[test]
fn a_synthesised_inline_step_keeps_its_run_in() {
    let root = tempdir("run-in");
    std::fs::write(
        root.path().join("effigy.toml"),
        "[bootstrap]\nrun = [{ run = \"bun install\", run_in = \"host\" }]\n",
    )
    .expect("write manifest");
    let manifest = load_task_manifest(&root.path().join("effigy.toml")).expect("manifest");
    let task = manifest
        .tasks
        .get("bootstrap:run:1")
        .expect("synthesised task");
    assert_eq!(task.run_in, Some(ManifestTaskRunIn::Host));
}

#[test]
fn bundle_defaults_compose_run_children_and_the_extend_path() {
    let fixture_root = tempdir("bundle");
    let root = fixture_root.path();
    let bundle = root.join("bundles/retired-bootstrap");
    copy_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/retired-bootstrap-bundle"),
        &bundle,
    );
    let manifest_path = root.join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/retired-bootstrap" }

[[bootstrap.children]]
path = "../poodle"
repo = "git@example.test:poodle.git"
branch = "main"

[tasks]
"bootstrap:deps" = "bun install --frozen-lockfile"
"#,
    )
    .expect("write consumer manifest");

    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("composed manifest");
    let manifest = loaded.manifest;
    let bootstrap = manifest.bootstrap.as_ref().expect("bootstrap contract");
    assert_eq!(bootstrap.setup, vec!["bootstrap:run:1"]);
    assert_eq!(bootstrap.start.as_deref(), Some("dev"));
    assert!(manifest.tasks.contains_key("bootstrap:run:1"));
    assert!(manifest.tasks.contains_key("dev"));
    // Both the bundle's child and the consumer's child reached the composed
    // manifest, and the retired-only surface still refuses only in bootstrap.
    let message = manifest
        .retired_bootstrap
        .refusal_message()
        .expect("children refuse in bootstrap");
    assert!(message.contains("[sources.<name>]"), "{message}");
    assert!(loaded.effective_manifest.contains("book.git"));
    assert!(loaded.effective_manifest.contains("poodle.git"));
}

fn tempdir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("effigy-retired-bootstrap-{name}-"))
        .tempdir()
        .expect("owned fixture root")
}

fn copy_dir(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).expect("mkdir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy fixture file");
        }
    }
}
