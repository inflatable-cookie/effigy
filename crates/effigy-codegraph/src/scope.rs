//! Catalog-derived code-graph scopes.
//!
//! A graph scope is a stable, path-derived view of the workspace corpus. It
//! attaches an indexing posture to an existing effective catalog instead of
//! introducing a second segment registry: catalog membership and aliases stay
//! the only topology and identity source.
//!
//! Three scope kinds exist:
//!
//! - the **root** scope: workspace root source plus folded member trees, with
//!   every segmented member root pruned before descent;
//! - a **catalog** scope: one segmented catalog's own tree, with any segmented
//!   descendants pruned; `independent` only changes physical storage;
//! - the **workspace** scope: the repository-owned corpus used by
//!   `effigy docs context`, which never prunes catalog roots.
//!
//! Scope membership is derived from canonical repository-relative paths, so a
//! record's owning scope is stable no matter which scope last refreshed it.
//! That is what lets a shared database hold several scopes without any scope
//! deleting, ranking, or refreshing another's records.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_manifest::LoadedCatalog;

use crate::error::CodeGraphError;
use crate::paths::GraphPaths;

/// Why a single scope was selected. Reported additively in graph JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphScopeSelection {
    /// `--catalog <alias>` was explicit.
    Explicit,
    /// The invocation cwd sits inside a segmented catalog.
    Cwd,
    /// No selector matched; the root scope applies.
    Root,
}

impl GraphScopeSelection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Cwd => "cwd",
            Self::Root => "root",
        }
    }
}

/// What a graph invocation asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphScopeRequest {
    /// Select by invocation cwd, falling back to the root scope.
    Cwd,
    /// Select exactly one effective segmented catalog by alias.
    Catalog(String),
    /// Explicit fan-out over the root scope and every segmented catalog.
    AllCatalogs,
}

/// The scopes a graph invocation will operate on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphScopePlan {
    /// One scope, selected deterministically.
    Single(GraphScope),
    /// Explicit fan-out: root scope first, then segmented catalogs in
    /// repository-relative order.
    FanOut(Vec<GraphScope>),
}

/// A path-derived graph corpus plus its storage posture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphScope {
    workspace_root: PathBuf,
    alias: String,
    catalog_root: PathBuf,
    relative_root: String,
    key: String,
    selection: GraphScopeSelection,
    segmented: bool,
    independent: bool,
    workspace: bool,
    prune_relative_roots: Vec<String>,
}

impl GraphScope {
    /// The root scope without catalog topology.
    ///
    /// This is the repository-owned corpus used by direct crate consumers and
    /// by repositories with no effective catalog membership. It shares the
    /// root scope's identity and shared database, so existing single-catalog
    /// graphs keep their stamps, paths, and behavior.
    pub fn repo_root(workspace_root: &Path) -> Result<Self, CodeGraphError> {
        let workspace_root = canonical_workspace_root(workspace_root)?;
        let alias = default_root_alias(&workspace_root);
        Ok(Self {
            catalog_root: workspace_root.clone(),
            workspace_root,
            alias,
            relative_root: String::new(),
            key: String::new(),
            selection: GraphScopeSelection::Root,
            segmented: false,
            independent: false,
            workspace: false,
            prune_relative_roots: Vec::new(),
        })
    }

    /// The repository root scope. Prunes every segmented catalog root.
    pub fn root(workspace_root: &Path, catalogs: &[LoadedCatalog]) -> Result<Self, CodeGraphError> {
        let scopes = build_scopes(workspace_root, catalogs)?;
        scopes
            .into_iter()
            .find(|scope| scope.is_root())
            .ok_or_else(|| CodeGraphError::validation("workspace has no root graph scope"))
    }

    /// The repository-owned documentation corpus.
    ///
    /// `effigy docs context` keeps `[docs_policy.graph]` authority: it indexes
    /// the whole repository corpus and never prunes a configured root, and it
    /// never opens an independent catalog database.
    pub fn workspace(workspace_root: &Path) -> Result<Self, CodeGraphError> {
        let workspace_root = canonical_workspace_root(workspace_root)?;
        Ok(Self {
            workspace_root: workspace_root.clone(),
            alias: WORKSPACE_ALIAS.to_owned(),
            catalog_root: workspace_root,
            relative_root: String::new(),
            key: WORKSPACE_SCOPE_KEY.to_owned(),
            selection: GraphScopeSelection::Root,
            segmented: false,
            independent: false,
            workspace: true,
            prune_relative_roots: Vec::new(),
        })
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn catalog_root(&self) -> &Path {
        &self.catalog_root
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn relative_root(&self) -> &str {
        &self.relative_root
    }

    /// Stable identity of this scope inside one workspace's shared store.
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn selection(&self) -> GraphScopeSelection {
        self.selection
    }

    pub fn segmented(&self) -> bool {
        self.segmented
    }

    pub fn independent(&self) -> bool {
        self.independent
    }

    /// The root scope: workspace-owned source plus folded members.
    pub fn is_root(&self) -> bool {
        !self.workspace && self.relative_root.is_empty()
    }

    /// The repository-owned documentation corpus scope.
    pub fn is_workspace(&self) -> bool {
        self.workspace
    }

    pub fn paths(&self) -> GraphPaths {
        GraphPaths::for_scope(self)
    }

    /// Absolute roots the walker must prune before descending.
    pub fn prune_absolute_roots(&self) -> Vec<PathBuf> {
        self.prune_relative_roots
            .iter()
            .map(|relative| self.workspace_root.join(relative))
            .collect()
    }

    /// Whether a repository-relative path belongs to this scope.
    pub fn contains_relative(&self, relative_path: &str) -> bool {
        if !self.workspace && !path_under(relative_path, &self.relative_root) {
            return false;
        }
        !self
            .prune_relative_roots
            .iter()
            .any(|prune| path_under(relative_path, prune))
    }

    pub(crate) fn with_selection(mut self, selection: GraphScopeSelection) -> Self {
        self.selection = selection;
        self
    }

    pub(crate) fn prune_relative_roots(&self) -> &[String] {
        &self.prune_relative_roots
    }
}

const WORKSPACE_ALIAS: &str = "workspace";
const WORKSPACE_SCOPE_KEY: &str = "docs-corpus";

/// Build every scope the workspace can address: the root scope first, then one
/// scope per segmented effective catalog in repository-relative order.
pub fn build_scopes(
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<GraphScope>, CodeGraphError> {
    let workspace_root = canonical_workspace_root(workspace_root)?;
    let mut segmented: Vec<(String, String, PathBuf, bool)> = Vec::new();
    let mut root_alias: Option<String> = None;
    let mut root_relative: Option<String> = None;

    for catalog in catalogs {
        let posture = catalog.catalog_graph_posture();
        if posture.independent && !posture.segmented {
            return Err(CodeGraphError::validation(format!(
                "catalog `{}` declares `[catalog.graph] independent = true` without `segmented = true`; a folded catalog always uses its parent graph corpus",
                catalog.alias
            )));
        }
        // Effective membership canonicalizes catalog roots already; doing it
        // again here keeps a hand-built catalog list honest about symlinked
        // workspace roots (for example macOS `/var` -> `/private/var`).
        let catalog_root = catalog
            .catalog_root
            .canonicalize()
            .unwrap_or_else(|_| catalog.catalog_root.clone());
        let relative = match catalog_root.strip_prefix(&workspace_root) {
            Ok(relative) => relative,
            Err(_) => {
                if posture.segmented {
                    return Err(CodeGraphError::validation(format!(
                        "segmented catalog `{}` resolves outside the workspace root {} at {}; v1 segmented graph scopes must live beneath the workspace root",
                        catalog.alias,
                        workspace_root.display(),
                        catalog_root.display()
                    )));
                }
                // A folded external catalog keeps its routing identity but is
                // not an implicitly scanned graph corpus.
                continue;
            }
        };
        let relative_root = relative_path_string(relative);
        if relative_root.is_empty() {
            if root_alias.is_none() {
                root_alias = Some(catalog.alias.clone());
                root_relative = Some(relative_root);
            }
            continue;
        }
        if posture.segmented {
            segmented.push((
                relative_root.clone(),
                catalog.alias.clone(),
                catalog_root,
                posture.independent,
            ));
        }
    }

    let mut encoded_aliases = BTreeSet::new();
    for (relative_root, alias, _, independent) in &segmented {
        if *independent {
            let encoded = crate::paths::encode_alias(alias)?;
            if !encoded_aliases.insert(encoded) {
                return Err(CodeGraphError::validation(format!(
                    "catalog aliases collide on the encoded graph database directory for `{alias}`"
                )));
            }
            let _ = relative_root;
        }
    }

    segmented.sort_by(|left, right| left.0.cmp(&right.0));
    let segmented_roots = segmented
        .iter()
        .map(|(relative_root, _, _, _)| relative_root.clone())
        .collect::<Vec<_>>();

    let root_alias = root_alias.unwrap_or_else(|| default_root_alias(&workspace_root));
    let root_relative = root_relative.unwrap_or_default();
    let mut scopes = Vec::with_capacity(segmented.len() + 1);
    scopes.push(GraphScope {
        workspace_root: workspace_root.clone(),
        alias: root_alias,
        catalog_root: workspace_root.clone(),
        key: root_relative.clone(),
        relative_root: root_relative,
        selection: GraphScopeSelection::Root,
        segmented: false,
        independent: false,
        workspace: false,
        prune_relative_roots: segmented_roots.clone(),
    });

    for (relative_root, alias, catalog_root, independent) in segmented {
        let prunes = segmented_roots
            .iter()
            .filter(|candidate| {
                candidate.as_str() != relative_root.as_str()
                    && path_under(candidate, &relative_root)
            })
            .cloned()
            .collect::<Vec<_>>();
        scopes.push(GraphScope {
            workspace_root: workspace_root.clone(),
            alias,
            catalog_root,
            key: relative_root.clone(),
            relative_root,
            selection: GraphScopeSelection::Root,
            segmented: true,
            independent,
            workspace: false,
            prune_relative_roots: prunes,
        });
    }

    Ok(scopes)
}

/// Resolve a scope request against explicit catalog membership.
pub fn select_scopes(
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
    request: &GraphScopeRequest,
    cwd: &Path,
) -> Result<GraphScopePlan, CodeGraphError> {
    let scopes = build_scopes(workspace_root, catalogs)?;
    let root = scopes
        .iter()
        .find(|scope| scope.is_root())
        .cloned()
        .ok_or_else(|| CodeGraphError::validation("workspace has no root graph scope"))?;
    let segmented = scopes
        .iter()
        .filter(|scope| scope.segmented)
        .cloned()
        .collect::<Vec<_>>();

    match request {
        GraphScopeRequest::AllCatalogs => {
            let mut fan_out = vec![root];
            fan_out.extend(segmented);
            Ok(GraphScopePlan::FanOut(fan_out))
        }
        GraphScopeRequest::Catalog(alias) => {
            let alias = alias.trim();
            if alias.is_empty() {
                return Err(CodeGraphError::validation(
                    "`--catalog` requires a non-empty catalog alias",
                ));
            }
            if let Some(scope) = segmented.iter().find(|scope| scope.alias == alias).cloned() {
                return Ok(GraphScopePlan::Single(
                    scope.with_selection(GraphScopeSelection::Explicit),
                ));
            }
            let known_folded = catalogs.iter().any(|catalog| {
                catalog.alias == alias && catalog.catalog_graph_posture().is_folded()
            });
            let available = available_aliases(&segmented);
            if known_folded {
                return Err(CodeGraphError::validation(format!(
                    "catalog `{alias}` is folded into its parent graph corpus and has no graph scope; available segmented catalogs: {available}"
                )));
            }
            Err(CodeGraphError::validation(format!(
                "unknown catalog `{alias}`; available segmented catalogs: {available}"
            )))
        }
        GraphScopeRequest::Cwd => {
            let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
            let deepest = segmented
                .iter()
                .filter(|scope| cwd.starts_with(scope.catalog_root()))
                .max_by_key(|scope| scope.relative_root().len())
                .cloned();
            match deepest {
                Some(scope) => Ok(GraphScopePlan::Single(
                    scope.with_selection(GraphScopeSelection::Cwd),
                )),
                None => Ok(GraphScopePlan::Single(root)),
            }
        }
    }
}

fn available_aliases(scopes: &[GraphScope]) -> String {
    if scopes.is_empty() {
        "none".to_owned()
    } else {
        scopes
            .iter()
            .map(|scope| scope.alias.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn canonical_workspace_root(workspace_root: &Path) -> Result<PathBuf, CodeGraphError> {
    workspace_root.canonicalize().map_err(|error| {
        CodeGraphError::validation(format!(
            "graph workspace root {} is not a usable path: {error}",
            workspace_root.display()
        ))
    })
}

fn default_root_alias(workspace_root: &Path) -> String {
    workspace_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "root".to_owned())
}

fn relative_path_string(relative: &Path) -> String {
    relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str().map(str::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn path_under(candidate: &str, root: &str) -> bool {
    if root.is_empty() {
        return true;
    }
    candidate == root
        || (candidate.len() > root.len()
            && candidate.starts_with(root)
            && candidate.as_bytes()[root.len()] == b'/')
}

#[cfg(test)]
mod tests {
    use super::{path_under, GraphScope, GraphScopePlan, GraphScopeRequest};
    use crate::paths::GraphPaths;

    #[test]
    fn path_under_matches_only_complete_segments() {
        assert!(path_under("apps/api", "apps/api"));
        assert!(path_under("apps/api/src/lib.rs", "apps/api"));
        assert!(!path_under("apps/api-v2/src/lib.rs", "apps/api"));
        assert!(path_under("anything", ""));
    }

    #[test]
    fn workspace_scope_never_prunes_and_keeps_the_shared_database() {
        let root = temp_root("scope-workspace");
        let scope = GraphScope::workspace(&root).expect("workspace scope");
        assert!(scope.is_workspace());
        assert!(scope.contains_relative("apps/bovine/src/lib.rs"));
        assert!(!scope.independent());
        assert_eq!(
            GraphPaths::for_scope(&scope).db_path,
            GraphPaths::for_repo(&root.canonicalize().unwrap()).db_path
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cwd_outside_every_catalog_selects_the_root_scope() {
        let fixture = temp_root("scope-cwd");
        let workspace = fixture.join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace");
        let plan = super::select_scopes(
            &workspace,
            &[],
            &GraphScopeRequest::Cwd,
            std::path::Path::new("/"),
        )
        .expect("select");
        match plan {
            GraphScopePlan::Single(scope) => assert!(scope.is_root()),
            other => panic!("unexpected plan: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(fixture);
    }

    fn temp_root(prefix: &str) -> std::path::PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{suffix}"));
        std::fs::create_dir_all(&path).expect("temp root");
        path
    }
}
