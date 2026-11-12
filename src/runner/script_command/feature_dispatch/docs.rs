use std::path::{Path, PathBuf};

use effigy_cli::{DocsBlockRequirement, DocsSubcommand};
use effigy_rhai::surface::*;
use serde_json::Value;

use super::*;
use crate::runner::error::RunnerError;

pub(super) fn handles(feature: &str) -> bool {
    matches!(
        feature,
        FEATURE_DOCS_CHECK_LINKS
            | FEATURE_DOCS_CHECK_JSON_EXAMPLES
            | FEATURE_DOCS_CHECK_HEADINGS
            | FEATURE_DOCS_CHECK_PATHS
            | FEATURE_DOCS_CHECK_CONTAINS
            | FEATURE_DOCS_CHECK_FORBIDDEN
            | FEATURE_DOCS_CHECK_INDEX
            | FEATURE_DOCS_CHECK_NEXT_ACTION
            | FEATURE_DOCS_CHECK_WORKFLOW_PATHS
            | FEATURE_DOCS_ADD_LOG_INDEX
    )
}

pub(super) fn dispatch(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    match feature {
        FEATURE_DOCS_CHECK_LINKS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Links,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_JSON_EXAMPLES => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::JsonExamples,
                paths: Vec::new(),
                file: path_option(&options, "file")?,
                section: string_option(&options, "section")?,
                min_blocks: usize_option(&options, "min_blocks")?,
                required_text: string_array(&options, "required")?,
                required_blocks: docs_block_requirements(&options)?,
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_HEADINGS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Headings,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: string_array(&options, "required_headings")?,
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_PATHS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Paths,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_CONTAINS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Contains,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: string_array_any(&options, &["required_text", "required"])?,
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_FORBIDDEN => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Forbidden,
                paths: path_array(&options, "paths")?,
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: string_array_any(&options, &["forbidden_text", "forbidden"])?,
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_INDEX => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::Index,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(string_option(&options, "policy_index")?),
                dir: Box::new(path_option(&options, "dir")?),
                index: Box::new(path_option(&options, "index")?),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_CHECK_NEXT_ACTION => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::NextAction,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(None),
                index: Box::new(None),
                policy_name: Box::new(string_option(&options, "policy_name")?),
            },
        ),
        FEATURE_DOCS_CHECK_WORKFLOW_PATHS => run_docs_json(
            repo_root,
            DocsSubcommand::Check {
                kind: effigy_cli::DocsCheckKind::WorkflowPaths,
                paths: Vec::new(),
                file: None,
                section: None,
                min_blocks: None,
                required_text: Vec::new(),
                required_blocks: Vec::new(),
                required_headings: Vec::new(),
                forbidden_text: Vec::new(),
                policy_index: Box::new(None),
                dir: Box::new(path_option(&options, "dir")?),
                index: Box::new(None),
                policy_name: Box::new(None),
            },
        ),
        FEATURE_DOCS_ADD_LOG_INDEX => run_docs_json(
            repo_root,
            DocsSubcommand::AddLogIndex {
                log_path: PathBuf::from(required_string(&options, "log_path")?),
            },
        ),

        other => Err(RunnerError::task_invocation(format!(
            "unknown Rhai feature `{other}`"
        ))),
    }
}

pub(super) fn docs_block_requirements(
    options: &Value,
) -> Result<Vec<DocsBlockRequirement>, RunnerError> {
    let Some(value) = options.get("required_blocks") else {
        return Ok(Vec::new());
    };
    let array = value.as_array().ok_or_else(|| {
        RunnerError::task_invocation("`required_blocks` must be an array of maps")
    })?;
    array
        .iter()
        .map(|entry| {
            Ok(DocsBlockRequirement {
                block_index: required_usize(entry, "block_index")?,
                needle: required_string(entry, "needle")?,
            })
        })
        .collect()
}
