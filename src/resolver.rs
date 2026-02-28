use std::fs;
use std::path::{Path, PathBuf};

use crate::tasks::ResolutionMode;

const ROOT_MARKERS: [&str; 4] = ["package.json", "composer.json", "Cargo.toml", ".git"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub resolved_root: PathBuf,
    pub resolution_mode: ResolutionMode,
    pub evidence: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum ResolveError {
    Cwd(std::io::Error),
    InvalidExplicitRoot { path: PathBuf },
    NoCandidateRoot { cwd: PathBuf },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Cwd(err) => write!(f, "failed to resolve current directory: {err}"),
            ResolveError::InvalidExplicitRoot { path } => {
                write!(
                    f,
                    "explicit --repo path is not a directory: {}",
                    path.display()
                )
            }
            ResolveError::NoCandidateRoot { cwd } => write!(
                f,
                "could not resolve a project root from cwd {} (use --repo <path>)",
                cwd.display()
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

pub fn resolve_target_root(
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
) -> Result<ResolvedTarget, ResolveError> {
    if let Some(explicit) = repo_override {
        let canonical = canonicalize_best_effort(explicit);
        if !canonical.is_dir() {
            return Err(ResolveError::InvalidExplicitRoot { path: canonical });
        }
        return Ok(ResolvedTarget {
            resolved_root: canonical,
            resolution_mode: ResolutionMode::Explicit,
            evidence: vec!["resolved via explicit --repo override".to_owned()],
            warnings: Vec::new(),
        });
    }

    let nearest = find_nearest_candidate(&cwd).ok_or(ResolveError::NoCandidateRoot { cwd })?;

    if let Some(promoted) = maybe_promote_to_parent_workspace(&nearest) {
        return Ok(promoted);
    }

    Ok(ResolvedTarget {
        resolved_root: nearest.clone(),
        resolution_mode: ResolutionMode::AutoNearest,
        evidence: vec![format!(
            "selected nearest root candidate {}",
            nearest.display()
        )],
        warnings: Vec::new(),
    })
}

fn find_nearest_candidate(cwd: &Path) -> Option<PathBuf> {
    let mut current = Some(canonicalize_best_effort(cwd.to_path_buf()));
    while let Some(path) = current {
        if is_candidate_root(&path) {
            return Some(path);
        }
        current = path.parent().map(Path::to_path_buf);
    }
    None
}

fn is_candidate_root(path: &Path) -> bool {
    ROOT_MARKERS.iter().any(|marker| path.join(marker).exists())
}

fn maybe_promote_to_parent_workspace(child: &Path) -> Option<ResolvedTarget> {
    let parent = child.parent()?;
    if !parent.is_dir() {
        return None;
    }

    let child_name = child.file_name()?.to_string_lossy().to_string();

    let mut evidence: Vec<String> = Vec::new();
    let mut should_promote = false;

    let parent_package = parent.join("package.json");
    if parent_package.exists() {
        let content = read_to_string(&parent_package);
        if content.contains("\"workspaces\"")
            && (content.contains(&child_name) || content.contains('*'))
        {
            should_promote = true;
            evidence.push("parent package.json workspace includes child".to_owned());
        }
    }

    let parent_cargo = parent.join("Cargo.toml");
    if parent_cargo.exists() {
        let content = read_to_string(&parent_cargo);
        if content.contains("[workspace]")
            && content.contains("members")
            && (content.contains(&child_name) || content.contains('*'))
        {
            should_promote = true;
            evidence.push("parent Cargo.toml workspace includes child".to_owned());
        }
    }

    if !should_promote {
        return None;
    }

    let child_has_own_git = child.join(".git").exists();
    if child_has_own_git {
        return Some(ResolvedTarget {
            resolved_root: child.to_path_buf(),
            resolution_mode: ResolutionMode::AutoNearest,
            evidence: vec![format!(
                "child repo {} has standalone .git; kept nearest root",
                child.display()
            )],
            warnings: vec![
                "workspace promotion skipped due to standalone child repository".to_owned(),
            ],
        });
    }

    Some(ResolvedTarget {
        resolved_root: parent.to_path_buf(),
        resolution_mode: ResolutionMode::AutoPromoted,
        evidence,
        warnings: Vec::new(),
    })
}

fn read_to_string(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn canonicalize_best_effort(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

#[cfg(test)]
#[path = "tests/resolver_tests.rs"]
mod tests;
