use super::*;
use crate::parser::parse_changelog;

fn analyze_str(input: &str) -> Analysis {
    let changelog = parse_changelog(input).expect("should parse");
    analyze_changelog(&changelog)
}

/// The bump a changelog suggests, its base version and its next version.
struct BumpCase {
    name: &'static str,
    input: &'static str,
    bump: BumpKind,
    current: Option<&'static str>,
    next: Option<&'static str>,
    unreleased_is_empty: bool,
}

/// Category and base version decide the bump, including the pre-1.0 rules.
#[test]
fn suggested_bumps_follow_the_category_and_the_base_version() {
    let cases = [
        BumpCase {
            name: "empty-unreleased-suggests-no-bump",
            input: "\
# Changelog

## [Unreleased]

## [0.2.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::None,
            current: Some("0.2.0"),
            next: None,
            unreleased_is_empty: true,
        },
        BumpCase {
            name: "pre-1.0-breaking-suggests-minor",
            input: "\
# Changelog

## [Unreleased]

### Breaking
- Breaking change

## [0.2.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Minor,
            current: Some("0.2.0"),
            next: Some("0.3.0"),
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "post-1.0-breaking-suggests-major",
            input: "\
# Changelog

## [Unreleased]

### Breaking
- Breaking change

## [1.0.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Major,
            current: Some("1.0.0"),
            next: Some("2.0.0"),
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "post-1.0-added-suggests-minor",
            input: "\
# Changelog

## [Unreleased]

### Added
- New feature

## [1.0.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Minor,
            current: Some("1.0.0"),
            next: Some("1.1.0"),
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "fixed-only-suggests-patch",
            input: "\
# Changelog

## [Unreleased]

### Fixed
- Bug fix

## [1.2.3] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Patch,
            current: Some("1.2.3"),
            next: Some("1.2.4"),
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "pre-1.0-added-suggests-patch",
            input: "\
# Changelog

## [Unreleased]

### Added
- New feature

## [0.5.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Patch,
            current: Some("0.5.0"),
            next: Some("0.5.1"),
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "no-previous-version-still-suggests-a-bump-kind",
            input: "\
# Changelog

## [Unreleased]

### Added
- First feature
",
            bump: BumpKind::Patch,
            current: None,
            next: None,
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "pre-1.0-removed-suggests-minor",
            input: "\
# Changelog

## [Unreleased]

### Removed
- Removed public API

## [0.5.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Minor,
            current: Some("0.5.0"),
            next: Some("0.6.0"),
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "post-1.0-removed-suggests-major",
            input: "\
# Changelog

## [Unreleased]

### Removed
- Removed public API

## [1.2.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Major,
            current: Some("1.2.0"),
            next: Some("2.0.0"),
            unreleased_is_empty: false,
        },
        BumpCase {
            name: "pre-1.0-mixed-removed-and-fixed-suggests-minor",
            input: "\
# Changelog

## [Unreleased]

### Removed
- Removed public API

### Fixed
- Compatible fix

## [0.5.0] - 2026-03-09

### Added
- Feature
",
            bump: BumpKind::Minor,
            current: Some("0.5.0"),
            next: Some("0.6.0"),
            unreleased_is_empty: false,
        },
    ];

    for case in cases {
        let analysis = analyze_str(case.input);
        assert_eq!(analysis.suggested_bump, case.bump, "{}", case.name);
        assert_eq!(
            analysis
                .current_version
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            case.current,
            "{}",
            case.name
        );
        assert_eq!(
            analysis
                .next_version
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            case.next,
            "{}",
            case.name
        );
        assert_eq!(
            analysis.unreleased_is_empty, case.unreleased_is_empty,
            "{}",
            case.name
        );
    }
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
fn repeated_category_counts_all_entries_even_before_validation() {
    let input = "# Changelog\n\n## [Unreleased]\n\n### Added\n- First\n\n### Fixed\n- Repair\n\n### Added\n- Second\n\n## [0.1.0] - 2026-03-01\n\n### Added\n- Initial\n";
    let analysis = analyze_str(input);
    assert_eq!(analysis.unreleased_counts.get("Added"), Some(&2));
    assert_eq!(analysis.unreleased_counts.get("Fixed"), Some(&1));
}
