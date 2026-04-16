use super::*;

#[test]
fn parse_minimal_changelog() {
    let input = "# Changelog\n\n## [Unreleased]\n";
    let changelog = parse_changelog(input).expect("should parse");

    assert_eq!(changelog.title, "Changelog");
    assert!(changelog.description.is_none());
    assert_eq!(changelog.releases.len(), 1);
    assert!(changelog.releases[0].is_unreleased());
    assert!(changelog.releases[0].categories.is_empty());
}

#[test]
fn parse_title_and_description() {
    let input = "\
# Changelog

All notable changes to this project will be documented in this file.
The format is based on Keep a Changelog.

## [Unreleased]
";
    let changelog = parse_changelog(input).expect("should parse");

    assert_eq!(changelog.title, "Changelog");
    let desc = changelog.description.as_deref().unwrap();
    assert!(desc.contains("All notable changes"));
    assert!(desc.contains("Keep a Changelog"));
}

#[test]
fn parse_single_release_with_entries() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Feature one
- Feature two

### Fixed
- Bug fix
";
    let changelog = parse_changelog(input).expect("should parse");

    assert_eq!(changelog.releases.len(), 1);
    let release = &changelog.releases[0];
    assert!(release.is_unreleased());
    assert_eq!(release.categories.len(), 2);

    let added = &release.categories[0];
    assert_eq!(added.kind, CategoryKind::Added);
    assert_eq!(added.entries.len(), 2);
    assert_eq!(added.entries[0].description, "Feature one");
    assert_eq!(added.entries[1].description, "Feature two");

    let fixed = &release.categories[1];
    assert_eq!(fixed.kind, CategoryKind::Fixed);
    assert_eq!(fixed.entries.len(), 1);
    assert_eq!(fixed.entries[0].description, "Bug fix");
}

#[test]
fn parse_versioned_release() {
    let input = "\
# Changelog

## [1.0.0] - 2026-03-10

### Added
- Initial release
";
    let changelog = parse_changelog(input).expect("should parse");

    assert_eq!(changelog.releases.len(), 1);
    let release = &changelog.releases[0];
    assert!(!release.is_unreleased());
    assert_eq!(release.version.as_ref().unwrap().to_string(), "1.0.0");
    assert_eq!(release.date.as_deref(), Some("2026-03-10"));
}

#[test]
fn parse_multiple_releases() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- New feature

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

    assert_eq!(changelog.releases.len(), 3);
    assert!(changelog.releases[0].is_unreleased());
    assert_eq!(
        changelog.releases[1].version.as_ref().unwrap().to_string(),
        "0.2.0"
    );
    assert_eq!(
        changelog.releases[2].version.as_ref().unwrap().to_string(),
        "0.1.0"
    );
}

#[test]
fn parse_all_category_kinds() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking
- break

### Added
- add

### Changed
- change

### Deprecated
- deprecate

### Removed
- remove

### Fixed
- fix

### Security
- security
";
    let changelog = parse_changelog(input).expect("should parse");

    let release = &changelog.releases[0];
    assert_eq!(release.categories.len(), 7);
    assert_eq!(release.categories[0].kind, CategoryKind::Breaking);
    assert_eq!(release.categories[1].kind, CategoryKind::Added);
    assert_eq!(release.categories[2].kind, CategoryKind::Changed);
    assert_eq!(release.categories[3].kind, CategoryKind::Deprecated);
    assert_eq!(release.categories[4].kind, CategoryKind::Removed);
    assert_eq!(release.categories[5].kind, CategoryKind::Fixed);
    assert_eq!(release.categories[6].kind, CategoryKind::Security);
}

#[test]
fn parse_multi_line_entry() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Add `@env-spec` integration: declarative `.env.schema` files with annotation
  DSL (`@type`, `@required`, `@sensitive`), value expressions (`exec()`,
  `env()`, `${VAR}` templates)
";
    let changelog = parse_changelog(input).expect("should parse");

    let entry = &changelog.releases[0].categories[0].entries[0];
    assert!(entry.description.starts_with("Add `@env-spec`"));
    assert_eq!(entry.continuation_lines.len(), 2);
    assert!(entry.continuation_lines[0].contains("DSL"));
    assert!(entry.continuation_lines[1].contains("templates"));
}

#[test]
fn parse_link_references() {
    let input = "\
# Changelog

## [Unreleased]

## [0.2.0] - 2026-03-09

### Added
- Feature

[Unreleased]: https://github.com/org/repo/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/org/repo/releases/tag/v0.2.0
";
    let changelog = parse_changelog(input).expect("should parse");

    assert_eq!(changelog.links.len(), 2);
    assert_eq!(changelog.links[0].label, "Unreleased");
    assert!(changelog.links[0].url.contains("compare"));
    assert_eq!(changelog.links[1].label, "0.2.0");
    assert!(changelog.links[1].url.contains("releases"));
}

#[test]
fn parse_empty_categories_are_preserved() {
    // The parser preserves empty categories as-is; the formatter removes them
    let input = "\
# Changelog

## [Unreleased]

### Breaking

### Added
- Feature

### Fixed
";
    let changelog = parse_changelog(input).expect("should parse");

    let release = &changelog.releases[0];
    // Empty categories still parsed — they have zero entries
    assert_eq!(release.categories.len(), 3);
    assert_eq!(release.categories[0].entries.len(), 0); // Breaking: empty
    assert_eq!(release.categories[1].entries.len(), 1); // Added: 1 entry
    assert_eq!(release.categories[2].entries.len(), 0); // Fixed: empty
}

#[test]
fn error_unknown_category() {
    let input = "\
# Changelog

## [Unreleased]

### Miscellaneous
- Something
";
    let err = parse_changelog(input).unwrap_err();
    match err {
        ChangelogError::Parse { errors } => {
            assert_eq!(
                errors.len(),
                1,
                "expected exactly one error, got: {errors:?}"
            );
            assert!(errors[0].message.contains("unknown category"));
            assert!(errors[0].message.contains("Miscellaneous"));
        }
        _ => panic!("expected Parse error"),
    }
}

#[test]
fn error_entry_outside_category() {
    let input = "\
# Changelog

## [Unreleased]
- Orphan entry
";
    let err = parse_changelog(input).unwrap_err();
    match err {
        ChangelogError::Parse { errors } => {
            assert_eq!(errors.len(), 1);
            assert!(errors[0].message.contains("entry found outside"));
        }
        _ => panic!("expected Parse error"),
    }
}

#[test]
fn parse_effigy_changelog() {
    // Parse the actual Effigy CHANGELOG.md as a primary fixture
    let content = include_str!("../../../../CHANGELOG.md");
    let changelog = parse_changelog(content).expect("Effigy CHANGELOG should parse");

    assert_eq!(changelog.title, "Changelog");
    assert!(changelog.description.is_some());

    // Should have Unreleased + several versioned releases
    assert!(changelog.releases.len() >= 5);
    assert!(changelog.releases[0].is_unreleased());

    // The second release should be versioned
    assert!(changelog.releases[1].version.is_some());
}

#[test]
fn parse_star_entries() {
    let input = "\
# Changelog

## [Unreleased]

### Added
* Feature using star syntax
";
    let changelog = parse_changelog(input).expect("should parse");
    assert_eq!(
        changelog.releases[0].categories[0].entries[0].description,
        "Feature using star syntax"
    );
}

#[test]
fn line_numbers_are_correct() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Entry on line 6
";
    let changelog = parse_changelog(input).expect("should parse");

    let release = &changelog.releases[0];
    assert_eq!(release.line, 3); // ## [Unreleased] is line 3
    assert_eq!(release.categories[0].line, 5); // ### Added is line 5
    assert_eq!(release.categories[0].entries[0].line, 6); // entry is line 6
}

#[test]
fn find_version_works() {
    let input = "\
# Changelog

## [Unreleased]

## [0.2.0] - 2026-03-09

### Added
- Feature

## [0.1.0] - 2026-03-01

### Added
- Initial
";
    let changelog = parse_changelog(input).expect("should parse");

    let found = changelog.find_version("0.2.0");
    assert!(found.is_some());
    assert_eq!(found.unwrap().date.as_deref(), Some("2026-03-09"));

    assert!(changelog.find_version("0.3.0").is_none());
}
