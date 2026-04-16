//! Version note extraction from a parsed changelog.
//!
//! Renders the categories and entries for a specific version as standalone
//! markdown, suitable for GitHub Release notes.

use super::types::Changelog;

/// Extract release notes for a specific version as markdown.
///
/// Accepts `"Unreleased"` or a semver version string like `"0.2.0"`.
/// Returns the categories and entries as standalone markdown without the
/// version header. Empty categories are excluded.
///
/// Returns `None` if the version is not found.
pub(super) fn extract_version(changelog: &Changelog, version: &str) -> Option<String> {
    let release = if version.eq_ignore_ascii_case("unreleased") {
        changelog.unreleased()?
    } else {
        changelog.find_version(version)?
    };

    let mut output = String::new();
    let mut first = true;

    for cat in &release.categories {
        if cat.entries.is_empty() {
            continue;
        }

        if !first {
            output.push('\n');
        }
        first = false;

        output.push_str("### ");
        output.push_str(cat.kind.header_text());
        output.push('\n');

        for entry in &cat.entries {
            output.push_str("- ");
            output.push_str(&entry.description);
            output.push('\n');

            for continuation in &entry.continuation_lines {
                output.push_str("  ");
                output.push_str(continuation);
                output.push('\n');
            }
        }
    }

    if output.is_empty() {
        return None;
    }

    Some(output)
}

#[cfg(test)]
mod tests {
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
}
