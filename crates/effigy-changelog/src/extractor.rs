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
#[path = "extractor/tests.rs"]
mod tests;
