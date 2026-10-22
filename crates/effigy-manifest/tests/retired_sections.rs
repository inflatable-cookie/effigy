//! Sections for features v0.15 removed keep a v0.14.1 project loading.
//!
//! A project that still declares a retired scanner table, `[catalog.graph]`,
//! `[shell]` or `[demos]` loads from the composed manifest, lists its tasks
//! and gets one deprecation notice per manifest (effigy#274). The removed
//! feature's command still refuses, and `[drafts]` still fails the parse.

use std::path::Path;

use effigy_manifest::{load_task_manifest_with_inspection, TaskManifest};

fn tempdir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("effigy-retired-sections-{name}-"))
        .tempdir()
        .expect("owned fixture root")
}

fn write_manifest(root: &Path, body: &str) -> std::path::PathBuf {
    let manifest = root.join("effigy.toml");
    std::fs::write(&manifest, body).expect("write manifest");
    manifest
}

#[test]
fn every_retired_section_loads_from_the_composed_manifest() {
    for (section, body) in [
        (
            "scan.attention_markers",
            "[scan.attention_markers]\nwarn = 1",
        ),
        ("scan.comment_ratio", "[scan.comment_ratio]\nwarn = 1"),
        ("scan.generated_assets", "[scan.generated_assets]\nwarn = 1"),
        ("scan.generated_in_src", "[scan.generated_in_src]\nwarn = 1"),
        (
            "scan.stale_suppressions",
            "[scan.stale_suppressions]\nwarn = 1",
        ),
        (
            "scan.boundary_violations",
            "[scan.boundary_violations]\nwarn = 1",
        ),
        ("scan.dead_code", "[scan.dead_code]\nwarn = 1"),
        ("scan.validation_gaps", "[scan.validation_gaps]\nwarn = 1"),
        (
            "catalog.graph",
            "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true",
        ),
        (
            "demos",
            "[demos.checkout]\ntitle = \"Checkout\"\nrun = \"bun run demo\"",
        ),
        ("shell", "[shell]\nrun = \"exec zsh -i\""),
    ] {
        let root = tempdir(section);
        let manifest_path = write_manifest(
            root.path(),
            &format!("{body}\n\n[tasks.build]\nrun = \"printf built\"\n"),
        );
        let loaded = load_task_manifest_with_inspection(&manifest_path)
            .unwrap_or_else(|error| panic!("{section} keeps loading: {error}"));
        assert!(loaded.manifest.tasks.contains_key("build"), "{section}");
        let notice = loaded
            .manifest
            .retired_config
            .notice(&manifest_path)
            .unwrap_or_else(|| panic!("{section} names itself in the notice"));
        assert!(
            notice.matches("effigy.toml").count() == 1,
            "{section}: one notice per manifest: {notice}"
        );
        assert!(
            notice.contains(&format!("[{section}]")),
            "{section}: {notice}"
        );
        // The composed value keeps what the project declared, so `effigy config`
        // and an `extend` path still see the section the parse ignored.
        assert!(
            loaded
                .effective_value
                .get(section.split('.').next().unwrap())
                .is_some(),
            "{section}: the declared section stays in the effective manifest"
        );
    }
}

#[test]
fn an_include_carrying_a_retired_section_is_ignored_too() {
    let root = tempdir("include");
    let fragments = root.path().join("fragments");
    std::fs::create_dir_all(&fragments).expect("mkdir");
    std::fs::write(
        fragments.join("scan.toml"),
        "[scan.generated_in_src]\ninclude = [\"src/**\"]\n",
    )
    .expect("write fragment");
    let manifest_path = write_manifest(
        root.path(),
        "[manifest]\ninclude = [\"fragments/scan.toml\"]\n\n[tasks.build]\nrun = \"printf built\"\n",
    );
    let loaded =
        load_task_manifest_with_inspection(&manifest_path).expect("composed manifest loads");
    assert_eq!(loaded.manifest.retired_config.statements().len(), 1);
    assert!(loaded.manifest.retired_config.statements()[0].contains("[scan.generated_in_src]"));
    assert!(loaded.manifest.scan.is_none());
    assert!(loaded.manifest.tasks.contains_key("build"));
}

#[test]
fn several_retired_sections_share_one_notice() {
    let root = tempdir("combined");
    let manifest_path = write_manifest(
        root.path(),
        r#"
[scan.attention_markers]
warn = 1

[scan.dead_code]
warn = 1

[catalog]
alias = "bovine"

[catalog.graph]
segmented = true

[shell]
run = "exec zsh -i"

[tasks.build]
run = "printf built"
"#,
    );
    let loaded = load_task_manifest_with_inspection(&manifest_path).expect("manifest loads");
    let notice = loaded
        .manifest
        .retired_config
        .notice(&manifest_path)
        .expect("one notice");
    assert_eq!(notice.matches(" is ignored").count(), 4, "{notice}");
    assert_eq!(
        notice.lines().count(),
        1,
        "the notice stays one line: {notice}"
    );
}

#[test]
fn drafts_still_fail_the_parse() {
    let root = tempdir("drafts");
    let manifest_path = write_manifest(
        root.path(),
        "[scan.attention_markers]\nwarn = 1\n\n[drafts.proof]\nrun = \"echo old\"\n",
    );
    let error = load_task_manifest_with_inspection(&manifest_path)
        .expect_err("drafts stay a manifest error");
    let rendered = error.to_string();
    assert!(
        rendered.contains("`[drafts]` was removed in v0.15"),
        "{rendered}"
    );
    assert!(rendered.contains("CHANGELOG.md"), "{rendered}");
}

#[test]
fn the_remaining_scanners_still_configure_the_scan_section() {
    let root = tempdir("kept-scanners");
    let manifest_path = write_manifest(
        root.path(),
        "[scan.god_files]\nwarn = 1200\n\n[tasks.build]\nrun = \"printf built\"\n",
    );
    let TaskManifest {
        scan,
        retired_config,
        ..
    } = effigy_manifest::load_task_manifest(&manifest_path).expect("manifest loads");
    assert_eq!(
        scan.and_then(|scan| scan.god_files)
            .and_then(|god_files| god_files.warn),
        Some(1200)
    );
    assert!(retired_config.is_empty());
}
