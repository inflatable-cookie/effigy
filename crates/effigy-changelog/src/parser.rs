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
    // Whether the last release-level content was a prose paragraph that the
    // next prose line continues. A blank line or structural line resets it.
    prose_paragraph_open: bool,
    // Whether the last category entry's paragraph is still open, so a
    // CommonMark lazy continuation line belongs to that entry. A blank line or
    // structural line closes it.
    entry_paragraph_open: bool,
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
            prose_paragraph_open: false,
            entry_paragraph_open: false,
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
            self.errors
                .push(release_header_diagnostic(trimmed, self.line_num()));
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
            self.prose_paragraph_open = false;
            return;
        }

        // New release header?
        if let Some(release) = try_parse_release_header(trimmed, self.line_num()) {
            self.prose_paragraph_open = false;
            self.finalize_category();
            self.finalize_release();
            self.current_release = Some(release);
            self.state = State::InRelease;
            return;
        }

        // Category header?
        if let Some(cat_name) = trimmed.strip_prefix("### ") {
            self.prose_paragraph_open = false;
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
            self.prose_paragraph_open = false;
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

        // Any other Markdown list item is a malformed entry, not prose.
        if unsupported_list_marker(trimmed) {
            self.errors
                .push(unsupported_entry_diagnostic(line, self.line_num()));
            return;
        }

        // A malformed `## ` line is a release-header problem, not prose.
        if trimmed.starts_with("## ") {
            self.errors
                .push(release_header_diagnostic(trimmed, self.line_num()));
            return;
        }

        // Any other heading-like line is still malformed structure, not prose.
        if trimmed.starts_with('#') {
            self.errors.push(ParseDiagnostic {
                line: self.line_num(),
                message: format!("unexpected content: {}", truncate_for_display(line, 60)),
            });
            return;
        }

        // Free prose paragraph: allowed directly under a release heading and
        // between category headings. Bump analysis still counts entries only.
        // A valid bullet outside a category is already rejected above; any
        // other non-heading line, including a bullet-looking line without the
        // required space, is prose.
        self.push_release_prose(trimmed, line);
    }

    fn handle_in_category(&mut self, trimmed: &str, line: &str) {
        if trimmed.is_empty() {
            self.prose_paragraph_open = false;
            self.entry_paragraph_open = false;
            return;
        }

        // New release header?
        if let Some(release) = try_parse_release_header(trimmed, self.line_num()) {
            self.prose_paragraph_open = false;
            self.entry_paragraph_open = false;
            self.finalize_category();
            self.finalize_release();
            self.current_release = Some(release);
            self.state = State::InRelease;
            return;
        }

        // New category header?
        if let Some(cat_name) = trimmed.strip_prefix("### ") {
            self.prose_paragraph_open = false;
            self.entry_paragraph_open = false;
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
            self.prose_paragraph_open = false;
            self.entry_paragraph_open = false;
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
            self.entry_paragraph_open = true;
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
            // An indented unsupported list marker is malformed nested entry
            // syntax, not opaque continuation text. Accepted `- `/`* `
            // markers and other indented text stay continuation content.
            if unsupported_list_marker(trimmed) {
                self.errors
                    .push(unsupported_entry_diagnostic(line, self.line_num()));
                return;
            }
            if let Some(ref mut cat) = self.current_category {
                if let Some(last_entry) = cat.entries.last_mut() {
                    last_entry.continuation_lines.push(trimmed.to_owned());
                    return;
                }
            }
        }

        // Any other Markdown list item is a malformed entry, not prose.
        if unsupported_list_marker(trimmed) {
            self.errors
                .push(unsupported_entry_diagnostic(line, self.line_num()));
            return;
        }

        // A malformed `## ` line is a release-header problem, not stray prose.
        if trimmed.starts_with("## ") {
            self.errors
                .push(release_header_diagnostic(trimmed, self.line_num()));
            return;
        }

        // Any other heading-like line stays malformed structure, not prose.
        if trimmed.starts_with('#') {
            self.errors.push(ParseDiagnostic {
                line: self.line_num(),
                message: format!(
                    "unexpected content in category: {}",
                    truncate_for_display(line, 60)
                ),
            });
            return;
        }

        // CommonMark lazy continuation: an unindented paragraph line directly
        // after an entry continues that entry, because the list item's
        // paragraph stays open until a blank line or a structural line. This
        // is the shape a wrapped changelog entry takes after `format` rewraps
        // prose, and refusing it made a released tag unreadable.
        if self.entry_paragraph_open {
            if let Some(ref mut cat) = self.current_category {
                if let Some(last_entry) = cat.entries.last_mut() {
                    last_entry.continuation_lines.push(trimmed.to_owned());
                    return;
                }
            }
        }

        // Free prose paragraph between category headings. It belongs to the
        // release, so close the open category before recording it.
        self.finalize_category();
        self.prose_paragraph_open = false;
        self.entry_paragraph_open = false;
        self.state = State::InRelease;
        self.push_release_prose(trimmed, line);
    }

    /// Record a free prose line on the release currently being built.
    ///
    /// Consecutive prose lines with no blank line between them stay in one
    /// paragraph; a blank line or any structural line starts a new one. Prose
    /// that arrives after the link references has no release to belong to and
    /// stays an error.
    fn push_release_prose(&mut self, trimmed: &str, line: &str) {
        let Some(release) = self.current_release.as_mut() else {
            self.errors.push(ParseDiagnostic {
                line: self.line_num(),
                message: format!("unexpected content: {}", truncate_for_display(line, 60)),
            });
            return;
        };

        if self.prose_paragraph_open {
            if let Some(paragraph) = release.prose.last_mut() {
                paragraph.push('\n');
                paragraph.push_str(trimmed);
                return;
            }
        }

        release.prose.push(trimmed.to_owned());
        self.prose_paragraph_open = true;
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
                prose: Vec::new(),
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
        prose: Vec::new(),
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

/// Diagnostics for a `## ` line that is not a valid release header.
///
/// The Northstar profile separates the version and the date with ASCII ` - `.
/// The old message was only "invalid release header", so an em dash read as
/// an unexplained rejection. Naming the offending separator makes docs QA and
/// `release status` agree on what to change without widening what parses.
fn release_header_diagnostic(trimmed: &str, line_num: usize) -> ParseDiagnostic {
    let display = truncate_for_display(trimmed, 60);
    if let Some(separator) = non_ascii_release_separator(trimmed) {
        return ParseDiagnostic {
            line: line_num,
            message: format!(
                "invalid release header: {display} (separate the version and date with ASCII \
                 ` - `, not `{separator}`)"
            ),
        };
    }
    ParseDiagnostic {
        line: line_num,
        message: format!(
            "invalid release header: {display} (expected `## [Unreleased]` or \
             `## [X.Y.Z] - YYYY-MM-DD`)"
        ),
    }
}

/// Return the separator when a bracketed release header uses a non-ASCII dash
/// where the profile requires ASCII ` - `.
fn non_ascii_release_separator(trimmed: &str) -> Option<char> {
    let rest = trimmed.strip_prefix("## ")?;
    let inner = rest.trim().strip_prefix('[')?;
    let close_bracket = inner.find(']')?;
    if semver::Version::parse(&inner[..close_bracket]).is_err() {
        return None;
    }
    let separator = inner[close_bracket + 1..].trim_start().chars().next()?;
    is_dash_like(separator).then_some(separator)
}

/// Whether `trimmed` opens a Markdown list item the Northstar profile does not
/// accept as a changelog entry.
///
/// Entries use `- ` or `* `. CommonMark also treats `+`, and ordered markers
/// such as `1.` or `1)`, as list items. Routing those lines into release prose
/// would let malformed entry syntax pass validation, so they stay errors. The
/// marker must be followed by a space, a tab, or end of line, which keeps
/// ordinary paragraphs such as `**Note:** ...` or `-like word` as prose.
fn unsupported_list_marker(trimmed: &str) -> bool {
    if matches!(trimmed, "+" | "-" | "*") || trimmed.starts_with("+ ") || trimmed.starts_with("+\t")
    {
        return true;
    }

    // CommonMark ordered list markers are 1-9 ASCII digits followed by `.` or
    // `)` and then a space, a tab, or the end of the line.
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if !(1..=9).contains(&digits) {
        return false;
    }
    let rest = &trimmed[digits..];
    let Some(after) = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')')) else {
        return false;
    };
    after.is_empty() || after.starts_with(' ') || after.starts_with('\t')
}

/// Diagnostic for a line that opens a Markdown list item the profile does not
/// accept as an entry.
fn unsupported_entry_diagnostic(line: &str, line_num: usize) -> ParseDiagnostic {
    ParseDiagnostic {
        line: line_num,
        message: format!(
            "unsupported entry marker (entries must use `- ` or `* `): {}",
            truncate_for_display(line, 60)
        ),
    }
}

/// Whether `c` is dash-like enough to be mistaken for the ASCII `-` separator.
fn is_dash_like(c: char) -> bool {
    matches!(
        c,
        '\u{2010}' // hyphen
            | '\u{2011}' // non-breaking hyphen
            | '\u{2012}' // figure dash
            | '\u{2013}' // en dash
            | '\u{2014}' // em dash
            | '\u{2015}' // horizontal bar
            | '\u{2212}' // minus sign
            | '\u{fe58}'
            | '\u{fe63}'
            | '\u{ff0d}'
    )
}

/// Truncate a string for display in error messages.
///
/// Counts characters rather than bytes: slicing a multi-byte boundary would
/// panic on exactly the non-ASCII input these messages describe.
fn truncate_for_display(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_owned();
    }
    let truncated: String = s.chars().take(max_len).collect();
    format!("{truncated}...")
}

#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
