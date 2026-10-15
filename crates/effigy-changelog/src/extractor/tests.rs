use super::*;
use crate::parser::parse_changelog;

#[test]
fn extract_specific_version() {
    let input = "\
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
";
    let changelog = parse_changelog(input).expect("should parse");

    let notes = extract_version(&changelog, "0.2.0").expect("should find version");
    assert!(notes.contains("### Breaking"));
    assert!(notes.contains("Changed spawn behavior"));
    assert!(notes.contains("### Added"));
    assert!(notes.contains("CI workflow"));
}

#[test]
fn extract_unreleased() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Upcoming feature
";
    let changelog = parse_changelog(input).expect("should parse");

    let notes = extract_version(&changelog, "Unreleased").expect("should find unreleased");
    assert!(notes.contains("### Added"));
    assert!(notes.contains("Upcoming feature"));
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
}

#[test]
fn extract_includes_release_prose() {
    let input = "\
# Changelog

## [Unreleased]

Intro paragraph.

### Added
- Upcoming feature
";
    let changelog = parse_changelog(input).expect("should parse");

    let notes = extract_version(&changelog, "Unreleased").expect("should extract");
    assert_eq!(notes, "Intro paragraph.\n\n### Added\n- Upcoming feature\n");
}

#[test]
fn extract_prose_only_release_returns_prose() {
    let input = "# Changelog\n\n## [Unreleased]\n\nOnly prose, no categories.\n";
    let changelog = parse_changelog(input).expect("should parse");

    assert_eq!(
        extract_version(&changelog, "Unreleased").as_deref(),
        Some("Only prose, no categories.\n")
    );
}

#[test]
fn extract_empty_unreleased_returns_none() {
    let input = "\
# Changelog

## [Unreleased]

## [0.1.0] - 2026-03-01

### Added
- Feature
";
    let changelog = parse_changelog(input).expect("should parse");

    assert!(extract_version(&changelog, "Unreleased").is_none());
}
