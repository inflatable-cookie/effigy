use super::*;
use crate::parser::parse_changelog;

#[test]
fn extract_version_sections() {
    // One table over (input, version, expected content): a specific release
    // and the Unreleased section are the same lookup by different keys.
    for (input, version, expected) in [
        (
            "\
# Changelog

## [Unreleased]

### Added
- Upcoming feature

## [0.2.0] - 2026-03-09

### Breaking
- Changed spawn behavior

### Added
- CI workflow

## [0.1.0] - 2026-03-01

### Added
- Initial release
",
            "0.2.0",
            vec![
                "### Breaking",
                "Changed spawn behavior",
                "### Added",
                "CI workflow",
            ],
        ),
        (
            "\
# Changelog

## [Unreleased]

### Added
- Upcoming feature
",
            "Unreleased",
            vec!["### Added", "Upcoming feature"],
        ),
    ] {
        let changelog = parse_changelog(input).expect("should parse");
        let notes = extract_version(&changelog, version).expect("should find version");
        for snippet in expected {
            assert!(notes.contains(snippet), "{version}: {notes}");
        }
    }
}

#[test]
fn extract_nonexistent_version() {
    let input = "\
# Changelog

## [Unreleased]
";
    let changelog = parse_changelog(input).expect("should parse");

    assert!(extract_version(&changelog, "9.9.9").is_none());
}

#[test]
fn extract_excludes_empty_categories() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking

### Added
- Feature

### Fixed
";
    let changelog = parse_changelog(input).expect("should parse");

    let notes = extract_version(&changelog, "Unreleased").expect("should extract");
    assert!(notes.contains("### Added"));
    assert!(!notes.contains("### Breaking"));
    assert!(!notes.contains("### Fixed"));

    // An Unreleased section with no content at all extracts to None rather
    // than an empty note (the extract_empty_unreleased_returns_none row).
    let empty = parse_changelog(
        "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-03-01\n\n### Added\n- Feature\n",
    )
    .expect("should parse");
    assert!(extract_version(&empty, "Unreleased").is_none());
}

#[test]
fn extract_includes_release_prose() {
    for (input, expected) in [
        (
            "\
# Changelog

## [Unreleased]

Intro paragraph.

### Added
- Upcoming feature
",
            "Intro paragraph.\n\n### Added\n- Upcoming feature\n",
        ),
        (
            "# Changelog\n\n## [Unreleased]\n\nOnly prose, no categories.\n",
            "Only prose, no categories.\n",
        ),
    ] {
        let changelog = parse_changelog(input).expect("should parse");
        assert_eq!(
            extract_version(&changelog, "Unreleased").as_deref(),
            Some(expected)
        );
    }
}
