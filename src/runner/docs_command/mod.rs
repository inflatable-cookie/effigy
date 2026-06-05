//! CLI command handler for `effigy docs` subcommands.

use std::path::{Path, PathBuf};

use effigy_docs_policy::{resolve_repo_input as resolve_docs_repo_input, DocsPolicyError};

use crate::runner::command_context::resolve_active_repo_root;
use crate::runner::manifest::{load_task_manifest, ManifestDocsPolicyConfig, TASK_MANIFEST_FILE};
use effigy_cli::{DocsArgs, DocsCheckKind, DocsSubcommand};

use super::error::RunnerError;

mod checks;
mod report;

const DEFAULT_JSON_EXAMPLES_FILE: &str = "docs/guides/026-json-payload-examples.md";
const DEFAULT_JSON_EXAMPLES_SECTION: &str = "Completion Candidates";
const DEFAULT_LOGS_DIR: &str = "docs/logs";
const DEFAULT_WORKFLOW_DOCS_DIR: &str = "docs";

pub(super) fn run_docs(args: DocsArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;

    match args.subcommand {
        DocsSubcommand::Check {
            kind,
            paths,
            file,
            section,
            min_blocks,
            required_text,
            required_blocks,
            required_headings,
            forbidden_text,
            policy_index,
            dir,
            index,
            policy_name,
        } => match kind {
            DocsCheckKind::Links => checks::run_check_links(&repo_root, &paths, args.output_json),
            DocsCheckKind::JsonExamples => checks::run_check_json_examples(
                &repo_root,
                file.as_ref(),
                section.as_deref(),
                min_blocks,
                &required_text,
                &required_blocks,
                args.output_json,
            ),
            DocsCheckKind::Headings => {
                checks::run_check_headings(&repo_root, &paths, &required_headings, args.output_json)
            }
            DocsCheckKind::Paths => checks::run_check_paths(&repo_root, &paths, args.output_json),
            DocsCheckKind::Contains => {
                checks::run_check_contains(&repo_root, &paths, &required_text, args.output_json)
            }
            DocsCheckKind::Forbidden => {
                checks::run_check_forbidden(&repo_root, &paths, &forbidden_text, args.output_json)
            }
            DocsCheckKind::Index => checks::run_check_index(
                &repo_root,
                policy_index.as_ref().as_deref(),
                dir.as_ref().as_ref(),
                index.as_ref().as_ref(),
                args.output_json,
            ),
            DocsCheckKind::NextAction => checks::run_check_next_action(
                &repo_root,
                policy_name.as_ref().as_deref(),
                args.output_json,
            ),
            DocsCheckKind::WorkflowPaths => checks::run_check_workflow_paths(
                &repo_root,
                dir.as_ref().as_ref(),
                args.output_json,
            ),
        },
        DocsSubcommand::AddLogIndex { log_path } => {
            checks::run_add_log_index(&repo_root, &log_path, args.output_json)
        }
    }
}

fn load_docs_policy_config(repo_root: &Path) -> Result<ManifestDocsPolicyConfig, RunnerError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Ok(ManifestDocsPolicyConfig::default());
    }
    Ok(load_task_manifest(&manifest_path)?
        .docs_policy
        .unwrap_or_default())
}

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    resolve_docs_repo_input(repo_root, path)
}

fn map_docs_policy_error(error: DocsPolicyError) -> RunnerError {
    report::map_docs_policy_error(error)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
