use std::path::{Path, PathBuf};

use crate::error::CodeGraphError;
use crate::scope::GraphScope;

pub const GRAPH_DIR_NAME: &str = ".effigy/graph";
pub const GRAPH_DB_FILE_NAME: &str = "graph.db";
pub const REFRESH_LOCK_FILE_NAME: &str = "refresh.lock";
/// Root-owned directory holding one physical database per independent catalog.
pub const CATALOG_GRAPH_DIR_NAME: &str = "catalogs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPaths {
    pub repo_root: PathBuf,
    pub graph_dir: PathBuf,
    pub db_path: PathBuf,
    pub refresh_lock_path: PathBuf,
}

impl GraphPaths {
    pub fn for_repo(repo_root: &Path) -> Self {
        let graph_dir = repo_root.join(GRAPH_DIR_NAME);
        let db_path = graph_dir.join(GRAPH_DB_FILE_NAME);
        let refresh_lock_path = graph_dir.join(REFRESH_LOCK_FILE_NAME);
        Self {
            repo_root: repo_root.to_path_buf(),
            graph_dir,
            db_path,
            refresh_lock_path,
        }
    }

    /// Storage paths for one scope.
    ///
    /// A segmented catalog with `independent = true` owns a deterministic
    /// database and lock under the root-owned graph directory; every other
    /// scope shares the root database. Manifests cannot supply database paths.
    pub fn for_scope(scope: &GraphScope) -> Self {
        if !scope.independent() {
            return Self::for_repo(scope.workspace_root());
        }
        // Aliases are validated and encoded at scope build time, so the encode
        // here cannot fail for a scope that reached storage; fall back to the
        // shared database rather than inventing a path.
        let Ok(encoded) = encode_alias(scope.alias()) else {
            return Self::for_repo(scope.workspace_root());
        };
        let graph_dir = scope
            .workspace_root()
            .join(GRAPH_DIR_NAME)
            .join(CATALOG_GRAPH_DIR_NAME)
            .join(encoded);
        let db_path = graph_dir.join(GRAPH_DB_FILE_NAME);
        let refresh_lock_path = graph_dir.join(REFRESH_LOCK_FILE_NAME);
        Self {
            repo_root: scope.workspace_root().to_path_buf(),
            graph_dir,
            db_path,
            refresh_lock_path,
        }
    }
}

/// Deterministic, traversal-safe encoding of a catalog alias for storage.
///
/// Every byte outside `[A-Za-z0-9._-]` is percent-encoded, and `%` itself is
/// always encoded, so the mapping is injective: two distinct aliases can never
/// share a database directory. `.` and `..` are rejected outright so an alias
/// can never name a parent directory.
pub fn encode_alias(alias: &str) -> Result<String, CodeGraphError> {
    if alias.is_empty() || alias == "." || alias == ".." {
        return Err(CodeGraphError::validation(format!(
            "catalog alias `{alias}` cannot be encoded as a graph storage directory"
        )));
    }
    let mut encoded = String::with_capacity(alias.len());
    for byte in alias.bytes() {
        let safe = byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_');
        if safe {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    if encoded == "." || encoded == ".." || encoded.is_empty() {
        return Err(CodeGraphError::validation(format!(
            "catalog alias `{alias}` cannot be encoded as a graph storage directory"
        )));
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::encode_alias;

    #[test]
    fn alias_encoding_is_deterministic_and_traversal_safe() {
        assert_eq!(encode_alias("bovine-desktop").unwrap(), "bovine-desktop");
        assert_eq!(encode_alias("apps/bovine").unwrap(), "apps%2Fbovine");
        assert_eq!(encode_alias("a%b").unwrap(), "a%25b");
        assert!(encode_alias("..").is_err());
        assert!(encode_alias(".").is_err());
        assert!(encode_alias("").is_err());
    }

    #[test]
    fn alias_encoding_never_collides_across_distinct_aliases() {
        let aliases = ["a/b", "a%2Fb", "a b", "a-b", "a_b", "a.b"];
        let mut encoded = aliases
            .iter()
            .map(|alias| encode_alias(alias).unwrap())
            .collect::<Vec<_>>();
        encoded.sort();
        encoded.dedup();
        assert_eq!(encoded.len(), aliases.len());
    }
}
