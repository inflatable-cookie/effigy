//! Cross-repository source routing for `effigy docs context --sources`.
//!
//! Two-sided membership, one level deep, sequential. A portfolio file names
//! directories; a repository joins only when its own manifest declares
//! `[docs_policy.sources] share = true`. Each repository is answered through
//! the unchanged single-repository entry point with its own store, lock, and
//! budget, and its results are reported in their own block. Nothing here
//! builds an index, caches across repositories, or compares one repository's
//! authority with another's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    load_committed_docs_policy_sources, load_portfolio, ManifestDocsPolicySourcesConfig, Portfolio,
    TASK_MANIFEST_FILE,
};

use crate::error::CodeGraphError;
use crate::storage::GraphStore;

use super::payload::{
    DocsContextBudgetsPayload, DocsContextPayload, DocsContextRequest,
    DocsContextRequestedBudgetsPayload,
};
use super::sources_payload::{
    DocsContextRepositoryPayload, DocsContextSourceResultPayload, DocsContextSourcesPayload,
    CONTENT_IDENTITY_COMMITTED, CONTENT_IDENTITY_WORKING_TREE, DOCS_CONTEXT_SOURCES_SCHEMA,
    DOCS_CONTEXT_SOURCES_SCHEMA_VERSION, STATUS_DISALLOWED, STATUS_EMPTY, STATUS_INVALID,
    STATUS_MISSING, STATUS_NOT_SHARED, STATUS_OK, STATUS_STALE, STATUS_TIMEOUT,
};

/// Directory names never considered as repositories, whatever they contain.
///
/// These hold checkouts of *other* trees (or build output). Descending into
/// them turns a named directory into a crawl and reports the same repository
/// under several handles.
const SKIPPED_DIRECTORY_NAMES: &[&str] = &[".paseo", "worktrees", "node_modules", "target"];

/// What one repository's query did, as reported by the caller that owns the
/// wall-clock budget.
///
/// The timeout arm lives here rather than inside the retrieval because the
/// bound belongs to the runner's single shared timeout model; this module
/// must not grow a second one.
pub enum SourceQueryOutcome {
    Answered(Box<DocsContextPayload>),
    TimedOut,
    Failed(String),
}

/// One enumerated child of a named directory, before it is queried.
struct EnumeratedRepository {
    handle: String,
    path: PathBuf,
    directory: String,
    membership: Membership,
}

enum Membership {
    Shared(ManifestDocsPolicySourcesConfig),
    NotShared(String),
    Invalid(String),
    Missing(String),
}

/// Route `query` across every opted-in repository named by the portfolio.
///
/// `query_repository` runs one repository's retrieval under the caller's
/// wall-clock budget. It is called sequentially, once per shared repository,
/// in report order.
pub fn docs_context_sources(
    portfolio_path: &Path,
    query: &str,
    request: DocsContextRequest,
    only: &[String],
    mut query_repository: impl FnMut(&Path) -> SourceQueryOutcome,
) -> Result<DocsContextSourcesPayload, CodeGraphError> {
    let applied = super::validate_docs_context_request(query, request)?;
    let query = query.trim();

    let portfolio = load_portfolio(portfolio_path).map_err(|error| {
        CodeGraphError::validation(format!("invalid `--sources` portfolio: {error}"))
    })?;

    let enumerated = enumerate(&portfolio);
    let requested: Vec<String> = only.iter().map(|handle| handle.trim().to_owned()).collect();
    let selected: Vec<EnumeratedRepository> = if requested.is_empty() {
        enumerated
    } else {
        enumerated
            .into_iter()
            .filter(|repository| requested.contains(&repository.handle))
            .collect()
    };

    let mut repositories = Vec::new();
    for repository in selected {
        repositories.push(match repository.membership {
            Membership::Shared(ref sources) => {
                let sources = sources.clone();
                let outcome = query_repository(&repository.path);
                answered_block(&repository, &sources, outcome)
            }
            Membership::NotShared(ref next_step) => {
                degraded_block(&repository, STATUS_NOT_SHARED, next_step.clone())
            }
            Membership::Invalid(ref next_step) => {
                degraded_block(&repository, STATUS_INVALID, next_step.clone())
            }
            Membership::Missing(ref next_step) => {
                degraded_block(&repository, STATUS_MISSING, next_step.clone())
            }
        });
    }

    for handle in &requested {
        if repositories
            .iter()
            .any(|repository| &repository.handle == handle)
        {
            continue;
        }
        repositories.push(DocsContextRepositoryPayload {
            handle: handle.clone(),
            path: None,
            directory: None,
            status: STATUS_DISALLOWED.to_owned(),
            next_step: Some(format!(
                "`--only {handle}` matched no directory named by `{}`; run without `--only` to list the handles this portfolio can reach",
                portfolio.source.display()
            )),
            current_head: None,
            indexed_head: None,
            freshness: None,
            profile_state: None,
            front_doors: Vec::new(),
            skill_roots: Vec::new(),
            results: Vec::new(),
        });
    }

    let next = next_steps(&repositories);
    Ok(DocsContextSourcesPayload {
        schema: DOCS_CONTEXT_SOURCES_SCHEMA.to_owned(),
        schema_version: DOCS_CONTEXT_SOURCES_SCHEMA_VERSION,
        query: query.to_owned(),
        portfolio_path: portfolio.source.display().to_string(),
        directories: portfolio.declared.clone(),
        only: requested,
        budgets: DocsContextBudgetsPayload {
            requested: DocsContextRequestedBudgetsPayload {
                max_sections: request.max_sections,
                max_bytes: request.max_bytes,
                max_hops: request.max_hops,
            },
            applied,
            defaults: super::DocsContextBudgetSetPayload::defaults(),
            maximum: super::DocsContextBudgetSetPayload::maximum(),
        },
        repositories,
        next,
    })
}

/// Resolve every named directory one level deep, in directory order then
/// child-name order. A directory that does not exist is one reported entry,
/// never a failure of the whole call.
fn enumerate(portfolio: &Portfolio) -> Vec<EnumeratedRepository> {
    let mut enumerated = Vec::new();
    for (index, directory) in portfolio.directories.iter().enumerate() {
        let declared = portfolio
            .declared
            .get(index)
            .cloned()
            .unwrap_or_else(|| directory.display().to_string());
        let Ok(entries) = std::fs::read_dir(directory) else {
            enumerated.push(EnumeratedRepository {
                handle: handle_for(directory, &declared),
                path: directory.clone(),
                directory: declared.clone(),
                membership: Membership::Missing(format!(
                    "portfolio directory `{}` is absent; create it, or drop it from the portfolio file",
                    directory.display()
                )),
            });
            continue;
        };

        let mut children: Vec<(String, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || SKIPPED_DIRECTORY_NAMES.contains(&name.as_str()) {
                continue;
            }
            let path = entry.path();
            if !path.is_dir() || escapes_directory(directory, &path) {
                continue;
            }
            children.push((name, path));
        }
        children.sort_by(|left, right| left.0.cmp(&right.0));

        for (name, path) in children {
            let membership = classify(&path);
            enumerated.push(EnumeratedRepository {
                handle: name,
                path,
                directory: declared.clone(),
                membership,
            });
        }
    }
    enumerated
}

/// A symlinked child is followed only while it stays inside the directory the
/// portfolio actually named; anything else is silently out of scope, because
/// a link is not a declaration of membership.
fn escapes_directory(directory: &Path, child: &Path) -> bool {
    let Ok(child_canonical) = child.canonicalize() else {
        return true;
    };
    let Ok(directory_canonical) = directory.canonicalize() else {
        return true;
    };
    !child_canonical.starts_with(&directory_canonical)
}

fn classify(path: &Path) -> Membership {
    if !path.join(".git").exists() {
        return Membership::Invalid(format!(
            "`{}` is not a git checkout; cross-repository routing reports commit identity, so it cannot search it",
            path.display()
        ));
    }
    let manifest_path = path.join(TASK_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Membership::NotShared(format!(
            "`{}` has no `{TASK_MANIFEST_FILE}`; add one with `[docs_policy.sources] share = true` to let it be found",
            path.display()
        ));
    }
    // Committed bytes of the child's own `effigy.toml`, and nothing else. This
    // runs on repositories that never opted in, so it must not read their
    // uncommitted overlay, follow their includes, or resolve their bundle —
    // any of which would let a neighbour be written to, or be searched on the
    // strength of text it never committed. Composition is reserved for a
    // repository that has already said yes, when it is queried.
    match load_committed_docs_policy_sources(path) {
        Err(error) => Membership::Invalid(format!(
            "`{}` could not be read: {error}",
            manifest_path.display()
        )),
        Ok(None) => Membership::NotShared(format!(
            "`{}` does not declare `[docs_policy.sources]`; add `share = true` there (in this file, not an include) to let it be found",
            manifest_path.display()
        )),
        Ok(Some(sources)) if !sources.share => Membership::NotShared(format!(
            "`{}` declares `[docs_policy.sources] share = false`; set it to `true` to let it be found",
            manifest_path.display()
        )),
        Ok(Some(sources)) => Membership::Shared(sources),
    }
}

fn handle_for(directory: &Path, declared: &str) -> String {
    directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| declared.to_owned())
}

fn degraded_block(
    repository: &EnumeratedRepository,
    status: &str,
    next_step: String,
) -> DocsContextRepositoryPayload {
    DocsContextRepositoryPayload {
        handle: repository.handle.clone(),
        path: Some(repository.path.display().to_string()),
        directory: Some(repository.directory.clone()),
        status: status.to_owned(),
        next_step: Some(next_step),
        current_head: None,
        indexed_head: None,
        freshness: None,
        profile_state: None,
        front_doors: Vec::new(),
        skill_roots: Vec::new(),
        results: Vec::new(),
    }
}

fn answered_block(
    repository: &EnumeratedRepository,
    sources: &ManifestDocsPolicySourcesConfig,
    outcome: SourceQueryOutcome,
) -> DocsContextRepositoryPayload {
    let path = &repository.path;
    let current_head = crate::git::current_head(path);
    let indexed_head = indexed_head(path);
    let mut block = DocsContextRepositoryPayload {
        handle: repository.handle.clone(),
        path: Some(path.display().to_string()),
        directory: Some(repository.directory.clone()),
        status: STATUS_OK.to_owned(),
        next_step: None,
        current_head,
        indexed_head,
        freshness: None,
        profile_state: None,
        front_doors: sources.front_doors.clone(),
        skill_roots: sources.skill_roots.clone(),
        results: Vec::new(),
    };

    match outcome {
        SourceQueryOutcome::TimedOut => {
            block.status = STATUS_TIMEOUT.to_owned();
            block.next_step = Some(format!(
                "`{}` did not answer inside the graph time budget; run `effigy graph index --repo {}` once to pay the build separately, or raise `EFFIGY_GRAPH_TIMEOUT_MS`",
                repository.handle,
                path.display()
            ));
        }
        SourceQueryOutcome::Failed(detail) => {
            block.status = STATUS_INVALID.to_owned();
            block.next_step = Some(format!(
                "`{}` could not be searched: {detail}",
                repository.handle
            ));
        }
        SourceQueryOutcome::Answered(payload) => {
            let dirty = crate::git::dirty_paths(path);
            block.results = payload
                .results
                .iter()
                .map(|result| DocsContextSourceResultPayload {
                    content_identity: content_identity(
                        &block.current_head,
                        dirty.as_ref(),
                        &result.path,
                    )
                    .to_owned(),
                    result: result.clone(),
                })
                .collect();
            block.profile_state = Some(payload.profile.state.clone());
            let freshness = payload.freshness.clone();
            block.status =
                if !freshness.usable || freshness.stale || freshness.failed_path_count > 0 {
                    STATUS_STALE.to_owned()
                } else if block.results.is_empty() {
                    STATUS_EMPTY.to_owned()
                } else {
                    STATUS_OK.to_owned()
                };
            if block.status == STATUS_STALE {
                block.next_step = Some(format!(
                    "`{}` answered from a graph index that is not fully trusted ({}); run `effigy graph index --repo {}`",
                    repository.handle,
                    freshness.summary,
                    path.display()
                ));
            }
            block.freshness = Some(freshness);
        }
    }
    block
}

/// HEAD the local index was built from, when it was built over a clean tree.
///
/// Reported as-is: an index built over uncommitted edits carries no stamp, and
/// inventing one would let a caller believe an excerpt is committed.
fn indexed_head(repo_root: &Path) -> Option<String> {
    let store = GraphStore::open(repo_root).ok()?;
    store
        .metadata_value(crate::git::GIT_INDEXED_HEAD_KEY)
        .ok()?
}

/// A result's bytes are `committed` only when git could answer and the file is
/// unchanged in the working tree. Every uncertainty reports `working-tree`.
fn content_identity(
    current_head: &Option<String>,
    dirty: Option<&BTreeSet<String>>,
    path: &str,
) -> &'static str {
    match (current_head, dirty) {
        (Some(_), Some(dirty)) if !dirty.contains(path) => CONTENT_IDENTITY_COMMITTED,
        _ => CONTENT_IDENTITY_WORKING_TREE,
    }
}

fn next_steps(repositories: &[DocsContextRepositoryPayload]) -> Vec<String> {
    let mut next = Vec::new();
    let answered = repositories
        .iter()
        .filter(|repository| repository.status == STATUS_OK)
        .count();
    if repositories.is_empty() {
        next.push(
            "the portfolio named no repositories; add a directory holding checkouts to it"
                .to_owned(),
        );
    } else if answered == 0 {
        next.push(
            "no repository returned a section; retry with terms that appear in the shared docs"
                .to_owned(),
        );
    }
    for repository in repositories {
        if let Some(step) = &repository.next_step {
            next.push(step.clone());
        }
    }
    next
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
