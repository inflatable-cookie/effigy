use super::*;
use crate::parser::parse_changelog;

fn analyze_str(input: &str) -> Analysis {
    let changelog = parse_changelog(input).expect("should parse");
    analyze_changelog(&changelog)
}

#[test]
fn empty_unreleased_suggests_no_bump() {
    let input = "\
# Changelog

## [Unreleased]

## [0.2.0] - 2026-03-09

### Added
- Feature
";
    let analysis = analyze_str(input);
    assert!(analysis.unreleased_is_empty);
    assert_eq!(analysis.suggested_bump, BumpKind::None);
    assert!(analysis.next_version.is_none());
    assert_eq!(
        analysis.current_version.as_ref().unwrap().to_string(),
        "0.2.0"
    );
}

#[test]
fn pre_1_0_breaking_suggests_minor() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking
- Breaking change

## [0.2.0] - 2026-03-09

### Added
- Feature
";
    let analysis = analyze_str(input);
    assert_eq!(analysis.suggested_bump, BumpKind::Minor);
    assert_eq!(analysis.next_version.as_ref().unwrap().to_string(), "0.3.0");
}

#[test]
fn post_1_0_breaking_suggests_major() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking
- Breaking change

## [1.0.0] - 2026-03-09

### Added
- Feature
";
    let analysis = analyze_str(input);
    assert_eq!(analysis.suggested_bump, BumpKind::Major);
    assert_eq!(analysis.next_version.as_ref().unwrap().to_string(), "2.0.0");
}

#[test]
fn post_1_0_added_suggests_minor() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- New feature

## [1.0.0] - 2026-03-09

### Added
- Feature
";
    let analysis = analyze_str(input);
    assert_eq!(analysis.suggested_bump, BumpKind::Minor);
    assert_eq!(analysis.next_version.as_ref().unwrap().to_string(), "1.1.0");
}

#[test]
fn fixed_only_suggests_patch() {
    let input = "\
# Changelog

## [Unreleased]

### Fixed
- Bug fix

## [1.2.3] - 2026-03-09

### Added
- Feature
";
    let analysis = analyze_str(input);
    assert_eq!(analysis.suggested_bump, BumpKind::Patch);
    assert_eq!(analysis.next_version.as_ref().unwrap().to_string(), "1.2.4");
}

#[test]
fn pre_1_0_added_suggests_patch() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- New feature

## [0.5.0] - 2026-03-09

### Added
- Feature
";
    let analysis = analyze_str(input);
    // Pre-1.0, Added (non-breaking) → PATCH
    assert_eq!(analysis.suggested_bump, BumpKind::Patch);
    assert_eq!(analysis.next_version.as_ref().unwrap().to_string(), "0.5.1");
}

#[test]
fn no_previous_versions() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- First feature
";
    let analysis = analyze_str(input);
    assert!(analysis.current_version.is_none());
    assert!(analysis.next_version.is_none());
    assert!(!analysis.unreleased_is_empty);
    // Still suggests a bump kind even without a base version
    assert_eq!(analysis.suggested_bump, BumpKind::Patch);
}

#[test]
fn unreleased_counts_are_correct() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking
- Break one

### Added
- Add one
- Add two

### Fixed
- Fix one

## [0.1.0] - 2026-03-01

### Added
- Initial
";
    let analysis = analyze_str(input);

    assert_eq!(analysis.unreleased_counts.get("Breaking"), Some(&1));
    assert_eq!(analysis.unreleased_counts.get("Added"), Some(&2));
    assert_eq!(analysis.unreleased_counts.get("Fixed"), Some(&1));
    assert_eq!(analysis.unreleased_counts.get("Changed"), None);
}

#[test]
fn analyze_effigy_changelog() {
    let content = include_str!("../../../../CHANGELOG.md");
    let changelog = parse_changelog(content).expect("should parse");
    let analysis = analyze_changelog(&changelog);

    // Effigy is pre-1.0 (0.2.x)
    assert!(analysis.current_version.is_some());
    let version = analysis.current_version.as_ref().unwrap();
    assert_eq!(version.major, 0);

    // The repo fixture may be checked before a release (with unreleased work)
    // or immediately after a release preparation (with an intentionally empty
    // [Unreleased] section). Both states should remain analyzable.
    if analysis.unreleased_is_empty {
        assert_eq!(analysis.suggested_bump, BumpKind::None);
        assert!(analysis.next_version.is_none());
        assert!(analysis.unreleased_counts.is_empty());
    } else {
        assert!(!analysis.unreleased_counts.is_empty());
        assert!(analysis.next_version.is_some());
    }
}
