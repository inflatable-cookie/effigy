//! Task catalog routing for Effigy.
//!
//! Extracted from `src/runner/catalog/**` under card `246`. Owns the
//! explicit catalog membership, prefix/cwd/depth selection strategies, and
//! the narrow `RoutingError` boundary that lifts to `RunnerError` at
//! the runner's edge via `impl From<RoutingError> for RunnerError` in
//! `src/runner/error.rs`.
//!
//! Callers import from `effigy_routing::` directly — there is no
//! transitional shim in `src/lib.rs` (242 / second-sweep lesson).

mod error;
mod manifest_load;
mod membership;
mod selection;

use std::path::Path;

use effigy_core::task_selection::TaskSelector;
use effigy_manifest::{LoadedCatalog, TaskResolveError, TaskSelection};

pub use error::RoutingError;
pub use manifest_load::{load_task_manifest, TASK_MANIFEST_FILE};
pub use membership::{
    default_alias, effective_manifest_paths, load_effective_catalogs,
    load_effective_catalogs_allow_missing, load_isolated_catalog, owning_workspace_root,
};
pub use selection::{resolve_catalog_by_prefix, select_catalog_and_task};

/// Typed adapter for `select_catalog_and_task`, shaped to fit
/// `effigy_manifest::TaskResolverFn`: only "no catalog declares this task"
/// becomes [`TaskResolveError::NotFound`].
pub fn resolve_task_selection<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, TaskResolveError> {
    select_catalog_and_task(selector, catalogs, cwd).map_err(classify_routing_error)
}

/// Project a routing failure onto the classes the resolver branches on.
pub fn classify_routing_error(error: RoutingError) -> TaskResolveError {
    let detail = error.to_string();
    match error {
        RoutingError::TaskNotFound { .. }
        | RoutingError::TaskNotFoundAny { .. }
        | RoutingError::TaskCatalogPrefixNotFound { .. } => TaskResolveError::not_found(detail),
        RoutingError::TaskAmbiguous { .. } => TaskResolveError::Ambiguous(detail),
        RoutingError::Manifest(_)
        | RoutingError::TaskCatalogsMissing { .. }
        | RoutingError::TaskCatalogAliasConflict { .. }
        | RoutingError::TaskCatalogMemberInvalid { .. } => TaskResolveError::Unavailable(detail),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{classify_routing_error, RoutingError};

    /// Only the three "nothing declares this task" shapes become the
    /// fallback-eligible class; an unreadable graph or an ambiguous selection
    /// stays distinct, and the sentence a person reads is unchanged.
    #[test]
    fn routing_failures_classify_into_the_resolver_classes() {
        let not_found = [
            RoutingError::TaskNotFound {
                name: "build".to_owned(),
                path: PathBuf::from("/repo"),
            },
            RoutingError::TaskNotFoundAny {
                name: "build".to_owned(),
                catalogs: vec!["repo".to_owned()],
            },
            RoutingError::TaskCatalogPrefixNotFound {
                prefix: "acme".to_owned(),
                available: vec!["repo".to_owned()],
            },
        ];
        for error in not_found {
            let sentence = error.to_string();
            let classified = classify_routing_error(error);
            assert!(classified.is_not_found(), "{classified:?}");
            assert_eq!(classified.to_string(), sentence);
        }

        let ambiguous = classify_routing_error(RoutingError::TaskAmbiguous {
            name: "build".to_owned(),
            candidates: vec!["repo".to_owned(), "services".to_owned()],
        });
        assert!(!ambiguous.is_not_found(), "{ambiguous:?}");

        for error in [
            RoutingError::TaskCatalogsMissing {
                root: PathBuf::from("/repo"),
            },
            RoutingError::TaskCatalogAliasConflict {
                alias: "repo".to_owned(),
                first_path: PathBuf::from("/repo/effigy.toml"),
                second_path: PathBuf::from("/repo/nested/effigy.toml"),
            },
        ] {
            let classified = classify_routing_error(error);
            assert!(
                !classified.is_not_found(),
                "a catalog graph failure is not a missing task: {classified:?}"
            );
        }
    }
}
