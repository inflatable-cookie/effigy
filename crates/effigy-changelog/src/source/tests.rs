use super::*;
use crate::test_support::Scratch;
use crate::{analyze, BumpKind};
use std::path::PathBuf;

const EMPTY_UNRELEASED_0_14: &str =
    "# Changelog\n\n## [Unreleased]\n\n## [0.14.1] - 2026-10-08\n\n### Fixed\n- Prior fix.\n";
const EMPTY_UNRELEASED_1_2: &str =
    "# Changelog\n\n## [Unreleased]\n\n## [1.2.0] - 2026-10-08\n\n### Fixed\n- Prior fix.\n";

fn adopted(scratch: &Scratch, base: &str) -> PathBuf {
    let changelog = scratch.write("CHANGELOG.md", base);
    scratch.write("changelog.d/.gitkeep", "");
    changelog
}

#[test]
fn a_plain_changelog_loads_exactly_as_before() {
    let scratch = Scratch::new("legacy");
    let changelog = scratch.write(
        "CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n### Added\n- A new thing.\n\n## [1.2.0] - 2026-10-08\n\n### Fixed\n- Prior fix.\n",
    );
    let source = load_source(&changelog).expect("load");
    assert!(!source.fragments.adopted);
    assert!(source.diagnostics.is_empty(), "{:?}", source.diagnostics);
    let analysis = analyze(&source.changelog);
    assert_eq!(analysis.suggested_bump, BumpKind::Minor);
    assert_eq!(analysis.unreleased_counts.get("Added"), Some(&1));
}

#[test]
fn fragments_alone_propose_the_same_bump_as_the_legacy_entries_they_replace() {
    // (fragment type, pre-1.0 bump, post-1.0 bump)
    let cases = [
        ("fixed", BumpKind::Patch, BumpKind::Patch),
        ("changed", BumpKind::Patch, BumpKind::Patch),
        ("security", BumpKind::Patch, BumpKind::Patch),
        ("added", BumpKind::Patch, BumpKind::Minor),
        ("removed", BumpKind::Minor, BumpKind::Major),
    ];
    for (kind, pre, post) in cases {
        let scratch = Scratch::new("bump");
        let changelog = adopted(&scratch, EMPTY_UNRELEASED_0_14);
        scratch.write(
            &format!("changelog.d/804-case.{kind}.md"),
            "One paragraph.\n",
        );
        let source = load_source(&changelog).expect("load");
        assert!(
            source.diagnostics.is_empty(),
            "{kind}: {:?}",
            source.diagnostics
        );
        assert_eq!(
            analyze(&source.changelog).suggested_bump,
            pre,
            "pre-1.0 {kind}"
        );

        let scratch = Scratch::new("bump-post");
        let changelog = adopted(&scratch, EMPTY_UNRELEASED_1_2);
        scratch.write(
            &format!("changelog.d/804-case.{kind}.md"),
            "One paragraph.\n",
        );
        let source = load_source(&changelog).expect("load");
        assert_eq!(
            analyze(&source.changelog).suggested_bump,
            post,
            "post-1.0 {kind}"
        );
    }
}

#[test]
fn an_empty_adopted_directory_proposes_nothing() {
    let scratch = Scratch::new("empty-adopted");
    let changelog = adopted(&scratch, EMPTY_UNRELEASED_0_14);
    let source = load_source(&changelog).expect("load");
    assert!(source.diagnostics.is_empty());
    let analysis = analyze(&source.changelog);
    assert!(analysis.unreleased_is_empty);
    assert_eq!(analysis.suggested_bump, BumpKind::None);
}

#[test]
fn direct_unreleased_entries_are_refused_once_the_repository_adopts_fragments() {
    let scratch = Scratch::new("direct");
    let changelog = adopted(
        &scratch,
        "# Changelog\n\n## [Unreleased]\n\n### Added\n- Typed straight in.\n\n## [0.14.1] - 2026-10-08\n\n### Fixed\n- Prior fix.\n",
    );
    let source = load_source(&changelog).expect("load");
    assert!(
        source
            .diagnostics
            .iter()
            .any(|d| d.rule == "fragment-direct-entry"),
        "{:?}",
        source.diagnostics
    );
}

#[test]
fn a_malformed_fragment_is_reported_and_never_counted() {
    let scratch = Scratch::new("malformed-source");
    let changelog = adopted(&scratch, EMPTY_UNRELEASED_0_14);
    scratch.write("changelog.d/804-bad.deprecated.md", "Nope.\n");
    scratch.write("changelog.d/804-good.fixed.md", "Fine.\n");
    let source = load_source(&changelog).expect("load");
    assert_eq!(source.fragments.fragments.len(), 1);
    assert!(source
        .diagnostics
        .iter()
        .any(|d| d.message.contains("804-bad.deprecated.md")));
    assert_eq!(
        analyze(&source.changelog).unreleased_counts.get("Fixed"),
        Some(&1)
    );
}

#[test]
fn loading_never_writes_the_changelog_or_fragments() {
    let scratch = Scratch::new("readonly");
    let changelog = adopted(&scratch, EMPTY_UNRELEASED_0_14);
    let fragment = scratch.write("changelog.d/804-keep.security.md", "Keep bytes.\n");
    let before_changelog = std::fs::read(&changelog).unwrap();
    let before_fragment = std::fs::read(&fragment).unwrap();
    let _ = load_source(&changelog).expect("load");
    assert_eq!(std::fs::read(&changelog).unwrap(), before_changelog);
    assert_eq!(std::fs::read(&fragment).unwrap(), before_fragment);
}
