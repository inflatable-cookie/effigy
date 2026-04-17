use std::path::Path;

use effigy_changelog as changelog;

// ---- Full Pipeline Tests ----

#[test]
fn load_and_parse_effigy_changelog() {
    let path = Path::new("CHANGELOG.md");
    let parsed = changelog::load(path).expect("should parse CHANGELOG.md");

    assert_eq!(parsed.title, "Changelog");
    assert!(parsed.description.is_some());
    assert!(!parsed.releases.is_empty());
    assert!(parsed.releases[0].is_unreleased());
}

#[test]
fn format_then_validate_roundtrip() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");
    let formatted = changelog::format(&parsed);
    let reparsed = changelog::parse(&formatted).expect("should parse formatted");
    let diagnostics = changelog::validate(&reparsed, &formatted);

    assert!(
        diagnostics.is_empty(),
        "formatted changelog should be valid, got: {:?}",
        diagnostics
    );
}

#[test]
fn analyze_unreleased_section() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");
    let analysis = changelog::analyze(&parsed);

    assert!(analysis.current_version.is_some());
    let version = analysis.current_version.as_ref().unwrap();
    assert_eq!(version.major, 0);
    // Effigy is pre-1.0, so we know bump rules
}

#[test]
fn extract_known_version() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");

    let notes = changelog::extract_version(&parsed, "0.2.0");
    assert!(notes.is_some(), "should find version 0.2.0");
    let notes = notes.unwrap();
    assert!(notes.contains("### Breaking"));
    assert!(notes.contains("### Added"));
}

#[test]
fn extract_unreleased_section() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");
    let analysis = changelog::analyze(&parsed);

    let notes = changelog::extract_version(&parsed, "Unreleased");
    if analysis.unreleased_is_empty {
        assert!(notes.is_none(), "empty Unreleased should not render notes");
    } else {
        assert!(notes.is_some(), "should render non-empty Unreleased notes");
    }
}

#[test]
fn validate_detects_malformed_changelog() {
    let input = "\
# Changelog

## [Unreleased]

### Broken

### Added
- Feature

### Fixed
";
    let err = changelog::parse(input).unwrap_err();
    match err {
        changelog::ChangelogError::Parse { errors } => {
            assert!(!errors.is_empty());
            assert!(errors[0].message.contains("unknown category"));
        }
        _ => panic!("expected Parse error"),
    }
}

#[test]
fn validate_empty_sections_detected() {
    let input = "\
# Changelog

## [Unreleased]

### Breaking

### Added
- Feature
";
    let parsed = changelog::parse(input).expect("should parse");
    let diagnostics = changelog::validate(&parsed, input);

    let empty_sections: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "empty-section")
        .collect();
    assert_eq!(empty_sections.len(), 1);
}

#[test]
fn format_idempotent_on_already_formatted() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");
    let formatted1 = changelog::format(&parsed);
    let reparsed = changelog::parse(&formatted1).expect("should parse");
    let formatted2 = changelog::format(&reparsed);

    assert_eq!(formatted1, formatted2, "format should be idempotent");
}

#[test]
fn parse_preserves_multi_line_entries() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");

    // Find an entry that we know has continuation lines
    let has_continuation = parsed
        .releases
        .iter()
        .flat_map(|r| r.categories.iter())
        .flat_map(|c| c.entries.iter())
        .any(|e| !e.continuation_lines.is_empty());

    assert!(
        has_continuation,
        "should have at least one multi-line entry"
    );
}

#[test]
fn analyze_json_output_structure() {
    let content = include_str!("../CHANGELOG.md");
    let parsed = changelog::parse(content).expect("should parse");
    let analysis = changelog::analyze(&parsed);

    // Verify the analysis has the expected structure
    assert!(analysis.current_version.is_some());
    if analysis.unreleased_is_empty {
        assert!(analysis.unreleased_counts.is_empty());
        assert_eq!(analysis.suggested_bump, changelog::BumpKind::None);
        assert!(analysis.next_version.is_none());
    } else {
        assert!(matches!(
            analysis.suggested_bump,
            changelog::BumpKind::Patch | changelog::BumpKind::Minor | changelog::BumpKind::Major
        ));
        assert!(analysis.next_version.is_some());
    }
}
