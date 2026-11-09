//! The one read path every consumer uses for a changelog and its fragments.
//!
//! A plain changelog without `changelog.d/` loads exactly as before. An
//! adopted changelog has its valid fragments folded into `[Unreleased]` for
//! analysis, validation and preparation, so every consumer sees the same
//! unreleased set. Nothing here writes to disk.

use std::path::Path;

use super::error::{ChangelogError, ValidationDiagnostic};
use super::fragments::{self, FragmentSet};
use super::types::{Category, Changelog, Entry, Release};
use super::{parser, validator};

/// A changelog read together with its adjacent fragments.
#[derive(Debug, Clone)]
pub struct ChangelogSource {
    /// The changelog, with valid fragments folded into `[Unreleased]`.
    pub changelog: Changelog,
    /// `CHANGELOG.md` exactly as read from disk.
    pub raw: String,
    /// The fragment directory beside the changelog.
    pub fragments: FragmentSet,
    /// Every profile diagnostic: changelog validation, fragment problems, and
    /// direct `[Unreleased]` entries in an adopted repository.
    pub diagnostics: Vec<ValidationDiagnostic>,
}

/// Read `path` and the fragments beside it.
///
/// Returns a parse error only when the changelog itself cannot be parsed.
/// Malformed fragments are diagnostics, and they are left out of the folded
/// changelog so no partial set is ever analysed as if it were complete.
pub fn load_source(path: &Path) -> Result<ChangelogSource, ChangelogError> {
    let raw = std::fs::read_to_string(path).map_err(|error| ChangelogError::Io {
        path: path.to_owned(),
        error,
    })?;
    let parsed = parser::parse_changelog(&raw)?;
    let mut diagnostics = validator::validate_changelog(&parsed, &raw);
    let fragments = fragments::load(path);
    diagnostics.extend(fragments.problems.iter().cloned());

    let mut changelog = parsed;
    if fragments.adopted {
        if let Some(direct) = changelog.unreleased().filter(|r| r.entry_count() > 0) {
            diagnostics.push(ValidationDiagnostic {
                line: direct.line,
                rule: "fragment-direct-entry",
                message: "[Unreleased] holds entries in a repository that records changes as changelog.d fragments".to_owned(),
                suggestion: Some(
                    "move each entry into its own changelog.d/<task-or-PR>-<slug>.<type>.md fragment"
                        .to_owned(),
                ),
            });
        }
        fold_into_unreleased(&mut changelog, &fragments);
    }

    Ok(ChangelogSource {
        changelog,
        raw,
        fragments,
        diagnostics,
    })
}

fn fold_into_unreleased(changelog: &mut Changelog, fragments: &FragmentSet) {
    let Some(unreleased) = changelog
        .releases
        .iter_mut()
        .find(|release: &&mut Release| release.is_unreleased())
    else {
        return;
    };
    for fragment in &fragments.fragments {
        let entry = Entry {
            description: fragment.entry.clone(),
            continuation_lines: Vec::new(),
            line: 0,
        };
        match unreleased
            .categories
            .iter_mut()
            .find(|category| category.kind == fragment.kind)
        {
            Some(category) => category.entries.push(entry),
            None => unreleased.categories.push(Category {
                kind: fragment.kind,
                entries: vec![entry],
                line: 0,
            }),
        }
    }
}

#[cfg(test)]
#[path = "source/tests.rs"]
mod tests;
