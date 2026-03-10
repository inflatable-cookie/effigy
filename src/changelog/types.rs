//! AST types for parsed changelog files.
//!
//! These types represent the in-memory structure of a changelog conforming to
//! the Northstar Changelog Profile — a strict subset of Keep a Changelog 1.0.0.

use std::fmt;

/// A fully parsed changelog file.
#[derive(Debug, Clone)]
pub struct Changelog {
    /// The changelog title (typically "Changelog").
    pub title: String,
    /// Optional description paragraph following the title.
    pub description: Option<String>,
    /// Releases in document order: Unreleased first, then newest-to-oldest.
    pub releases: Vec<Release>,
    /// Link references at the end of the file (`[label]: url`).
    pub links: Vec<LinkReference>,
}

/// A single release section (`## [X.Y.Z] - YYYY-MM-DD` or `## [Unreleased]`).
#[derive(Debug, Clone)]
pub struct Release {
    /// Parsed semver version. `None` for the Unreleased section.
    pub version: Option<semver::Version>,
    /// Date string in `YYYY-MM-DD` format. `None` for Unreleased.
    pub date: Option<String>,
    /// Categories present in this release (only non-empty categories).
    pub categories: Vec<Category>,
    /// Source line number of the release header.
    pub line: usize,
}

/// A category section within a release (`### Added`, `### Fixed`, etc.).
#[derive(Debug, Clone)]
pub struct Category {
    /// Which category this is.
    pub kind: CategoryKind,
    /// Entries within this category.
    pub entries: Vec<Entry>,
    /// Source line number of the category header.
    pub line: usize,
}

/// The fixed set of allowed category names in the Northstar Changelog Profile.
///
/// Categories are ordered: Breaking first, Security last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CategoryKind {
    Breaking,
    Added,
    Changed,
    Deprecated,
    Removed,
    Fixed,
    Security,
}

/// A single changelog entry (list item starting with `- `).
#[derive(Debug, Clone)]
pub struct Entry {
    /// The entry text after the `- ` prefix.
    pub description: String,
    /// Indented continuation lines following the entry.
    pub continuation_lines: Vec<String>,
    /// Source line number of the entry.
    pub line: usize,
}

/// A link reference at the end of the file (`[label]: url`).
#[derive(Debug, Clone)]
pub struct LinkReference {
    /// The link label (e.g., `"0.2.0"` or `"Unreleased"`).
    pub label: String,
    /// The URL.
    pub url: String,
    /// Source line number.
    pub line: usize,
}

impl CategoryKind {
    /// All category kinds in canonical sort order.
    pub const ALL: [CategoryKind; 7] = [
        CategoryKind::Breaking,
        CategoryKind::Added,
        CategoryKind::Changed,
        CategoryKind::Deprecated,
        CategoryKind::Removed,
        CategoryKind::Fixed,
        CategoryKind::Security,
    ];

    /// Parse a category name from its header text.
    pub fn parse_name(s: &str) -> Option<CategoryKind> {
        match s {
            "Breaking" => Some(CategoryKind::Breaking),
            "Added" => Some(CategoryKind::Added),
            "Changed" => Some(CategoryKind::Changed),
            "Deprecated" => Some(CategoryKind::Deprecated),
            "Removed" => Some(CategoryKind::Removed),
            "Fixed" => Some(CategoryKind::Fixed),
            "Security" => Some(CategoryKind::Security),
            _ => None,
        }
    }

    /// Returns a numeric sort key for canonical ordering.
    pub fn sort_order(self) -> u8 {
        match self {
            CategoryKind::Breaking => 0,
            CategoryKind::Added => 1,
            CategoryKind::Changed => 2,
            CategoryKind::Deprecated => 3,
            CategoryKind::Removed => 4,
            CategoryKind::Fixed => 5,
            CategoryKind::Security => 6,
        }
    }

    /// The section header text for this category.
    pub fn header_text(self) -> &'static str {
        match self {
            CategoryKind::Breaking => "Breaking",
            CategoryKind::Added => "Added",
            CategoryKind::Changed => "Changed",
            CategoryKind::Deprecated => "Deprecated",
            CategoryKind::Removed => "Removed",
            CategoryKind::Fixed => "Fixed",
            CategoryKind::Security => "Security",
        }
    }
}

impl fmt::Display for CategoryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.header_text())
    }
}

impl Ord for CategoryKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_order().cmp(&other.sort_order())
    }
}

impl PartialOrd for CategoryKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Changelog {
    /// Returns the Unreleased release, if present.
    pub fn unreleased(&self) -> Option<&Release> {
        self.releases.iter().find(|r| r.version.is_none())
    }

    /// Returns the most recent versioned release (highest version), if any.
    pub fn latest_version(&self) -> Option<&Release> {
        self.releases.iter().find(|r| r.version.is_some())
    }

    /// Find a release by version string.
    pub fn find_version(&self, version: &str) -> Option<&Release> {
        let target = semver::Version::parse(version).ok()?;
        self.releases
            .iter()
            .find(|r| r.version.as_ref() == Some(&target))
    }
}

impl Release {
    /// Whether this is the Unreleased section.
    pub fn is_unreleased(&self) -> bool {
        self.version.is_none()
    }

    /// Total number of entries across all categories.
    pub fn entry_count(&self) -> usize {
        self.categories.iter().map(|c| c.entries.len()).sum()
    }
}
