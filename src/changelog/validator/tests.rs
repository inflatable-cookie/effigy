use super::*;
use crate::changelog::parser::parse_changelog;

fn validate_str(input: &str) -> Vec<ValidationDiagnostic> {
    let changelog = parse_changelog(input).expect("should parse");
    validate_changelog(&changelog, input)
}

#[test]
fn valid_changelog_has_no_diagnostics() {
    let input = "\
# Changelog

## [Unreleased]

### Added
- Feature

## [0.1.0] - 2026-03-01

### Added
- Initial release
";
    let diagnostics = validate_str(input);
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        diagnostics
    );
}

#[test]
fn missing_unreleased_section() {
    let input = "\
# Changelog

## [0.1.0] - 2026-03-01

### Added
- Feature
";
    let diagnostics = validate_str(input);
    assert!(diagnostics.iter().any(|d| d.rule == "missing-unreleased"));
}

#[test]
fn empty_category_detected() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking

### Added
- Feature
";
    let diagnostics = validate_str(input);
    let empty = diagnostics
        .iter()
        .filter(|d| d.rule == "empty-section")
        .collect::<Vec<_>>();
    assert_eq!(empty.len(), 1);
    assert!(empty[0].message.contains("Breaking"));
}

#[test]
fn duplicate_version_detected() {
    let input = "\
# Changelog

## [Unreleased]

## [0.1.0] - 2026-03-01

### Added
- Feature

## [0.1.0] - 2026-02-01

### Added
- Duplicate
";
    let diagnostics = validate_str(input);
    assert!(diagnostics.iter().any(|d| d.rule == "duplicate-version"));
}

#[test]
fn out_of_order_versions() {
    let input = "\
# Changelog

## [Unreleased]

## [0.1.0] - 2026-03-01

### Added
- Older

## [0.2.0] - 2026-03-09

### Added
- Newer
";
    let diagnostics = validate_str(input);
    assert!(diagnostics.iter().any(|d| d.rule == "version-ordering"));
}

#[test]
fn invalid_date_detected() {
    let input = "\
# Changelog

## [Unreleased]

## [0.1.0] - 2026-02-30

### Added
- Feature
";
    let diagnostics = validate_str(input);
    assert!(diagnostics.iter().any(|d| d.rule == "invalid-date"));
}

#[test]
fn trailing_blank_lines_detected() {
    let input = "# Changelog\n\n## [Unreleased]\n\n### Added\n- Feature\n\n";
    let diagnostics = validate_str(input);
    assert!(diagnostics.iter().any(|d| d.rule == "trailing-blank-lines"));
}

#[test]
fn date_validation_edge_cases() {
    assert!(is_valid_iso_date("2026-03-10"));
    assert!(is_valid_iso_date("2024-02-29")); // leap year
    assert!(!is_valid_iso_date("2023-02-29")); // not a leap year
    assert!(!is_valid_iso_date("2026-13-01")); // invalid month
    assert!(!is_valid_iso_date("2026-00-01")); // zero month
    assert!(!is_valid_iso_date("2026-01-32")); // invalid day
    assert!(!is_valid_iso_date("26-01-01")); // short year
    assert!(!is_valid_iso_date("2026-1-01")); // single digit month
    assert!(!is_valid_iso_date("not-a-date"));
}

#[test]
fn multiple_diagnostics_for_messy_changelog() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking

### Changed


### Added
- Feature

## [0.1.0] - 2026-02-30

### Fixed
";
    let diagnostics = validate_str(input);

    // Should catch: empty sections (Breaking, Changed, Fixed),
    // extra blank line, invalid date
    let rules: Vec<&str> = diagnostics.iter().map(|d| d.rule).collect();
    assert!(
        rules.contains(&"empty-section"),
        "missing empty-section diagnostic"
    );
    assert!(
        rules.contains(&"invalid-date"),
        "missing invalid-date diagnostic"
    );
    assert!(
        rules.contains(&"extra-blank-line"),
        "missing extra-blank-line diagnostic"
    );
}
