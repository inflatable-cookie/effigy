//! Read-only release surfaces over a changelog adopted as `changelog.d`
//! fragments: status and plan propose the right version, mutate nothing, and
//! refuse an escape.

use super::*;
use std::path::Path;

const FRAGMENT_BASE: &str = "# Changelog\n\n## [Unreleased]\n\n## [0.2.4] - 2026-10-01\n\n### Fixed\n- A previous release.\n";

/// A single-identity project at 0.2.4 with an empty `[Unreleased]` and the
/// given fragments.
fn fragment_repo(name: &str, fragments: &[(&str, &str)]) -> PathBuf {
    let root = temp_repo(name);
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\n",
    )
    .expect("manifest");
    fs::write(root.join("VERSION"), "0.2.4\n").expect("version");
    fs::write(root.join("CHANGELOG.md"), FRAGMENT_BASE).expect("changelog");
    fs::create_dir_all(root.join("changelog.d")).expect("fragment directory");
    fs::write(root.join("changelog.d/.gitkeep"), "").expect(".gitkeep");
    for (file, body) in fragments {
        fs::write(root.join("changelog.d").join(file), body).expect("fragment");
    }
    root
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for relative in [
        "CHANGELOG.md",
        "VERSION",
        "effigy.toml",
        "changelog.d/.gitkeep",
        "changelog.d/804-fixed-one.fixed.md",
        "changelog.d/805-added-two.added.md",
    ] {
        if let Ok(bytes) = fs::read(root.join(relative)) {
            files.insert(relative.to_owned(), bytes);
        }
    }
    files
}

#[test]
fn fragment_only_with_an_empty_unreleased_proposes_the_version_its_types_imply() {
    // Pre-1.0: a Fixed fragment is a PATCH, so 0.2.4 proposes 0.2.5.
    let root = fragment_repo(
        "fragment-proposal",
        &[("804-fixed-one.fixed.md", "Fix one.\n")],
    );
    let context = load_release_context(&root).expect("release context");
    assert!(
        !context.unreleased_empty,
        "fragments are unreleased changes"
    );
    assert_eq!(
        context.next_version.map(|v| v.to_string()),
        Some("0.2.5".to_owned())
    );
    assert!(
        context.changelog_diagnostics.is_empty(),
        "{:?}",
        context.changelog_diagnostics
    );

    let plan = crate::plan_release_pipeline(&root).expect("release plan");
    assert!(plan.changelog_valid, "{:?}", plan.changelog_diagnostics);
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert_eq!(plan.changelog_fragments.len(), 1);
    assert!(
        plan.files
            .iter()
            .any(|file| file.ends_with("changelog.d/804-fixed-one.fixed.md")),
        "the plan lists the fragment a prepare would consume: {:?}",
        plan.files
    );
}

#[test]
fn a_removed_fragment_proposes_a_minor_bump_pre_1_0() {
    let root = fragment_repo(
        "fragment-removed",
        &[("804-removed-one.removed.md", "**Breaking:** Removed.\n")],
    );
    let context = load_release_context(&root).expect("release context");
    assert_eq!(
        context.next_version.map(|v| v.to_string()),
        Some("0.3.0".to_owned())
    );
}

#[test]
fn status_and_plan_never_change_the_changelog_or_any_fragment() {
    let root = fragment_repo(
        "fragment-readonly",
        &[
            ("804-fixed-one.fixed.md", "Fix one.\n"),
            ("805-added-two.added.md", "Add two.\n"),
        ],
    );
    let before = snapshot(&root);

    load_release_context(&root).expect("release context");
    crate::plan_release_pipeline(&root).expect("release plan");

    assert_eq!(snapshot(&root), before);
}

#[test]
fn a_symlinked_fragment_directory_blocks_the_plan_without_being_followed() {
    let root = temp_repo("fragment-escape");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\n",
    )
    .expect("manifest");
    fs::write(root.join("VERSION"), "0.2.4\n").expect("version");
    fs::write(root.join("CHANGELOG.md"), FRAGMENT_BASE).expect("changelog");
    let outside = root.join("outside-fragments");
    fs::create_dir_all(&outside).expect("outside");
    fs::write(outside.join("804-escape.fixed.md"), "Escaped.\n").expect("outside fragment");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("changelog.d")).expect("symlink");

    let plan = crate::plan_release_pipeline(&root).expect("release plan");

    #[cfg(unix)]
    {
        assert!(
            plan.changelog_fragments.is_empty(),
            "nothing outside is folded"
        );
        assert!(!plan.changelog_valid);
        assert!(
            plan.changelog_diagnostics
                .iter()
                .any(|d| d.contains("changelog.d")),
            "{:?}",
            plan.changelog_diagnostics
        );
        assert!(
            outside.join("804-escape.fixed.md").is_file(),
            "outside file untouched"
        );
    }
}
