use super::constants::{
    DEFAULT_DATA_DIRS, DEFAULT_DOC_DIRS, DEFAULT_EXCLUDED_DIRS, DEFAULT_LOCK_FILE_NAMES,
    GENERATED_PATH_COMPONENTS,
};
use crate::error::ScanError;

#[path = "support/heuristics/mod.rs"]
mod heuristics;
#[path = "support/traversal/mod.rs"]
mod traversal;

pub use heuristics::{
    classify_duplicate_block_severity, classify_severity, count_code_lines,
    duplicate_block_severity_rank, is_generated_artifact, is_literal_only_block,
    normalized_code_lines, severity_rank, trim_snippet, NormalizedCodeLine,
};
#[cfg(test)]
pub use traversal::normalize_rel_path;
pub use traversal::{
    compile_glob_set, rebase_finding_path, should_skip_inventory_path, should_skip_path,
    walk_scan_files, workspace_scan_roots,
};
