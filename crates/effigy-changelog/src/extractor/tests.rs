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
