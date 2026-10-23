//! CLI command handler for `effigy docs` subcommands.

use std::path::{Path, PathBuf};

use effigy_docs_policy::{resolve_repo_input as resolve_docs_repo_input, DocsPolicyError};

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::manifest::{load_task_manifest, ManifestDocsPolicyConfig, TASK_MANIFEST_FILE};
use effigy_cli::{DocsArgs, DocsCheckKind, DocsSubcommand};

use super::error::RunnerError;

mod checks;
mod paths;
mod report;

const DEFAULT_JSON_EXAMPLES_FILE: &str = "docs/guides/026-json-payload-examples.md";
const DEFAULT_JSON_EXAMPLES_SECTION: &str = "Completion Candidates";
const DEFAULT_LOGS_DIR: &str = "docs/notes";
const DEFAULT_WORKFLOW_DOCS_DIR: &str = "docs";

pub(super) fn run_docs(args: DocsArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_command_context(args.repo_override.clone())?;
    let repo_root = resolved.resolved.resolved_root;
    let invocation_cwd = resolved.invocation_cwd;
    // Explicit path arguments belong to the invoking task's cwd when that cwd
    // is inside the checkout (a nested catalog's `docs/`, for example). A cwd
    // outside the checkout cannot own project paths, so `--repo <ROOT>` from an
    // unrelated directory keeps resolving arguments from the repository root.
    let task_cwd = if invocation_cwd.starts_with(&repo_root) {
        invocation_cwd
    } else {
        repo_root.clone()
    };

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
            DocsCheckKind::Links => {
                let resolved_paths = checked_paths(&repo_root, &task_cwd, &paths);
                checks::run_check_links(&repo_root, &resolved_paths, args.output_json)
            }
            DocsCheckKind::JsonExamples => {
                let (file, warnings) =
                    paths::resolve_explicit_optional(&repo_root, &task_cwd, file.as_ref());
                emit_transition_warnings(&warnings);
                checks::run_check_json_examples(
                    &repo_root,
                    file.as_ref(),
                    section.as_deref(),
                    min_blocks,
                    &required_text,
                    &required_blocks,
                    args.output_json,
                )
            }
            DocsCheckKind::Headings => {
                let resolved_paths = checked_paths(&repo_root, &task_cwd, &paths);
                checks::run_check_headings(
                    &repo_root,
                    &resolved_paths,
                    &required_headings,
                    args.output_json,
                )
            }
            DocsCheckKind::Paths => {
                let resolved_paths = checked_paths(&repo_root, &task_cwd, &paths);
                checks::run_check_paths(&repo_root, &resolved_paths, args.output_json)
            }
            DocsCheckKind::Contains => {
                let resolved_paths = checked_paths(&repo_root, &task_cwd, &paths);
                checks::run_check_contains(
                    &repo_root,
                    &resolved_paths,
                    &required_text,
                    args.output_json,
                )
            }
            DocsCheckKind::Forbidden => {
                let resolved_paths = checked_paths(&repo_root, &task_cwd, &paths);
                checks::run_check_forbidden(
                    &repo_root,
                    &resolved_paths,
                    &forbidden_text,
                    args.output_json,
                )
            }
            DocsCheckKind::Index => {
                let (dir, mut warnings) =
                    paths::resolve_explicit_optional(&repo_root, &task_cwd, dir.as_ref().as_ref());
                let (index, index_warnings) = paths::resolve_explicit_optional(
                    &repo_root,
                    &task_cwd,
                    index.as_ref().as_ref(),
                );
                warnings.extend(index_warnings);
                emit_transition_warnings(&warnings);
                checks::run_check_index(
                    &repo_root,
                    policy_index.as_ref().as_deref(),
                    dir.as_ref(),
                    index.as_ref(),
                    args.output_json,
                )
            }
            DocsCheckKind::NextAction => checks::run_check_next_action(
                &repo_root,
                policy_name.as_ref().as_deref(),
                args.output_json,
            ),
            DocsCheckKind::WorkflowPaths => {
                let (dir, warnings) =
                    paths::resolve_explicit_optional(&repo_root, &task_cwd, dir.as_ref().as_ref());
                emit_transition_warnings(&warnings);
                checks::run_check_workflow_paths(&repo_root, dir.as_ref(), args.output_json)
            }
        },
        DocsSubcommand::AddLogIndex { log_path } => {
            checks::run_add_log_index(&repo_root, &log_path, args.output_json)
        }
    }
}

/// Resolve the check's explicit path arguments against the invoking task's cwd.
fn checked_paths(repo_root: &Path, invocation_cwd: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    let (resolved, warnings) = paths::resolve_explicit_paths(repo_root, invocation_cwd, paths);
    emit_transition_warnings(&warnings);
    resolved
}

/// The one-release transition notice accepts an existing root-relative spelling.
fn emit_transition_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("docs check: {warning}");
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
