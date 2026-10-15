//! Version note extraction from a parsed changelog.
//!
//! Renders the categories and entries for a specific version as standalone
//! markdown, suitable for GitHub Release notes.

use super::types::Changelog;

/// Extract release notes for a specific version as markdown.
///
/// Accepts `"Unreleased"` or a semver version string like `"0.2.0"`.
/// Returns the release body as standalone markdown without the version header:
/// free prose paragraphs first, then the categories and entries. Empty
/// categories are excluded.
///
/// Returns `None` if the version is not found or the release body is empty.
pub(super) fn extract_version(changelog: &Changelog, version: &str) -> Option<String> {
    let release = if version.eq_ignore_ascii_case("unreleased") {
        changelog.unreleased()?
    } else {
        changelog.find_version(version)?
    };

    // Each block is one prose paragraph or one category body, and each block
    // ends with a newline. Release prose is emitted before the categories, so
    // joining with a blank line keeps the canonical single-blank-line
    // separation for both prose and category content.
    let mut blocks: Vec<String> = release
        .prose
        .iter()
        .map(|paragraph| format!("{paragraph}\n"))
        .collect();

    for cat in &release.categories {
        if cat.entries.is_empty() {
            continue;
        }

        let mut block = String::new();
        block.push_str("### ");
        block.push_str(cat.kind.header_text());
        block.push('\n');

        for entry in &cat.entries {
            block.push_str("- ");
            block.push_str(&entry.description);
            block.push('\n');

            for continuation in &entry.continuation_lines {
                block.push_str("  ");
                block.push_str(continuation);
                block.push('\n');
            }
        }

        blocks.push(block);
    }

    if blocks.is_empty() {
        return None;
    }

    Some(blocks.join("\n"))
}

#[cfg(test)]
#[path = "extractor/tests.rs"]
mod tests;
