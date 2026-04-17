use std::path::PathBuf;

use effigy_manifest::ManifestError;

/// Narrow error boundary for the task-routing surface.
///
/// Produced by `catalog/**` discovery and selection. Lifts to
/// `RunnerError` at the runner edge via `From<RoutingError> for RunnerError`
/// in `src/runner/error.rs`. Matches the Job-8 pattern used by
/// `effigy-process`, `effigy-ui`, `effigy-managed`, `effigy-env`, and
/// sets the crate-move seam for card `246` (extraction to `effigy-routing`).
///
/// Variants:
///
/// - Seven routing-specific variants, same shapes as the corresponding
///   `RunnerError::Task*` variants. Kept shape-identical so the existing
///   deferral `policy.rs` pattern-match on `RunnerError` continues to
///   work after the `From` impl reproduces them.
/// - One `Manifest` wrapper variant that bridges catalog's own
///   `load_task_manifest` call path (Part B of card `245`). Catalog no
///   longer reaches into `src/runner/manifest.rs` for loading; errors
///   surface through this variant and lift to the existing
///   `RunnerError::TaskManifest*` variants via the runner-edge `From`.
#[derive(Debug)]
pub(in crate::runner) enum RoutingError {
    TaskCatalogsMissing {
        root: PathBuf,
    },
    TaskCatalogReadDir {
        path: PathBuf,
        error: std::io::Error,
    },
    TaskCatalogAliasConflict {
        alias: String,
        first_path: PathBuf,
        second_path: PathBuf,
    },
    TaskCatalogPrefixNotFound {
        prefix: String,
        available: Vec<String>,
    },
    TaskNotFound {
        name: String,
        path: PathBuf,
    },
    TaskNotFoundAny {
        name: String,
        catalogs: Vec<String>,
    },
    TaskAmbiguous {
        name: String,
        candidates: Vec<String>,
    },
    Manifest(ManifestError),
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingError::TaskCatalogsMissing { root } => write!(
                f,
                "no task catalogs found under {} (expected one or more effigy.toml files)",
                root.display()
            ),
            RoutingError::TaskCatalogReadDir { path, error } => {
                write!(f, "failed to read directory {}: {error}", path.display())
            }
            RoutingError::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path,
            } => write!(
                f,
                "duplicate task catalog alias `{alias}` found in {} and {}",
                first_path.display(),
                second_path.display()
            ),
            RoutingError::TaskCatalogPrefixNotFound { prefix, available } => write!(
                f,
                "task catalog prefix `{prefix}` not found (available: {})",
                available.join(", ")
            ),
            RoutingError::TaskNotFound { name, path } => {
                write!(f, "task `{name}` is not defined in {}", path.display())
            }
            RoutingError::TaskNotFoundAny { name, catalogs } => write!(
                f,
                "task `{name}` is not defined in discovered catalogs: {}",
                catalogs.join(", ")
            ),
            RoutingError::TaskAmbiguous { name, candidates } => write!(
                f,
                "task `{name}` is ambiguous; matched multiple catalogs: {}",
                candidates.join(", ")
            ),
            RoutingError::Manifest(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RoutingError {}

impl From<ManifestError> for RoutingError {
    fn from(value: ManifestError) -> Self {
        RoutingError::Manifest(value)
    }
}
