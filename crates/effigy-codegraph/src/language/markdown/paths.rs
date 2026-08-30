use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::extractor::SourceFile;
use crate::support::normalize_rel_path;

pub(super) fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_owned()
}

pub(super) fn heading_level_number(level: pulldown_cmark::HeadingLevel) -> u8 {
    match level {
        pulldown_cmark::HeadingLevel::H1 => 1,
        pulldown_cmark::HeadingLevel::H2 => 2,
        pulldown_cmark::HeadingLevel::H3 => 3,
        pulldown_cmark::HeadingLevel::H4 => 4,
        pulldown_cmark::HeadingLevel::H5 => 5,
        pulldown_cmark::HeadingLevel::H6 => 6,
    }
}

pub(super) fn code_fence_language(info: &str) -> &str {
    info.split(|ch: char| ch.is_whitespace() || ch == ',')
        .find(|part| !part.is_empty())
        .unwrap_or("text")
}

pub(super) fn local_path_references(file: &SourceFile, text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in path_candidates(text) {
        if let Some(path) = resolve_local_path(file, &candidate) {
            if seen.insert(path.clone()) {
                refs.push(path);
            }
        }
    }
    refs
}

fn path_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for token in text.split_whitespace() {
        let candidate = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | ':'
            )
        });
        if candidate.is_empty() || candidate.contains("://") {
            continue;
        }
        if candidate.contains('/')
            || [
                ".rs", ".md", ".toml", ".rhai", ".ts", ".tsx", ".js", ".jsx", ".php", ".json",
                ".yaml", ".yml", ".sh",
            ]
            .iter()
            .any(|ext| candidate.ends_with(ext))
        {
            candidates.push(candidate.to_owned());
        }
    }
    candidates
}

pub(super) fn resolve_local_path(file: &SourceFile, target: &str) -> Option<String> {
    let path_part = target.split('#').next().unwrap_or(target).trim();
    if path_part.is_empty() {
        return None;
    }
    let absolute = if path_part.starts_with('/') {
        normalize_repo_path(file.repo_root.join(path_part.trim_start_matches('/')))
    } else {
        let base = Path::new(&file.relative_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        normalize_repo_path(file.repo_root.join(base).join(path_part))
    };
    let relative = absolute.strip_prefix(&file.repo_root).ok()?;
    let normalized = normalize_rel_path(relative);
    let full_path = file.repo_root.join(&normalized);
    if full_path.is_file() {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_repo_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
