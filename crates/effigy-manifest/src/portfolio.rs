//! Portfolio files: the caller-owned half of two-sided source membership.
//!
//! A portfolio names directories to look in; each repository still decides
//! whether it wants to be found (`[docs_policy.sources] share = true`). The
//! grammar is deliberately tiny — one table, one list of relative directory
//! paths, unknown keys rejected — because widening it is how a naming file
//! turns into a crawler.

use std::path::{Component, Path, PathBuf};

use crate::ManifestError;

/// File name used when `--sources` names a directory instead of a file.
pub const PORTFOLIO_FILE: &str = "portfolio.toml";

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PortfolioDocument {
    portfolio: PortfolioTable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
struct PortfolioTable {
    #[serde(default)]
    directories: Vec<String>,
}

/// One resolved portfolio: the file it came from and the directories it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Portfolio {
    /// The portfolio file, or the directory that was passed directly.
    pub source: PathBuf,
    /// Declared entries in file order, as written.
    pub declared: Vec<String>,
    /// Declared entries resolved against the portfolio file's directory.
    ///
    /// Resolution is lexical only: a directory that does not exist is reported
    /// per repository as `missing`, never as a parse failure, so one absent
    /// checkout cannot silence every healthy one.
    pub directories: Vec<PathBuf>,
}

/// Load a portfolio from a file, or from a directory that stands for a
/// portfolio naming exactly that one directory.
///
/// Errors are usage errors: a caller that named a portfolio wants that
/// portfolio, and silently answering from a different set of repositories
/// would be worse than failing.
pub fn load_portfolio(path: &Path) -> Result<Portfolio, ManifestError> {
    if path.is_dir() {
        let named = path.join(PORTFOLIO_FILE);
        if named.is_file() {
            return load_portfolio_file(&named);
        }
        return Ok(Portfolio {
            source: path.to_path_buf(),
            declared: vec![".".to_owned()],
            directories: vec![path.to_path_buf()],
        });
    }
    if !path.is_file() {
        return Err(ManifestError::Compose {
            path: path.to_path_buf(),
            detail: "portfolio file does not exist; pass a `[portfolio]` file or a directory"
                .to_owned(),
        });
    }
    load_portfolio_file(path)
}

fn load_portfolio_file(path: &Path) -> Result<Portfolio, ManifestError> {
    let text = std::fs::read_to_string(path).map_err(|error| ManifestError::Read {
        path: path.to_path_buf(),
        error,
    })?;
    let document: PortfolioDocument =
        toml::from_str(&text).map_err(|error| ManifestError::Parse {
            path: path.to_path_buf(),
            error,
        })?;
    if document.portfolio.directories.is_empty() {
        return Err(portfolio_error(
            path,
            "`portfolio.directories` must name at least one directory",
        ));
    }

    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut declared = Vec::new();
    let mut directories = Vec::new();
    for (index, entry) in document.portfolio.directories.iter().enumerate() {
        let trimmed = entry.trim();
        validate_directory_entry(path, index, trimmed)?;
        declared.push(trimmed.to_owned());
        directories.push(normalize(&base.join(trimmed)));
    }

    Ok(Portfolio {
        source: path.to_path_buf(),
        declared,
        directories,
    })
}

/// Reject anything that would turn naming into crawling: empty entries, glob
/// metacharacters, and `..` escapes above the portfolio file.
fn validate_directory_entry(path: &Path, index: usize, entry: &str) -> Result<(), ManifestError> {
    let field = format!("portfolio.directories[{index}]");
    if entry.is_empty() {
        return Err(portfolio_error(
            path,
            format!("`{field}` must be a non-empty directory path"),
        ));
    }
    if entry.contains(['*', '?', '[', ']']) {
        return Err(portfolio_error(
            path,
            format!("`{field}` must name one directory; globs are not supported"),
        ));
    }
    if Path::new(entry)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(portfolio_error(
            path,
            format!("`{field}` must not escape the portfolio file with `..`"),
        ));
    }
    Ok(())
}

/// Drop `.` segments so the reported path reads as the operator wrote it.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

fn portfolio_error(path: &Path, detail: impl Into<String>) -> ManifestError {
    ManifestError::Compose {
        path: path.to_path_buf(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("effigy-portfolio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn directories_resolve_against_the_portfolio_file() {
        let dir = temp_dir("resolve");
        let file = dir.join("portfolio.toml");
        std::fs::write(&file, "[portfolio]\ndirectories = [\".\", \"projects\"]\n").expect("write");
        let portfolio = load_portfolio(&file).expect("portfolio");
        assert_eq!(
            portfolio.declared,
            vec![".".to_owned(), "projects".to_owned()]
        );
        assert_eq!(
            portfolio.directories,
            vec![dir.clone(), dir.join("projects")]
        );
    }

    #[test]
    fn a_missing_directory_is_not_a_parse_failure() {
        let dir = temp_dir("missing");
        let file = dir.join("portfolio.toml");
        std::fs::write(&file, "[portfolio]\ndirectories = [\"absent\"]\n").expect("write");
        let portfolio = load_portfolio(&file).expect("portfolio");
        assert_eq!(portfolio.directories, vec![dir.join("absent")]);
        assert!(!dir.join("absent").exists());
    }

    #[test]
    fn unknown_keys_globs_and_escapes_are_rejected() {
        let dir = temp_dir("reject");
        let unknown = dir.join("unknown.toml");
        std::fs::write(&unknown, "[portfolio]\ndirectories = [\".\"]\ndepth = 2\n").expect("write");
        assert!(load_portfolio(&unknown).is_err());

        let glob = dir.join("glob.toml");
        std::fs::write(&glob, "[portfolio]\ndirectories = [\"projects/*\"]\n").expect("write");
        assert!(load_portfolio(&glob).is_err());

        let escape = dir.join("escape.toml");
        std::fs::write(&escape, "[portfolio]\ndirectories = [\"../elsewhere\"]\n").expect("write");
        assert!(load_portfolio(&escape).is_err());

        let empty = dir.join("empty.toml");
        std::fs::write(&empty, "[portfolio]\ndirectories = []\n").expect("write");
        assert!(load_portfolio(&empty).is_err());
    }

    #[test]
    fn a_directory_stands_for_a_portfolio_naming_itself() {
        let dir = temp_dir("directory");
        let portfolio = load_portfolio(&dir).expect("portfolio");
        assert_eq!(portfolio.declared, vec![".".to_owned()]);
        assert_eq!(portfolio.directories, vec![dir.clone()]);

        std::fs::write(
            dir.join("portfolio.toml"),
            "[portfolio]\ndirectories = [\"repos\"]\n",
        )
        .expect("write");
        let named = load_portfolio(&dir).expect("portfolio");
        assert_eq!(named.directories, vec![dir.join("repos")]);
    }
}
