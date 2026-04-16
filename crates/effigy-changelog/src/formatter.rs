//! Changelog formatting and normalization.
//!
//! The formatter normalizes a parsed changelog back to canonical Northstar
//! Profile form: empty sections are removed, spacing is normalized to exactly
//! one blank line between sections, and categories are sorted in canonical
//! order (Breaking, Added, Changed, Deprecated, Removed, Fixed, Security).

use super::types::{Changelog, Entry, Release};

/// Format a parsed changelog into canonical Northstar Profile form.
///
/// This removes empty categories, normalizes spacing, and enforces category
/// ordering. Entry content is preserved verbatim.
pub(super) fn format_changelog(changelog: &Changelog) -> String {
    let mut output = String::new();

    // Title
    output.push_str("# ");
    output.push_str(&changelog.title);
    output.push('\n');

    // Description
    if let Some(ref desc) = changelog.description {
        output.push('\n');
        output.push_str(desc);
        output.push('\n');
    }

    // Releases
    for release in &changelog.releases {
        output.push('\n');
        format_release(&mut output, release);
    }

    // Link references
    if !changelog.links.is_empty() {
        output.push('\n');
        for link in &changelog.links {
            output.push('[');
            output.push_str(&link.label);
            output.push_str("]: ");
            output.push_str(&link.url);
            output.push('\n');
        }
    }

    output
}

fn format_release(output: &mut String, release: &Release) {
    // Release header
    if release.is_unreleased() {
        output.push_str("## [Unreleased]\n");
    } else {
        let version = release.version.as_ref().unwrap();
        let date = release.date.as_deref().unwrap_or("YYYY-MM-DD");
        output.push_str(&format!("## [{version}] - {date}\n"));
    }

    // Categories — sorted in canonical order, empty ones omitted
    let mut sorted_categories: Vec<_> = release
        .categories
        .iter()
        .filter(|c| !c.entries.is_empty())
        .collect();
    sorted_categories.sort_by_key(|c| c.kind.sort_order());

    for cat in sorted_categories {
        output.push('\n');
        output.push_str("### ");
        output.push_str(cat.kind.header_text());
        output.push('\n');

        for entry in &cat.entries {
            format_entry(output, entry);
        }
    }
}

fn format_entry(output: &mut String, entry: &Entry) {
    output.push_str("- ");
    output.push_str(&entry.description);
    output.push('\n');

    for continuation in &entry.continuation_lines {
        output.push_str("  ");
        output.push_str(continuation);
        output.push('\n');
    }
}

#[cfg(test)]
#[path = "formatter/tests.rs"]
mod tests;
