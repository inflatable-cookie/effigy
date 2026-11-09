//! Changelog fragments: one small file per unreleased change under
//! `changelog.d/`, folded into `[Unreleased]` when a release is prepared.
//!
//! This follows the canonical Northstar profile (`changelog.py`): a fragment
//! is `<name>.<type>.md`, where the name is a task or PR reference plus a short
//! slug and the type is one of `added`, `changed`, `fixed`, `removed` or
//! `security`. Its body is one entry: wrapped lines join, and a blank line
//! inside the body is malformed. `.gitkeep` marks an adopted directory and is
//! not an entry.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::error::ValidationDiagnostic;
use super::types::CategoryKind;

/// The conventional fragment directory, adjacent to the changelog.
pub const FRAGMENT_DIR: &str = "changelog.d";

const TYPES: [(&str, CategoryKind); 5] = [
    ("added", CategoryKind::Added),
    ("changed", CategoryKind::Changed),
    ("fixed", CategoryKind::Fixed),
    ("removed", CategoryKind::Removed),
    ("security", CategoryKind::Security),
];

/// One valid fragment that folds into `[Unreleased]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    /// File name under `changelog.d/`, such as `803-ci-bootstrap.fixed.md`.
    pub file_name: String,
    /// Path of the fragment file.
    pub path: PathBuf,
    /// The category this fragment's type names.
    pub kind: CategoryKind,
    /// The entry text: one paragraph with wrapped lines joined.
    pub entry: String,
    /// The file's bytes as read. A later read must match them exactly.
    pub contents: Vec<u8>,
}

/// The fragment directory beside a changelog.
#[derive(Debug, Clone, Default)]
pub struct FragmentSet {
    /// Whether `changelog.d/` exists. An adopted changelog records its
    /// unreleased changes only as fragments.
    pub adopted: bool,
    /// Valid fragments, ordered by type (as the canonical fold lists them) then file name.
    pub fragments: Vec<Fragment>,
    /// Malformed or ambiguous fragments, each naming its file and remedy.
    pub problems: Vec<ValidationDiagnostic>,
}

/// Read the fragment directory beside `changelog_path`.
///
/// A missing directory means the repository has not adopted fragments, and
/// the result is empty and not `adopted`. Nothing is written here.
pub fn load(changelog_path: &Path) -> FragmentSet {
    let dir = changelog_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(FRAGMENT_DIR);
    let mut set = FragmentSet::default();
    match std::fs::symlink_metadata(&dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return set,
        Err(error) => {
            set.adopted = true;
            set.problems.push(path_problem(
                FRAGMENT_DIR,
                &format!("cannot be read ({error})"),
                "make changelog.d a readable directory",
            ));
            return set;
        }
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            set.adopted = true;
            set.problems.push(path_problem(
                FRAGMENT_DIR,
                "is not a real directory",
                "make changelog.d a real directory, not a file or symlink",
            ));
            return set;
        }
        Ok(_) => set.adopted = true,
    }

    let mut names: Vec<OsString> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|e| e.file_name()))
            .collect(),
        Err(error) => {
            set.problems.push(path_problem(
                FRAGMENT_DIR,
                &format!("cannot be listed ({error})"),
                "make changelog.d readable",
            ));
            return set;
        }
    };
    names.sort();

    // `.md` files are the only candidates; `.gitkeep` and other files are not
    // entries. Names compare case-insensitively so a case-only pair is refused
    // as ambiguous rather than ordered by accident.
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    for name in names {
        let Some(file_name) = name.to_str().map(str::to_owned) else {
            set.problems.push(fragment_problem(
                &name.to_string_lossy(),
                "file name is not valid UTF-8",
                "rename the file to ASCII letters, digits, `-` or `_`",
            ));
            continue;
        };
        if file_name.ends_with(".md") {
            candidates.push((file_name.clone(), dir.join(&file_name)));
        }
    }

    let mut by_lower: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (file_name, _) in &candidates {
        by_lower
            .entry(file_name.to_ascii_lowercase())
            .or_default()
            .push(file_name.clone());
    }

    let mut parsed: Vec<Fragment> = Vec::new();
    for (file_name, path) in candidates {
        let collisions = &by_lower[&file_name.to_ascii_lowercase()];
        if collisions.len() > 1 {
            let other = collisions
                .iter()
                .find(|other| **other != file_name)
                .cloned()
                .unwrap_or_default();
            set.problems
                .push(fragment_problem_ambiguous(&file_name, &other));
            continue;
        }
        match read_fragment(&file_name, &path) {
            Ok(fragment) => parsed.push(fragment),
            Err(diagnostic) => set.problems.push(diagnostic),
        }
    }

    // The canonical fold order: by type as `changelog.py` lists them, then by name.
    parsed.sort_by(|a, b| {
        type_position(a.kind)
            .cmp(&type_position(b.kind))
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    set.fragments = parsed;
    set
}

fn read_fragment(file_name: &str, path: &Path) -> Result<Fragment, ValidationDiagnostic> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        fragment_problem(
            file_name,
            &format!("cannot be read ({error})"),
            "restore the file or remove it",
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ValidationDiagnostic {
            line: 0,
            rule: "fragment-path",
            message: format!("{FRAGMENT_DIR}/{file_name} is not a regular file"),
            suggestion: Some("replace it with a regular file or remove it".to_owned()),
        });
    }
    let contents = std::fs::read(path).map_err(|error| {
        fragment_problem(
            file_name,
            &format!("cannot be read ({error})"),
            "restore the file or remove it",
        )
    })?;
    let text = std::str::from_utf8(&contents).map_err(|_| {
        fragment_problem(
            file_name,
            "is not valid UTF-8 text",
            "save the fragment as UTF-8 text",
        )
    })?;

    let stem = &file_name[..file_name.len() - ".md".len()];
    let Some((name, type_name)) = stem.rsplit_once('.') else {
        return Err(fragment_problem(
            file_name,
            "is missing its type",
            "name it <task-or-PR>-<slug>.<type>.md",
        ));
    };
    let Some((_, kind)) = TYPES.iter().find(|(known, _)| *known == type_name) else {
        return Err(fragment_problem(
            file_name,
            &format!("has unknown type `{type_name}`"),
            "use one of added, changed, fixed, removed or security",
        ));
    };
    if !valid_name(name) {
        return Err(fragment_problem(
            file_name,
            "has an invalid name",
            "use a task or PR number and a short slug, with letters, digits, `-` or `_`",
        ));
    }

    let body = text.trim();
    if body.is_empty() {
        return Err(fragment_problem(
            file_name,
            "is empty",
            "write one entry or remove the file",
        ));
    }
    if body.lines().any(|line| line.trim().is_empty()) {
        return Err(fragment_problem(
            file_name,
            "holds more than one paragraph",
            "keep one paragraph here and move the rest into another fragment",
        ));
    }

    Ok(Fragment {
        file_name: file_name.to_owned(),
        path: path.to_owned(),
        kind: *kind,
        entry: body.split_whitespace().collect::<Vec<_>>().join(" "),
        contents,
    })
}

fn type_position(kind: CategoryKind) -> usize {
    TYPES
        .iter()
        .position(|(_, known)| *known == kind)
        .unwrap_or(TYPES.len())
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn fragment_problem(file_name: &str, reason: &str, remedy: &str) -> ValidationDiagnostic {
    ValidationDiagnostic {
        line: 0,
        rule: "fragment-malformed",
        message: format!("{FRAGMENT_DIR}/{file_name} {reason}"),
        suggestion: Some(remedy.to_owned()),
    }
}

fn fragment_problem_ambiguous(file_name: &str, other: &str) -> ValidationDiagnostic {
    ValidationDiagnostic {
        line: 0,
        rule: "fragment-ambiguous",
        message: format!(
            "{FRAGMENT_DIR}/{file_name} has the same name as {FRAGMENT_DIR}/{other} (names compare case-insensitively)"
        ),
        suggestion: Some("rename one of them so every fragment name is unique".to_owned()),
    }
}

fn path_problem(what: &str, reason: &str, remedy: &str) -> ValidationDiagnostic {
    ValidationDiagnostic {
        line: 0,
        rule: "fragment-path",
        message: format!("{what} {reason}"),
        suggestion: Some(remedy.to_owned()),
    }
}

#[cfg(test)]
#[path = "fragments/tests.rs"]
mod tests;
