//! Northstar Changelog Profile validation.
//!
//! Validates a parsed changelog against the Northstar Profile rules:
//! no empty sections, valid dates, valid semver, no duplicates, etc.

use std::collections::HashSet;

use super::error::ValidationDiagnostic;
use super::types::Changelog;

/// Validate a changelog against the Northstar Profile.
///
/// Takes both the parsed AST and the raw content (for spacing checks
/// that require access to the original text).
///
/// Returns a list of diagnostics. An empty list means the changelog
/// is fully compliant.
pub(super) fn validate_changelog(
    changelog: &Changelog,
    raw_content: &str,
) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();

    check_unreleased_present(changelog, &mut diagnostics);
    check_empty_sections(changelog, &mut diagnostics);
    check_duplicate_versions(changelog, &mut diagnostics);
    check_version_ordering(changelog, &mut diagnostics);
    check_date_validity(changelog, &mut diagnostics);
    check_spacing(raw_content, &mut diagnostics);
    check_trailing_blank_lines(raw_content, &mut diagnostics);

    diagnostics
}

fn check_unreleased_present(changelog: &Changelog, diagnostics: &mut Vec<ValidationDiagnostic>) {
    if changelog.unreleased().is_none() {
        diagnostics.push(ValidationDiagnostic {
            line: 0,
            rule: "missing-unreleased",
            message: "changelog must contain an `## [Unreleased]` section".to_owned(),
            suggestion: Some("add `## [Unreleased]` after the preamble".to_owned()),
        });
    }
}

fn check_empty_sections(changelog: &Changelog, diagnostics: &mut Vec<ValidationDiagnostic>) {
    for release in &changelog.releases {
        for cat in &release.categories {
            if cat.entries.is_empty() {
                diagnostics.push(ValidationDiagnostic {
                    line: cat.line,
                    rule: "empty-section",
                    message: format!(
                        "category `### {}` has no entries and should be removed",
                        cat.kind
                    ),
                    suggestion: Some(format!("remove the `### {}` section", cat.kind)),
                });
            }
        }
    }
}

fn check_duplicate_versions(changelog: &Changelog, diagnostics: &mut Vec<ValidationDiagnostic>) {
    let mut seen = HashSet::new();
    for release in &changelog.releases {
        if let Some(ref version) = release.version {
            let key = version.to_string();
            if !seen.insert(key.clone()) {
                diagnostics.push(ValidationDiagnostic {
                    line: release.line,
                    rule: "duplicate-version",
                    message: format!("version `{key}` appears more than once"),
                    suggestion: None,
                });
            }
        }
    }
}

fn check_version_ordering(changelog: &Changelog, diagnostics: &mut Vec<ValidationDiagnostic>) {
    let versions: Vec<_> = changelog
        .releases
        .iter()
        .filter_map(|r| r.version.as_ref().map(|v| (v, r.line)))
        .collect();

    for window in versions.windows(2) {
        let (prev_version, _) = &window[0];
        let (curr_version, curr_line) = &window[1];
        if curr_version > prev_version {
            diagnostics.push(ValidationDiagnostic {
                line: *curr_line,
                rule: "version-ordering",
                message: format!(
                    "version `{}` should appear before `{}` (descending order)",
                    curr_version, prev_version
                ),
                suggestion: Some("reorder releases from newest to oldest".to_owned()),
            });
        }
    }
}

fn check_date_validity(changelog: &Changelog, diagnostics: &mut Vec<ValidationDiagnostic>) {
    for release in &changelog.releases {
        if let Some(ref date_str) = release.date {
            if !is_valid_iso_date(date_str) {
                diagnostics.push(ValidationDiagnostic {
                    line: release.line,
                    rule: "invalid-date",
                    message: format!("date `{date_str}` is not a valid ISO 8601 date (YYYY-MM-DD)"),
                    suggestion: Some("use format YYYY-MM-DD with a valid calendar date".to_owned()),
                });
            }
        }
    }
}

fn check_spacing(raw_content: &str, diagnostics: &mut Vec<ValidationDiagnostic>) {
    let lines: Vec<&str> = raw_content.lines().collect();
    let mut consecutive_blanks = 0;

    for (idx, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            consecutive_blanks += 1;
            if consecutive_blanks > 1 {
                diagnostics.push(ValidationDiagnostic {
                    line: idx + 1,
                    rule: "extra-blank-line",
                    message: "more than one consecutive blank line".to_owned(),
                    suggestion: Some("use exactly one blank line between sections".to_owned()),
                });
            }
        } else {
            consecutive_blanks = 0;
        }
    }
}

fn check_trailing_blank_lines(raw_content: &str, diagnostics: &mut Vec<ValidationDiagnostic>) {
    if raw_content.ends_with("\n\n") {
        let line_count = raw_content.lines().count();
        diagnostics.push(ValidationDiagnostic {
            line: line_count,
            rule: "trailing-blank-lines",
            message: "file has trailing blank lines".to_owned(),
            suggestion: Some("remove trailing blank lines".to_owned()),
        });
    }
}

/// Validate a date string as a valid ISO 8601 calendar date.
fn is_valid_iso_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return false;
    }

    let year: u32 = match parts[0].parse() {
        Ok(y) if parts[0].len() == 4 => y,
        _ => return false,
    };
    let month: u32 = match parts[1].parse() {
        Ok(m) if parts[1].len() == 2 && (1..=12).contains(&m) => m,
        _ => return false,
    };
    let day: u32 = match parts[2].parse() {
        Ok(d) if parts[2].len() == 2 && d >= 1 => d,
        _ => return false,
    };

    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => return false,
    };

    day <= max_day
}

fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
#[path = "validator/tests.rs"]
mod tests;
