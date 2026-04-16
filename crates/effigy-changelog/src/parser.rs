//! Line-based parser for changelog files.
//!
//! The parser is a hand-written state machine that processes the file line by
//! line. It expects the Northstar Changelog Profile format: a title line
//! (`# Changelog`), followed by release sections (`## [X.Y.Z] - YYYY-MM-DD`),
//! category sections (`### Added`, `### Fixed`, etc.), and list entries
//! (`- description`).
//!
//! Link references (`[label]: url`) at the end of the file are collected
//! separately.

use super::error::{ChangelogError, ParseDiagnostic};
use super::types::{Category, CategoryKind, Changelog, Entry, LinkReference, Release};

/// Parse the content of a changelog file into a [`Changelog`] AST.
///
/// Collects all parse errors and returns them together rather than failing
/// on the first error.
pub(super) fn parse_changelog(content: &str) -> Result<Changelog, ChangelogError> {
    let mut parser = Parser::new(content);
    parser.parse();

    if !parser.errors.is_empty() {
        return Err(ChangelogError::Parse {
            errors: parser.errors,
        });
    }

    Ok(Changelog {
        title: parser.title,
        description: parser.description,
        releases: parser.releases,
        links: parser.links,
    })
}

#[derive(Debug, PartialEq)]
enum State {
    Start,
    Preamble,
    InRelease,
    InCategory,
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
    state: State,
    title: String,
    description: Option<String>,
    releases: Vec<Release>,
    links: Vec<LinkReference>,
    errors: Vec<ParseDiagnostic>,

    // Current release being built
    current_release: Option<Release>,
    // Current category being built
    current_category: Option<Category>,
}

impl<'a> Parser<'a> {
    fn new(content: &'a str) -> Self {
        Self {
            lines: content.lines().collect(),
            pos: 0,
            state: State::Start,
            title: String::new(),
            description: None,
            releases: Vec::new(),
            links: Vec::new(),
            errors: Vec::new(),
            current_release: None,
            current_category: None,
        }
    }

    fn line_num(&self) -> usize {
        self.pos + 1
    }

    fn parse(&mut self) {
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos];
            let trimmed = line.trim();

            match self.state {
                State::Start => self.handle_start(trimmed),
                State::Preamble => self.handle_preamble(trimmed, line),
                State::InRelease => self.handle_in_release(trimmed, line),
                State::InCategory => self.handle_in_category(trimmed, line),
            }
            self.pos += 1;
        }

        // Finalize any pending category/release
        self.finalize_category();
        self.finalize_release();
    }

    fn handle_start(&mut self, trimmed: &str) {
        if trimmed.is_empty() {
            return;
        }
        if let Some(title) = trimmed.strip_prefix("# ") {
            self.title = title.trim().to_owned();
            self.state = State::Preamble;
        } else {
            self.errors.push(ParseDiagnostic {
                line: self.line_num(),
                message: format!(
                    "expected `# Changelog` title, got: {}",
                    truncate_for_display(trimmed, 60)
                ),
            });
            // Try to recover by treating it as the title
            self.title = trimmed.to_owned();
            self.state = State::Preamble;
        }
    }

    fn handle_preamble(&mut self, trimmed: &str, _raw: &str) {
        if trimmed.is_empty() {
            return;
        }

        // Check if this is a release header
        if let Some(release) = try_parse_release_header(trimmed, self.line_num()) {
            self.current_release = Some(release);
            self.state = State::InRelease;
            return;
        }

        // Check if it looks like a header we should not be seeing
        if trimmed.starts_with("## ") {
            self.errors.push(ParseDiagnostic {
                line: self.line_num(),
                message: format!(
                    "invalid release header: {}",
                    truncate_for_display(trimmed, 60)
                ),
            });
            return;
        }

        // Otherwise, accumulate description
        let desc = self.description.get_or_insert_with(String::new);
        if !desc.is_empty() {
            desc.push('\n');
        }
        desc.push_str(trimmed);
    }

    fn handle_in_release(&mut self, trimmed: &str, line: &str) {
        if trimmed.is_empty() {
            return;
        }

        // New release header?
        if let Some(release) = try_parse_release_header(trimmed, self.line_num()) {
            self.finalize_category();
            self.finalize_release();
            self.current_release = Some(release);
            self.state = State::InRelease;
            return;
        }

        // Category header?
        if let Some(cat_name) = trimmed.strip_prefix("### ") {
            let cat_name = cat_name.trim();
            if let Some(kind) = CategoryKind::parse_name(cat_name) {
                self.finalize_category();
                self.current_category = Some(Category {
                    kind,
                    entries: Vec::new(),
                    line: self.line_num(),
                });
                self.state = State::InCategory;
            } else {
                self.errors.push(ParseDiagnostic {
                    line: self.line_num(),
                    message: format!(
                        "unknown category `{cat_name}` (expected: {})",
                        CategoryKind::ALL
                            .iter()
                            .map(|k| k.header_text())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
                // Still transition to InCategory so entries are silently
                // consumed rather than producing cascading errors.
                self.finalize_category();
                self.current_category = None;
                self.state = State::InCategory;
            }
            return;
        }

        // Link reference? Check if we've entered the link references section
        if let Some(link) = try_parse_link_reference(trimmed, self.line_num()) {
            self.finalize_category();
            self.finalize_release();
            self.links.push(link);
            // Consume remaining link references
            self.consume_link_references();
            return;
        }

        // Entry outside of a category
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            self.errors.push(ParseDiagnostic {
                line: self.line_num(),
                message: "entry found outside of a category section".to_owned(),
            });
            return;
        }

        // Unexpected content
        if !trimmed.starts_with("## ") {
            // Could be malformed — try to give a helpful message
            if try_parse_link_reference(trimmed, self.line_num()).is_none() {
                self.errors.push(ParseDiagnostic {
                    line: self.line_num(),
                    message: format!("unexpected content: {}", truncate_for_display(line, 60)),
                });
            }
        } else {
            self.errors.push(ParseDiagnostic {
                line: self.line_num(),
                message: format!(
                    "invalid release header: {}",
                    truncate_for_display(trimmed, 60)
                ),
            });
        }
    }

    fn handle_in_category(&mut self, trimmed: &str, line: &str) {
        if trimmed.is_empty() {
            return;
        }

        // New release header?
        if let Some(release) = try_parse_release_header(trimmed, self.line_num()) {
            self.finalize_category();
            self.finalize_release();
            self.current_release = Some(release);
            self.state = State::InRelease;
            return;
        }

        // New category header?
        if let Some(cat_name) = trimmed.strip_prefix("### ") {
            let cat_name = cat_name.trim();
            if let Some(kind) = CategoryKind::parse_name(cat_name) {
                self.finalize_category();
                self.current_category = Some(Category {
                    kind,
                    entries: Vec::new(),
                    line: self.line_num(),
                });
                return;
            } else {
                self.errors.push(ParseDiagnostic {
                    line: self.line_num(),
                    message: format!(
                        "unknown category `{cat_name}` (expected: {})",
                        CategoryKind::ALL
                            .iter()
                            .map(|k| k.header_text())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
                // Stay in InCategory with None category to consume entries
                self.finalize_category();
                self.current_category = None;
                return;
            }
        }

        // Link reference?
        if let Some(link) = try_parse_link_reference(trimmed, self.line_num()) {
            self.finalize_category();
            self.finalize_release();
            self.links.push(link);
            self.consume_link_references();
            return;
        }

        // Entry?
        if let Some(entry_text) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let ln = self.line_num();
            if let Some(ref mut cat) = self.current_category {
                cat.entries.push(Entry {
                    description: entry_text.to_owned(),
                    continuation_lines: Vec::new(),
                    line: ln,
                });
            }
            return;
        }

        // Continuation line (indented)?
        if (line.starts_with("  ") || line.starts_with('\t')) && !trimmed.is_empty() {
            if let Some(ref mut cat) = self.current_category {
                if let Some(last_entry) = cat.entries.last_mut() {
                    last_entry.continuation_lines.push(trimmed.to_owned());
                    return;
                }
            }
        }

        // Unexpected content
        self.errors.push(ParseDiagnostic {
            line: self.line_num(),
            message: format!(
                "unexpected content in category: {}",
                truncate_for_display(line, 60)
            ),
        });
    }

    fn consume_link_references(&mut self) {
        self.pos += 1;
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos];
            let trimmed = line.trim();

            if trimmed.is_empty() {
                self.pos += 1;
                continue;
            }

            if let Some(link) = try_parse_link_reference(trimmed, self.line_num()) {
                self.links.push(link);
            } else {
                // Non-link content after link references — stop
                self.pos -= 1; // back up so the main loop re-processes this line
                return;
            }
            self.pos += 1;
        }
        // Went past end — back up one so the main loop terminates cleanly
        self.pos -= 1;
    }

    fn finalize_category(&mut self) {
        if let Some(cat) = self.current_category.take() {
            if let Some(ref mut release) = self.current_release {
                release.categories.push(cat);
            }
        }
    }

    fn finalize_release(&mut self) {
        if let Some(release) = self.current_release.take() {
            self.releases.push(release);
        }
        self.state = State::InRelease;
    }
}

/// Try to parse a release header line like `## [1.2.3] - 2026-03-10` or `## [Unreleased]`.
fn try_parse_release_header(trimmed: &str, line_num: usize) -> Option<Release> {
    let rest = trimmed.strip_prefix("## ")?;
    let rest = rest.trim();

    // Must start with [
    let inner = rest.strip_prefix('[')?;

    if let Some(remaining) = inner.strip_prefix("Unreleased]") {
        // Allow trailing whitespace or nothing
        if remaining.trim().is_empty() {
            return Some(Release {
                version: None,
                date: None,
                categories: Vec::new(),
                line: line_num,
            });
        }
        return None;
    }

    // Look for version] - date pattern
    let close_bracket = inner.find(']')?;
    let version_str = &inner[..close_bracket];
    let after_bracket = inner[close_bracket + 1..].trim();

    // Parse version
    let version = semver::Version::parse(version_str).ok()?;

    // Parse date
    let date_str = after_bracket.strip_prefix('-')?.trim();
    if date_str.is_empty() {
        return None;
    }

    Some(Release {
        version: Some(version),
        date: Some(date_str.to_owned()),
        categories: Vec::new(),
        line: line_num,
    })
}

/// Try to parse a link reference line like `[Unreleased]: https://...`.
fn try_parse_link_reference(trimmed: &str, line_num: usize) -> Option<LinkReference> {
    let rest = trimmed.strip_prefix('[')?;
    let close_bracket = rest.find("]:")?;
    let label = &rest[..close_bracket];
    let url = rest[close_bracket + 2..].trim();

    if label.is_empty() || url.is_empty() {
        return None;
    }

    Some(LinkReference {
        label: label.to_owned(),
        url: url.to_owned(),
        line: line_num,
    })
}

/// Truncate a string for display in error messages.
fn truncate_for_display(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_owned()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
