//! CLI command handler for `effigy docs` subcommands.

use std::path::{Path, PathBuf};

use effigy_docs_policy::{
    add_log_index_report, check_contains, check_forbidden, check_headings, check_paths,
    check_workflow_paths,
    checks::{
        check_next_action, default_json_example_block_requirements,
        default_json_example_requirements, validate_json_examples,
    },
    collect_index_markdown_links, collect_link_check_files, collect_markdown_children,
    contains_check_report, forbidden_check_report, heading_check_report, index_check_report,
    insert_log_index_entry, json_examples_check_report, link_check_report,
    next_action_check_report, normalize_log_index_relative_path, path_check_report,
    resolve_docs_index_spec, resolve_docs_next_action_spec, resolve_repo_input,
    scan_markdown_links, workflow_path_check_report, AddLogIndexReportInputs, DocsCheckReport,
    DocsPolicyError, JsonExamplesReportInputs,
};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::manifest::{load_task_manifest, ManifestDocsPolicyConfig};
use crate::{DocsArgs, DocsBlockRequirement, DocsSubcommand};

use super::error::RunnerError;

const DEFAULT_JSON_EXAMPLES_FILE: &str = "docs/guides/026-json-payload-examples.md";
const DEFAULT_JSON_EXAMPLES_SECTION: &str = "Completion Candidates";
const DEFAULT_LOGS_DIR: &str = "docs/logs";
const DEFAULT_WORKFLOW_DOCS_DIR: &str = "docs";

pub(super) fn run_docs(args: DocsArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;

    match args.subcommand {
        DocsSubcommand::CheckLinks { paths } => {
            run_check_links(&repo_root, &paths, args.output_json)
        }
        DocsSubcommand::CheckJsonExamples {
            file,
            section,
            min_blocks,
            required,
            required_blocks,
        } => run_check_json_examples(
            &repo_root,
            file.as_ref(),
            section.as_deref(),
            min_blocks,
            &required,
            &required_blocks,
            args.output_json,
        ),
        DocsSubcommand::CheckHeadings {
            paths,
            required_headings,
        } => run_check_headings(&repo_root, &paths, &required_headings, args.output_json),
        DocsSubcommand::CheckPaths { paths } => {
            run_check_paths(&repo_root, &paths, args.output_json)
        }
        DocsSubcommand::CheckContains {
            paths,
            required_text,
        } => run_check_contains(&repo_root, &paths, &required_text, args.output_json),
        DocsSubcommand::CheckForbidden {
            paths,
            forbidden_text,
        } => run_check_forbidden(&repo_root, &paths, &forbidden_text, args.output_json),
        DocsSubcommand::CheckIndex {
            policy_index,
            dir,
            index,
        } => run_check_index(
            &repo_root,
            policy_index.as_deref(),
            dir.as_ref(),
            index.as_ref(),
            args.output_json,
        ),
        DocsSubcommand::CheckNextAction { policy_name } => {
            run_check_next_action(&repo_root, policy_name.as_deref(), args.output_json)
        }
        DocsSubcommand::CheckWorkflowPaths { dir } => {
            run_check_workflow_paths(&repo_root, dir.as_ref(), args.output_json)
        }
        DocsSubcommand::AddLogIndex { log_path } => {
            run_add_log_index(&repo_root, &log_path, args.output_json)
        }
    }
}

/// Surface a docs-policy check report in the form the user requested.
///
/// `ok` reports produce a success string or json payload; failing reports
/// produce a `RunnerError::task_invocation` whose body carries the report
/// failure text (or the json payload, in json mode).
fn dispatch_docs_report(report: DocsCheckReport, output_json: bool) -> Result<String, RunnerError> {
    if output_json {
        let rendered = report.json.to_string();
        if report.ok {
            Ok(rendered)
        } else {
            Err(RunnerError::task_invocation(rendered))
        }
    } else if report.ok {
        Ok(report.success_text)
    } else {
        Err(RunnerError::task_invocation(report.failure_text))
    }
}

fn run_check_links(
    repo_root: &Path,
    paths: &[PathBuf],
    output_json: bool,
) -> Result<String, RunnerError> {
    let files = collect_link_check_files(repo_root, paths);
    let failures = files
        .iter()
        .flat_map(|file| scan_markdown_links(file).unwrap_or_else(|err| vec![err]))
        .collect::<Vec<_>>();
    dispatch_docs_report(link_check_report(&files, &failures), output_json)
}

fn run_check_json_examples(
    repo_root: &Path,
    file_override: Option<&PathBuf>,
    section_override: Option<&str>,
    min_blocks_override: Option<usize>,
    required_override: &[String],
    required_blocks_override: &[DocsBlockRequirement],
    output_json: bool,
) -> Result<String, RunnerError> {
    let file = resolve_repo_input(
        repo_root,
        file_override
            .cloned()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_JSON_EXAMPLES_FILE)),
    );
    let section_title = section_override.unwrap_or(DEFAULT_JSON_EXAMPLES_SECTION);
    let min_blocks = min_blocks_override.unwrap_or(2);
    let required = if required_override.is_empty() {
        default_json_example_requirements()
    } else {
        required_override.to_vec()
    };
    let required_blocks_tuples: Vec<(usize, String)> = if required_blocks_override.is_empty() {
        default_json_example_block_requirements()
    } else {
        required_blocks_override
            .iter()
            .map(|r| (r.block_index, r.needle.clone()))
            .collect()
    };

    let content = std::fs::read_to_string(&file)
        .map_err(|err| RunnerError::task_invocation_failed_read(&file, err))?;

    let result = validate_json_examples(
        &content,
        &file.display().to_string(),
        section_title,
        min_blocks,
        &required,
        &required_blocks_tuples,
    );

    dispatch_docs_report(
        json_examples_check_report(JsonExamplesReportInputs {
            file: &result.file,
            section: &result.section,
            block_count: result.block_count,
            min_blocks: result.min_blocks,
            required: &result.required,
            required_blocks: &required_blocks_tuples,
            failures: &result.failures,
            ok: result.ok,
        }),
        output_json,
    )
}

fn run_check_headings(
    repo_root: &Path,
    paths: &[PathBuf],
    required_headings: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    if paths.is_empty() {
        return Err(RunnerError::task_invocation(
            "`check-headings` requires at least one file path".to_owned(),
        ));
    }
    if required_headings.is_empty() {
        return Err(RunnerError::task_invocation(
            "`check-headings` requires at least one `--require-heading` value".to_owned(),
        ));
    }

    let (files, findings) =
        check_headings(repo_root, paths, required_headings).map_err(map_docs_policy_error)?;

    dispatch_docs_report(
        heading_check_report(&files, required_headings, &findings),
        output_json,
    )
}

fn run_check_contains(
    repo_root: &Path,
    paths: &[PathBuf],
    required_text: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    if paths.is_empty() {
        return Err(RunnerError::task_invocation(
            "`check-contains` requires at least one file path".to_owned(),
        ));
    }
    if required_text.is_empty() {
        return Err(RunnerError::task_invocation(
            "`check-contains` requires at least one `--require` value".to_owned(),
        ));
    }

    let (files, findings) =
        check_contains(repo_root, paths, required_text).map_err(map_docs_policy_error)?;

    dispatch_docs_report(
        contains_check_report(&files, required_text, &findings),
        output_json,
    )
}

fn run_check_paths(
    repo_root: &Path,
    paths: &[PathBuf],
    output_json: bool,
) -> Result<String, RunnerError> {
    if paths.is_empty() {
        return Err(RunnerError::task_invocation(
            "`check-paths` requires at least one path".to_owned(),
        ));
    }

    let (resolved_paths, findings) = check_paths(repo_root, paths);
    dispatch_docs_report(path_check_report(&resolved_paths, &findings), output_json)
}

fn run_check_forbidden(
    repo_root: &Path,
    paths: &[PathBuf],
    forbidden_text: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    if paths.is_empty() {
        return Err(RunnerError::task_invocation(
            "`check-forbidden` requires at least one file path".to_owned(),
        ));
    }
    if forbidden_text.is_empty() {
        return Err(RunnerError::task_invocation(
            "`check-forbidden` requires at least one `--forbid` value".to_owned(),
        ));
    }

    let (files, findings) =
        check_forbidden(repo_root, paths, forbidden_text).map_err(map_docs_policy_error)?;

    dispatch_docs_report(
        forbidden_check_report(&files, forbidden_text, &findings),
        output_json,
    )
}

fn run_check_index(
    repo_root: &Path,
    policy_index: Option<&str>,
    dir_override: Option<&PathBuf>,
    index_override: Option<&PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_docs_policy_config(repo_root)?;
    let spec = resolve_docs_index_spec(
        repo_root,
        &policy,
        policy_index,
        dir_override,
        index_override,
    )
    .map_err(map_docs_policy_error)?;

    if !spec.dir.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "docs index directory not found: {}",
            spec.dir.display()
        )));
    }
    if !spec.index.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "docs index not found: {}",
            spec.index.display()
        )));
    }

    let all_docs = collect_markdown_children(&spec.dir, &spec.exclude);
    let indexed = collect_index_markdown_links(&spec.index, spec.section.as_deref())
        .map_err(map_docs_policy_error)?;
    let missing = all_docs.difference(&indexed).cloned().collect::<Vec<_>>();
    let extra = indexed.difference(&all_docs).cloned().collect::<Vec<_>>();

    dispatch_docs_report(index_check_report(&spec, &missing, &extra), output_json)
}

fn load_docs_policy_config(repo_root: &Path) -> Result<ManifestDocsPolicyConfig, RunnerError> {
    let manifest_path = repo_root.join("effigy.toml");
    if !manifest_path.is_file() {
        return Ok(ManifestDocsPolicyConfig::default());
    }
    Ok(load_task_manifest(&manifest_path)?
        .docs_policy
        .unwrap_or_default())
}

fn run_check_next_action(
    repo_root: &Path,
    policy_name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_docs_policy_config(repo_root)?;
    let spec = resolve_docs_next_action_spec(repo_root, &policy, policy_name)
        .map_err(map_docs_policy_error)?;

    let findings = check_next_action(&spec).map_err(map_docs_policy_error)?;

    dispatch_docs_report(next_action_check_report(&spec, &findings), output_json)
}

fn run_add_log_index(
    repo_root: &Path,
    log_path: &Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    let index_path = repo_root.join("docs/logs/README.md");
    if !index_path.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "logs index not found: {}",
            index_path.display()
        )));
    }

    let relative_path =
        normalize_log_index_relative_path(log_path).map_err(map_docs_policy_error)?;
    let resolved_log_path = repo_root.join(DEFAULT_LOGS_DIR).join(&relative_path);
    if !resolved_log_path.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "log file not found: {}",
            resolved_log_path.display()
        )));
    }

    let entry = format!("- [`{relative_path}`](./{relative_path})");
    let original = std::fs::read_to_string(&index_path)
        .map_err(|err| RunnerError::task_invocation_failed_read(&index_path, err))?;
    let already_indexed = original.lines().any(|line| line.trim() == entry);

    if !already_indexed {
        let updated = insert_log_index_entry(&original, &entry);
        std::fs::write(&index_path, updated.as_bytes())
            .map_err(|err| RunnerError::task_invocation_failed_write(&index_path, err))?;
    }

    dispatch_docs_report(
        add_log_index_report(AddLogIndexReportInputs {
            relative_path: &relative_path,
            index_path: &index_path,
            already_indexed,
        }),
        output_json,
    )
}

fn run_check_workflow_paths(
    repo_root: &Path,
    dir_override: Option<&PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let dir = resolve_repo_input(
        repo_root,
        dir_override
            .cloned()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_WORKFLOW_DOCS_DIR)),
    );
    if !dir.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "docs directory not found: {}",
            dir.display()
        )));
    }

    let default_logs_dir = repo_root.join(DEFAULT_LOGS_DIR);
    let findings = check_workflow_paths(repo_root, &dir, &default_logs_dir, dir_override.is_none())
        .map_err(map_docs_policy_error)?;

    dispatch_docs_report(workflow_path_check_report(&dir, &findings), output_json)
}

fn map_docs_policy_error(error: DocsPolicyError) -> RunnerError {
    match error {
        DocsPolicyError::Io { path, error } => {
            RunnerError::task_invocation_failed_read(&path, error)
        }
        DocsPolicyError::Message(message) => RunnerError::task_invocation(message),
    }
}

#[cfg(test)]
#[path = "docs_command/tests.rs"]
mod tests;
