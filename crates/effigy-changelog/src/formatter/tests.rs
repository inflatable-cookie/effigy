use super::*;
use crate::parser::parse_changelog;

#[test]
fn format_removes_empty_sections() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking

### Added
- Feature one

### Changed

### Fixed
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    assert!(formatted.contains("### Added"));
    assert!(formatted.contains("- Feature one"));
    assert!(!formatted.contains("### Breaking"));
    assert!(!formatted.contains("### Changed"));
    assert!(!formatted.contains("### Fixed"));
}

#[test]
fn format_preserves_unreleased_even_when_empty() {
    let input = "\
# Changelog

## [Unreleased]

## [0.1.0] - 2026-03-01

### Added
- Feature
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    assert!(formatted.contains("## [Unreleased]"));
    assert!(formatted.contains("## [0.1.0] - 2026-03-01"));
}

#[test]
fn format_sorts_categories_canonically() {
    let input = "\
# Changelog

## [Unreleased]

### Fixed
- Bug fix

### Added
- Feature

### Breaking
- Breaking change
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    let breaking_pos = formatted.find("### Breaking").unwrap();
    let added_pos = formatted.find("### Added").unwrap();
    let fixed_pos = formatted.find("### Fixed").unwrap();

    assert!(
        breaking_pos < added_pos,
        "Breaking should come before Added"
    );
    assert!(added_pos < fixed_pos, "Added should come before Fixed");
}

#[test]
fn format_normalizes_spacing() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Feature

## [0.1.0] - 2026-03-01

### Added
- Initial
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    // Should not have double blank lines
    assert!(
        !formatted.contains("\n\n\n"),
        "should not have triple newlines"
    );
}

#[test]
fn format_roundtrip_is_idempotent() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Feature one
- Feature two

### Fixed
- Bug fix

## [0.1.0] - 2026-03-01

### Added
- Initial release
";
    let parsed1 = parse_changelog(input).expect("should parse");
    let formatted1 = format_changelog(&parsed1);
    let parsed2 = parse_changelog(&formatted1).expect("should parse formatted");
    let formatted2 = format_changelog(&parsed2);

    assert_eq!(formatted1, formatted2, "format should be idempotent");
}

#[test]
fn format_preserves_multi_line_entries() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Add `@env-spec` integration: declarative `.env.schema` files with annotation
  DSL (`@type`, `@required`, `@sensitive`), value expressions
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    assert!(formatted.contains("- Add `@env-spec` integration"));
    assert!(formatted.contains("  DSL (`@type`, `@required`, `@sensitive`)"));
}

#[test]
fn format_preserves_link_references() {
    let input = "\
# Changelog

## [Unreleased]

## [0.1.0] - 2026-03-01

### Added
- Feature

[Unreleased]: https://github.com/org/repo/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/org/repo/releases/tag/v0.1.0
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    assert!(formatted.contains("[Unreleased]: https://"));
    assert!(formatted.contains("[0.1.0]: https://"));
}

/// An empty category section is dropped rather than left as a bare heading.
#[test]
fn formatting_removes_empty_category_sections() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking

### Added
- One feature

### Changed

### Fixed
- One fix

## [0.1.0] - 2026-03-01

### Added
- Initial
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    let lines: Vec<&str> = formatted.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("### ") {
            continue;
        }
        let next = lines[index + 1..]
            .iter()
            .find(|line| !line.trim().is_empty());
        assert!(
            !next.is_some_and(|next| next.starts_with("## ") || next.starts_with("### ")),
            "found a bare empty section at line {}: `{line}`",
            index + 1
        );
    }
    assert!(formatted.contains("### Added"), "{formatted}");
    assert!(formatted.contains("### Fixed"), "{formatted}");
    assert!(!formatted.contains("### Breaking"), "{formatted}");
    assert!(!formatted.contains("### Changed"), "{formatted}");
}

#[test]
fn format_keeps_release_prose_before_categories() {
    let input = "\
# Changelog

## [Unreleased]

Intro paragraph.

### Added
- Feature

## [0.1.0] - 2026-03-01

Old note.

### Added
- Initial release
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    assert!(
        formatted.contains("## [Unreleased]\n\nIntro paragraph.\n\n### Added"),
        "{formatted}"
    );
    assert!(
        formatted.contains("## [0.1.0] - 2026-03-01\n\nOld note.\n\n### Added"),
        "{formatted}"
    );
}

#[test]
fn format_prose_roundtrip_is_idempotent() {
    let input = "\
# Changelog

## [Unreleased]

First paragraph line one
continues on line two.

Second paragraph.

### Added
- Feature
";
    let parsed1 = parse_changelog(input).expect("should parse");
    let formatted1 = format_changelog(&parsed1);
    let parsed2 = parse_changelog(&formatted1).expect("should parse formatted");
    let formatted2 = format_changelog(&parsed2);

    assert_eq!(formatted1, formatted2, "format should be idempotent");
}

#[test]
fn format_removes_empty_releases() {
    let input = "\
# Changelog

## [Unreleased]

## [0.2.0] - 2026-03-10

### Breaking

### Changed

## [0.1.0] - 2026-03-01

### Added
- Feature
";
    let changelog = parse_changelog(input).expect("should parse");
    let formatted = format_changelog(&changelog);

    // 0.2.0 had only empty categories, so after formatting it should have no
    // category sections. But the release header should still appear.
    assert!(formatted.contains("## [0.2.0]"));
    assert!(formatted.contains("## [0.1.0]"));
}
