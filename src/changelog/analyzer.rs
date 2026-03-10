//! Changelog analysis for version bump suggestions.
//!
//! Analyzes the Unreleased section of a changelog to determine the suggested
//! semver version bump based on which categories have entries, following the
//! Northstar Changelog Profile rules.

use std::collections::BTreeMap;

use super::types::{CategoryKind, Changelog};

/// Result of analyzing a changelog's Unreleased section.
#[derive(Debug, Clone)]
pub struct Analysis {
    /// Number of entries per category in the Unreleased section.
    pub unreleased_counts: BTreeMap<String, usize>,
    /// Whether the Unreleased section has zero entries.
    pub unreleased_is_empty: bool,
    /// Suggested semver version bump based on entry categories.
    pub suggested_bump: BumpKind,
    /// The most recent released version, if any.
    pub current_version: Option<semver::Version>,
    /// The computed next version (current + bump), if computable.
    pub next_version: Option<semver::Version>,
}

/// The kind of semver version bump suggested by changelog analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpKind {
    Major,
    Minor,
    Patch,
    None,
}

/// Analyze a changelog and return version bump suggestions.
///
/// The bump logic follows the Northstar Changelog Profile:
///
/// **Pre-1.0** (`current_version < 1.0.0`):
/// - Breaking entries → MINOR
/// - Any other entries → PATCH
/// - Empty → None
///
/// **Post-1.0** (`current_version >= 1.0.0`):
/// - Breaking entries → MAJOR
/// - Added entries (no Breaking) → MINOR
/// - Any other entries → PATCH
/// - Empty → None
pub(super) fn analyze_changelog(changelog: &Changelog) -> Analysis {
    let mut unreleased_counts = BTreeMap::new();
    let mut has_breaking = false;
    let mut has_added = false;
    let mut has_any = false;

    if let Some(unreleased) = changelog.unreleased() {
        for cat in &unreleased.categories {
            let count = cat.entries.len();
            if count > 0 {
                unreleased_counts.insert(cat.kind.header_text().to_owned(), count);
                has_any = true;

                match cat.kind {
                    CategoryKind::Breaking => has_breaking = true,
                    CategoryKind::Added => has_added = true,
                    _ => {}
                }
            }
        }
    }

    let current_version = changelog.latest_version().and_then(|r| r.version.clone());

    let is_pre_1_0 = current_version.as_ref().is_none_or(|v| v.major == 0);

    let suggested_bump = if !has_any {
        BumpKind::None
    } else if has_breaking {
        if is_pre_1_0 {
            BumpKind::Minor
        } else {
            BumpKind::Major
        }
    } else if has_added && !is_pre_1_0 {
        BumpKind::Minor
    } else {
        BumpKind::Patch
    };

    let next_version = current_version
        .as_ref()
        .and_then(|v| apply_bump(v, suggested_bump));

    Analysis {
        unreleased_counts,
        unreleased_is_empty: !has_any,
        suggested_bump,
        current_version,
        next_version,
    }
}

fn apply_bump(version: &semver::Version, bump: BumpKind) -> Option<semver::Version> {
    match bump {
        BumpKind::Major => Some(semver::Version::new(version.major + 1, 0, 0)),
        BumpKind::Minor => Some(semver::Version::new(version.major, version.minor + 1, 0)),
        BumpKind::Patch => Some(semver::Version::new(
            version.major,
            version.minor,
            version.patch + 1,
        )),
        BumpKind::None => None,
    }
}

impl std::fmt::Display for BumpKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BumpKind::Major => write!(f, "major"),
            BumpKind::Minor => write!(f, "minor"),
            BumpKind::Patch => write!(f, "patch"),
            BumpKind::None => write!(f, "none"),
        }
    }
}

#[cfg(test)]
#[path = "analyzer/tests.rs"]
mod tests;
