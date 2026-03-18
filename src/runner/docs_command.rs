//! CLI command handler for `effigy docs` subcommands.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::json;
use walkdir::WalkDir;

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::manifest::{load_task_manifest, ManifestDocsPolicyConfig};
use crate::{DocsArgs, DocsBlockRequirement, DocsSubcommand};

use super::error::RunnerError;

const DEFAULT_LINK_FILES: &[&str] = &["README.md"];
const DEFAULT_LINK_DOCS_DIR: &str = "docs";
const DEFAULT_JSON_EXAMPLES_FILE: &str = "docs/guides/026-json-payload-examples.md";
const DEFAULT_JSON_EXAMPLES_SECTION: &str = "Completion Candidates";
const DEFAULT_LOGS_DIR: &str = "docs/logs";
const DEFAULT_LOGS_INDEX: &str = "docs/logs/README.md";
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

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.link-check.v1",
            "schema_version": 1,
            "ok": failures.is_empty(),
            "checked_files": files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "broken_links": failures.iter().map(|failure| {
                json!({
                    "file": failure.file.display().to_string(),
                    "target": failure.target,
                    "reason": failure.reason,
                })
            }).collect::<Vec<_>>(),
        });
        return if failures.is_empty() {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if failures.is_empty() {
        return Ok("link check passed".to_owned());
    }

    let mut output = String::new();
    for failure in &failures {
        output.push_str(&format!(
            "broken link: {} -> {} ({})\n",
            failure.file.display(),
            failure.target,
            failure.reason
        ));
    }
    output.push_str(&format!(
        "\nlink check failed: {} broken link(s)",
        failures.len()
    ));
    Err(RunnerError::task_invocation(output))
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
    let required_blocks = if required_blocks_override.is_empty() {
        default_json_example_block_requirements()
    } else {
        required_blocks_override.to_vec()
    };

    let content = std::fs::read_to_string(&file)
        .map_err(|err| RunnerError::task_invocation_failed_read(&file, err))?;
    let section = extract_h2_section(&content, section_title).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "section `{section_title}` not found in {}",
            file.display()
        ))
    })?;
    let blocks = extract_fenced_json_blocks(&section);

    let mut failures = Vec::new();
    if blocks.len() < min_blocks {
        failures.push(format!(
            "expected at least {min_blocks} JSON example block(s), found {}",
            blocks.len()
        ));
    }

    for needle in &required {
        for (index, block) in blocks.iter().enumerate() {
            if !block.contains(needle) {
                failures.push(format!(
                    "missing `{needle}` in JSON example block #{}",
                    index + 1
                ));
            }
        }
    }

    for requirement in &required_blocks {
        let block = blocks.get(requirement.block_index.saturating_sub(1));
        match block {
            Some(block) if block.contains(&requirement.needle) => {}
            Some(_) => failures.push(format!(
                "missing `{}` in JSON example block #{}",
                requirement.needle, requirement.block_index
            )),
            None => failures.push(format!(
                "required JSON example block #{} is missing",
                requirement.block_index
            )),
        }
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.json-examples.v1",
            "schema_version": 1,
            "ok": failures.is_empty(),
            "file": file.display().to_string(),
            "section": section_title,
            "block_count": blocks.len(),
            "min_blocks": min_blocks,
            "required": required,
            "required_blocks": required_blocks.iter().map(|requirement| {
                json!({
                    "block_index": requirement.block_index,
                    "needle": requirement.needle,
                })
            }).collect::<Vec<_>>(),
            "failures": failures,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if failures.is_empty() {
        return Ok("examples json check passed".to_owned());
    }

    Err(RunnerError::task_invocation(failures.join("\n")))
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

    let files = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for file in &files {
        let content = std::fs::read_to_string(file)
            .map_err(|err| RunnerError::task_invocation_failed_read(file, err))?;
        for heading in required_headings {
            if !content.lines().any(|line| line.trim() == heading.trim()) {
                findings.push(json!({
                    "file": file.display().to_string(),
                    "kind": "missing-heading",
                    "heading": heading,
                }));
            }
        }
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.heading-check.v1",
            "schema_version": 1,
            "ok": findings.is_empty(),
            "files": files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "required_headings": required_headings,
            "findings": findings,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if findings.is_empty() {
        return Ok("docs heading check passed".to_owned());
    }

    let mut output = String::new();
    for finding in findings {
        output.push_str(&format!(
            "missing heading `{}` in {}\n",
            finding["heading"].as_str().unwrap_or_default(),
            finding["file"].as_str().unwrap_or_default()
        ));
    }
    Err(RunnerError::task_invocation(output.trim_end().to_owned()))
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

    let files = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for file in &files {
        let content = std::fs::read_to_string(file)
            .map_err(|err| RunnerError::task_invocation_failed_read(file, err))?;
        for needle in required_text {
            if !content.contains(needle) {
                findings.push(json!({
                    "file": file.display().to_string(),
                    "kind": "missing-text",
                    "needle": needle,
                }));
            }
        }
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.contains-check.v1",
            "schema_version": 1,
            "ok": findings.is_empty(),
            "files": files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "required_text": required_text,
            "findings": findings,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if findings.is_empty() {
        return Ok("docs contains check passed".to_owned());
    }

    let mut output = String::new();
    for finding in findings {
        output.push_str(&format!(
            "missing text `{}` in {}\n",
            finding["needle"].as_str().unwrap_or_default(),
            finding["file"].as_str().unwrap_or_default()
        ));
    }
    Err(RunnerError::task_invocation(output.trim_end().to_owned()))
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

    let resolved_paths = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let findings = resolved_paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| {
            json!({
                "path": path.display().to_string(),
                "kind": "missing-path",
            })
        })
        .collect::<Vec<_>>();

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.path-check.v1",
            "schema_version": 1,
            "ok": findings.is_empty(),
            "paths": resolved_paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "findings": findings,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if findings.is_empty() {
        return Ok("docs path check passed".to_owned());
    }

    let mut output = String::new();
    for finding in findings {
        output.push_str(&format!(
            "missing path {}\n",
            finding["path"].as_str().unwrap_or_default()
        ));
    }
    Err(RunnerError::task_invocation(output.trim_end().to_owned()))
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

    let files = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for file in &files {
        let content = std::fs::read_to_string(file)
            .map_err(|err| RunnerError::task_invocation_failed_read(file, err))?;
        for needle in forbidden_text {
            if content.contains(needle) {
                findings.push(json!({
                    "file": file.display().to_string(),
                    "kind": "forbidden-text",
                    "needle": needle,
                }));
            }
        }
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.forbidden-check.v1",
            "schema_version": 1,
            "ok": findings.is_empty(),
            "files": files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "forbidden_text": forbidden_text,
            "findings": findings,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if findings.is_empty() {
        return Ok("docs forbidden check passed".to_owned());
    }

    let mut output = String::new();
    for finding in findings {
        output.push_str(&format!(
            "forbidden text `{}` in {}\n",
            finding["needle"].as_str().unwrap_or_default(),
            finding["file"].as_str().unwrap_or_default()
        ));
    }
    Err(RunnerError::task_invocation(output.trim_end().to_owned()))
}

fn run_check_index(
    repo_root: &Path,
    policy_index: Option<&str>,
    dir_override: Option<&PathBuf>,
    index_override: Option<&PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let spec = resolve_docs_index_spec(repo_root, policy_index, dir_override, index_override)?;

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
    let indexed = collect_index_markdown_links(&spec.index, spec.section.as_deref())?;
    let missing = all_docs.difference(&indexed).cloned().collect::<Vec<_>>();
    let extra = indexed.difference(&all_docs).cloned().collect::<Vec<_>>();

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.index-check.v1",
            "schema_version": 1,
            "ok": missing.is_empty() && extra.is_empty(),
            "dir": spec.dir.display().to_string(),
            "index": spec.index.display().to_string(),
            "policy_index": spec.policy_name,
            "section": spec.section,
            "missing": missing,
            "extra": extra,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if missing.is_empty() && extra.is_empty() {
        return Ok(match spec.policy_name.as_deref() {
            Some(name) => format!("docs index check passed ({name})"),
            None => "docs index check passed".to_owned(),
        });
    }

    let mut output = String::new();
    if !missing.is_empty() {
        output.push_str("docs index is missing entries:\n");
        for entry in &missing {
            output.push_str(&format!("  - {entry}\n"));
        }
    }
    if !extra.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("docs index references non-existent markdown files:\n");
        for entry in &extra {
            output.push_str(&format!("  - {entry}\n"));
        }
    }
    Err(RunnerError::task_invocation(output.trim_end().to_owned()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocsIndexSpec {
    policy_name: Option<String>,
    dir: PathBuf,
    index: PathBuf,
    section: Option<String>,
    exclude: Vec<String>,
}

fn resolve_docs_index_spec(
    repo_root: &Path,
    policy_index: Option<&str>,
    dir_override: Option<&PathBuf>,
    index_override: Option<&PathBuf>,
) -> Result<DocsIndexSpec, RunnerError> {
    let policy = load_docs_policy_config(repo_root)?;
    let configured =
        policy_index.and_then(|name| policy.indexes.get(name).map(|entry| (name, entry)));

    if let Some(name) = policy_index {
        if configured.is_none() {
            let available = policy.indexes.keys().cloned().collect::<Vec<_>>();
            let suffix = if available.is_empty() {
                "no `[docs_policy.indexes]` entries are configured".to_owned()
            } else {
                format!("available indexes: {}", available.join(", "))
            };
            return Err(RunnerError::task_invocation(format!(
                "unknown docs policy index `{name}` in `effigy.toml`; {suffix}"
            )));
        }
    }

    let dir = resolve_repo_input(
        repo_root,
        dir_override
            .cloned()
            .or_else(|| configured.map(|(_, entry)| PathBuf::from(entry.dir.clone())))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_LOGS_DIR)),
    );
    let index = resolve_repo_input(
        repo_root,
        index_override
            .cloned()
            .or_else(|| configured.map(|(_, entry)| PathBuf::from(entry.file.clone())))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_LOGS_INDEX)),
    );

    Ok(DocsIndexSpec {
        policy_name: policy_index.map(ToOwned::to_owned),
        dir,
        index,
        section: configured.and_then(|(_, entry)| entry.section.clone()),
        exclude: configured
            .map(|(_, entry)| entry.exclude.clone())
            .unwrap_or_default(),
    })
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocsNextActionSpec {
    policy_name: Option<String>,
    index: DocsIndexSpec,
    heading: String,
    heading_without_hashes: String,
    allowlist_file: PathBuf,
}

fn run_check_next_action(
    repo_root: &Path,
    policy_name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let spec = resolve_docs_next_action_spec(repo_root, policy_name)?;
    let referenced =
        collect_index_markdown_links(&spec.index.index, spec.index.section.as_deref())?;
    let allowlist = load_next_action_allowlist(&spec.allowlist_file)?;
    let mut findings = Vec::new();

    for relative in referenced {
        let file = spec.index.dir.join(&relative);
        if !file.is_file() {
            findings.push(json!({
                "file": file.display().to_string(),
                "relative": relative,
                "kind": "missing-file",
                "message": format!("missing indexed markdown file: {}", file.display()),
            }));
            continue;
        }

        let content = std::fs::read_to_string(&file)
            .map_err(|err| RunnerError::task_invocation_failed_read(&file, err))?;
        let Some(section) = extract_h2_section(&content, &spec.heading_without_hashes) else {
            findings.push(json!({
                "file": file.display().to_string(),
                "relative": relative,
                "kind": "missing-heading",
                "message": format!("missing `{}` section in {}", spec.heading, file.display()),
            }));
            continue;
        };
        let Some(first_line) = first_non_empty_section_line(&section) else {
            findings.push(json!({
                "file": file.display().to_string(),
                "relative": relative,
                "kind": "empty-section",
                "message": format!("empty `{}` section in {}", spec.heading, file.display()),
            }));
            continue;
        };
        let verb = extract_lead_verb(&first_line);
        if verb.is_empty() || !allowlist.contains(&verb) {
            findings.push(json!({
                "file": file.display().to_string(),
                "relative": relative,
                "kind": "non-actionable",
                "message": format!(
                    "non-actionable `{}` lead verb in {}: `{}`",
                    spec.heading,
                    file.display(),
                    first_line
                ),
                "line": first_line,
                "verb": verb,
                "allowlist_file": spec.allowlist_file.display().to_string(),
            }));
        }
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.next-action-check.v1",
            "schema_version": 1,
            "ok": findings.is_empty(),
            "policy": spec.policy_name,
            "heading": spec.heading,
            "index": spec.index.index.display().to_string(),
            "dir": spec.index.dir.display().to_string(),
            "allowlist_file": spec.allowlist_file.display().to_string(),
            "findings": findings,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if findings.is_empty() {
        return Ok(match spec.policy_name.as_deref() {
            Some(name) => format!("docs next-action check passed ({name})"),
            None => "docs next-action check passed".to_owned(),
        });
    }

    let mut output = String::new();
    for finding in findings {
        output.push_str(finding["message"].as_str().unwrap_or_default());
        output.push('\n');
    }
    Err(RunnerError::task_invocation(output.trim_end().to_owned()))
}

fn resolve_docs_next_action_spec(
    repo_root: &Path,
    policy_name: Option<&str>,
) -> Result<DocsNextActionSpec, RunnerError> {
    let policy = load_docs_policy_config(repo_root)?;
    let name = policy_name.unwrap_or("vision");
    let Some(entry) = policy.next_actions.get(name) else {
        let available = policy.next_actions.keys().cloned().collect::<Vec<_>>();
        let suffix = if available.is_empty() {
            "no `[docs_policy.next_actions]` entries are configured".to_owned()
        } else {
            format!("available next-action policies: {}", available.join(", "))
        };
        return Err(RunnerError::task_invocation(format!(
            "unknown docs next-action policy `{name}` in `effigy.toml`; {suffix}"
        )));
    };

    let heading_without_hashes = entry
        .heading
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_owned();
    if heading_without_hashes.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "invalid docs next-action heading for policy `{name}`: heading cannot be empty"
        )));
    }

    Ok(DocsNextActionSpec {
        policy_name: Some(name.to_owned()),
        index: resolve_docs_index_spec(repo_root, Some(&entry.index), None, None)?,
        heading: format!("## {heading_without_hashes}"),
        heading_without_hashes,
        allowlist_file: resolve_repo_input(repo_root, PathBuf::from(&entry.allowlist_file)),
    })
}

fn load_next_action_allowlist(path: &Path) -> Result<BTreeSet<String>, RunnerError> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| RunnerError::task_invocation_failed_read(path, err))?;
    let mut verbs = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line
            .split('#')
            .next()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if !trimmed.is_empty() {
            verbs.insert(trimmed);
        }
    }
    if verbs.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "actionable verb allowlist is empty: {}",
            path.display()
        )));
    }
    Ok(verbs)
}

fn run_add_log_index(
    repo_root: &Path,
    log_path: &Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    let index_path = repo_root.join(DEFAULT_LOGS_INDEX);
    if !index_path.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "logs index not found: {}",
            index_path.display()
        )));
    }

    let relative_path = normalize_log_index_relative_path(log_path)?;
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

    if output_json {
        return Ok(json!({
            "schema": "effigy.docs.add-log-index.v1",
            "schema_version": 1,
            "ok": true,
            "log": relative_path,
            "index": index_path.display().to_string(),
            "already_indexed": already_indexed,
        })
        .to_string());
    }

    if already_indexed {
        Ok(format!("log already indexed: {relative_path}"))
    } else {
        Ok(format!("indexed log: {relative_path}"))
    }
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
    let workflow_re = Regex::new(r"\.github(-bak)?/workflows/[A-Za-z0-9._-]+\.ya?ml")
        .expect("workflow path regex");
    let mut findings = Vec::new();

    for path in collect_workflow_check_files(&dir, &default_logs_dir, dir_override.is_none()) {
        let content = std::fs::read_to_string(&path)
            .map_err(|err| RunnerError::task_invocation_failed_read(&path, err))?;
        for (line_index, line) in content.lines().enumerate() {
            for hit in workflow_re.find_iter(line) {
                let workflow_path = hit.as_str();
                let candidate = repo_root.join(workflow_path);
                if candidate.is_file() {
                    continue;
                }

                let mut reason = "missing workflow path".to_owned();
                let mut suggestion = None;
                if let Some(name) = workflow_path.strip_prefix(".github/workflows/") {
                    let alt = format!(".github-bak/workflows/{name}");
                    if repo_root.join(&alt).is_file() {
                        reason = "stale workflow path".to_owned();
                        suggestion = Some(alt);
                    }
                } else if let Some(name) = workflow_path.strip_prefix(".github-bak/workflows/") {
                    let alt = format!(".github/workflows/{name}");
                    if repo_root.join(&alt).is_file() {
                        reason = "stale workflow path".to_owned();
                        suggestion = Some(alt);
                    }
                }

                findings.push(json!({
                    "file": path.display().to_string(),
                    "line": line_index + 1,
                    "workflow_path": workflow_path,
                    "reason": reason,
                    "suggestion": suggestion,
                }));
            }
        }
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.docs.workflow-path-check.v1",
            "schema_version": 1,
            "ok": findings.is_empty(),
            "dir": dir.display().to_string(),
            "findings": findings,
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if findings.is_empty() {
        return Ok("doc workflow path check passed".to_owned());
    }

    let mut output = String::new();
    for finding in findings {
        let file = finding["file"].as_str().unwrap_or_default();
        let line = finding["line"].as_u64().unwrap_or_default();
        let workflow_path = finding["workflow_path"].as_str().unwrap_or_default();
        let reason = finding["reason"].as_str().unwrap_or_default();
        if let Some(suggestion) = finding["suggestion"].as_str() {
            output.push_str(&format!(
                "{reason} in {file}:{line}: {workflow_path} (use {suggestion})\n"
            ));
        } else {
            output.push_str(&format!("{reason} in {file}:{line}: {workflow_path}\n"));
        }
    }
    Err(RunnerError::task_invocation(output.trim_end().to_owned()))
}

#[derive(Debug)]
struct BrokenLink {
    file: PathBuf,
    target: String,
    reason: String,
}

fn collect_link_check_files(repo_root: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    if !paths.is_empty() {
        return paths
            .iter()
            .map(|path| resolve_repo_input(repo_root, path.clone()))
            .filter(|path| path.is_file())
            .collect();
    }

    let mut defaults = DEFAULT_LINK_FILES
        .iter()
        .map(|path| repo_root.join(path))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();

    let docs_dir = repo_root.join(DEFAULT_LINK_DOCS_DIR);
    if docs_dir.is_dir() {
        defaults.extend(
            WalkDir::new(docs_dir)
                .min_depth(1)
                .into_iter()
                .filter_map(Result::ok)
                .map(|entry| entry.into_path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md")),
        );
    }
    defaults.sort();
    defaults.dedup();
    defaults
}

fn scan_markdown_links(file: &Path) -> Result<Vec<BrokenLink>, BrokenLink> {
    let content = std::fs::read_to_string(file).map_err(|err| BrokenLink {
        file: file.to_path_buf(),
        target: file.display().to_string(),
        reason: err.to_string(),
    })?;
    let link_re = Regex::new(r"\[[^\]]+\]\(([^)]+)\)").expect("link regex");
    let code_re = Regex::new(r"`[^`]*`").expect("inline code regex");
    let mut in_fence = false;
    let mut failures = Vec::new();

    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let sanitized = code_re.replace_all(line, "");
        for capture in link_re.captures_iter(&sanitized) {
            let target = capture[1].to_owned();
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with('#')
            {
                continue;
            }
            let stripped = target.split('#').next().unwrap_or_default().trim();
            if stripped.is_empty() {
                continue;
            }
            let resolved = file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(stripped);
            if !resolved.exists() {
                failures.push(BrokenLink {
                    file: file.to_path_buf(),
                    target: target.clone(),
                    reason: "target does not exist".to_owned(),
                });
            }
        }
    }

    Ok(failures)
}

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn normalize_log_index_relative_path(log_path: &Path) -> Result<String, RunnerError> {
    let raw = log_path.to_string_lossy().replace('\\', "/");
    let trimmed = raw.trim_start_matches("./");
    let relative = trimmed
        .strip_prefix("docs/logs/")
        .unwrap_or(trimmed)
        .to_owned();

    if relative == "README.md" {
        return Err(RunnerError::task_invocation(
            "README.md is not a log artifact".to_owned(),
        ));
    }
    if !relative.ends_with(".md") {
        return Err(RunnerError::task_invocation(format!(
            "log must be a .md file: {relative}"
        )));
    }
    let path_re =
        Regex::new(r"^[0-9]{4}-[0-9]{2}/[0-9]{2}-[0-9]{6}-.+\.md$").expect("logs path regex");
    if !path_re.is_match(&relative) {
        return Err(RunnerError::task_invocation(format!(
            "log path must match YYYY-MM/DD-HHMMSS-slug.md: {relative}"
        )));
    }

    Ok(relative)
}

fn insert_log_index_entry(index_contents: &str, entry: &str) -> String {
    let marker = "## Archived Validation Logs";
    if let Some(position) = index_contents.find(marker) {
        let (before, after) = index_contents.split_at(position);
        let mut output = String::new();
        output.push_str(before);
        output.push_str(entry);
        output.push_str("\n\n");
        output.push_str(after);
        output
    } else {
        let mut output = index_contents.trim_end().to_owned();
        output.push_str("\n\n");
        output.push_str(entry);
        output.push('\n');
        output
    }
}

fn collect_workflow_check_files(dir: &Path, logs_dir: &Path, exclude_logs: bool) -> Vec<PathBuf> {
    let mut files = WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .filter(|path| !(exclude_logs && path.starts_with(logs_dir)))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn normalize_section_title(title: &str) -> &str {
    let trimmed = title.trim();
    let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
    let rest = rest
        .strip_prefix(") ")
        .or_else(|| rest.strip_prefix(". "))
        .unwrap_or(trimmed);
    rest.trim()
}

fn extract_h2_section(content: &str, section_title: &str) -> Option<String> {
    let mut in_section = false;
    let mut lines = Vec::new();
    let wanted = normalize_section_title(section_title);

    for line in content.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            let title = title.trim();
            let normalized = normalize_section_title(title);
            let is_match = normalized == wanted || normalized.starts_with(wanted);
            if in_section && !is_match {
                break;
            }
            if is_match {
                in_section = true;
            }
        }
        if in_section {
            lines.push(line);
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn extract_fenced_json_blocks(section: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    let mut in_block = false;

    for line in section.lines() {
        if line.trim() == "```json" {
            in_block = true;
            current.clear();
            continue;
        }
        if in_block && line.trim() == "```" {
            blocks.push(current.join("\n"));
            current.clear();
            in_block = false;
            continue;
        }
        if in_block {
            current.push(line);
        }
    }

    blocks
}

fn first_non_empty_section_line(section: &str) -> Option<String> {
    section
        .lines()
        .skip(1)
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_lead_verb(line: &str) -> String {
    let normalized = line
        .trim_start()
        .trim_start_matches(['-', '*', '+'])
        .trim_start();
    let normalized = if let Some(rest) = normalized.strip_prefix('(') {
        rest.split_once(')')
            .map(|(_, tail)| tail.trim_start())
            .unwrap_or(normalized)
    } else {
        normalized
    };
    let normalized = normalized
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
        .trim_start();
    normalized
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase()
}

fn default_json_example_requirements() -> Vec<String> {
    vec![
        "\"schema\": \"effigy.completion.candidates.v1\"".to_owned(),
        "\"schema_version\": 1".to_owned(),
        "\"cache_state\":".to_owned(),
        "\"cache_age_ms\":".to_owned(),
        "\"cache_ttl_ms\":".to_owned(),
        "\"effective_cache_ttl_ms\":".to_owned(),
        "\"cache_ttl_source\":".to_owned(),
    ]
}

fn default_json_example_block_requirements() -> Vec<DocsBlockRequirement> {
    vec![
        DocsBlockRequirement {
            block_index: 1,
            needle: "\"cache_state\": \"hit\"".to_owned(),
        },
        DocsBlockRequirement {
            block_index: 2,
            needle: "\"cache_hit\": false".to_owned(),
        },
    ]
}

fn collect_markdown_children(dir: &Path, exclude: &[String]) -> BTreeSet<String> {
    WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .filter(|path| !path_matches_any_exclude(path, dir, exclude))
        .filter_map(|path| {
            path.strip_prefix(dir)
                .ok()
                .map(|relative| relative.to_path_buf())
        })
        .filter(|path| path.as_path() != Path::new("README.md"))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect()
}

fn collect_index_markdown_links(
    index: &Path,
    section: Option<&str>,
) -> Result<BTreeSet<String>, RunnerError> {
    let content = std::fs::read_to_string(index)
        .map_err(|err| RunnerError::task_invocation_failed_read(index, err))?;
    let content = if let Some(section_name) = section {
        extract_h2_section(&content, section_name).ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "section `{section_name}` not found in {}",
                index.display()
            ))
        })?
    } else {
        content
    };
    let link_re = Regex::new(r"\((\./[^)]+\.md)\)").expect("index link regex");
    let mut links = BTreeSet::new();
    for capture in link_re.captures_iter(&content) {
        let relative = capture[1].trim_start_matches("./");
        links.insert(relative.replace('\\', "/"));
    }
    Ok(links)
}

fn path_matches_any_exclude(path: &Path, root: &Path, exclude: &[String]) -> bool {
    let relative = match path.strip_prefix(root) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => return false,
    };
    exclude
        .iter()
        .any(|pattern| path_matches_exclude(&relative, pattern))
}

fn path_matches_exclude(relative: &str, pattern: &str) -> bool {
    let normalized = pattern.trim_start_matches("./").replace('\\', "/");
    if let Some(prefix) = normalized.strip_suffix("/**") {
        relative == prefix || relative.starts_with(&format!("{prefix}/"))
    } else {
        relative == normalized || relative.starts_with(&format!("{normalized}/"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_index_markdown_links, collect_link_check_files, collect_markdown_children,
        collect_workflow_check_files, extract_fenced_json_blocks, extract_h2_section,
        extract_lead_verb, first_non_empty_section_line, insert_log_index_entry,
        normalize_log_index_relative_path, path_matches_exclude, resolve_docs_index_spec,
        resolve_docs_next_action_spec, scan_markdown_links,
    };
    use std::{fs, path::Path};

    #[test]
    fn extract_h2_section_returns_requested_section_only() {
        let content = "## One\nalpha\n## Two\nbeta\n## Three\ngamma\n";
        let section = extract_h2_section(content, "Two").expect("section");
        assert_eq!(section, "## Two\nbeta");
    }

    #[test]
    fn extract_h2_section_matches_numbered_heading_without_ordinal() {
        let content =
            "## 8) Bootstrap (`effigy.bootstrap.v1`)\nalpha\n## 19) Completion Candidates (`effigy.completion.candidates.v1`)\nbeta\n";
        let section = extract_h2_section(
            content,
            "Completion Candidates (`effigy.completion.candidates.v1`)",
        )
        .expect("section");
        assert_eq!(
            section,
            "## 19) Completion Candidates (`effigy.completion.candidates.v1`)\nbeta"
        );
    }

    #[test]
    fn extract_fenced_json_blocks_returns_json_blocks_only() {
        let section = "## Two\n```json\n{\"ok\":true}\n```\n```txt\nignored\n```\n```json\n{\"ok\":false}\n```\n";
        let blocks = extract_fenced_json_blocks(section);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("{\"ok\":true}"));
        assert!(blocks[1].contains("{\"ok\":false}"));
    }

    #[test]
    fn scan_markdown_links_ignores_fenced_code_blocks() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-links-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        let markdown = root.join("README.md");
        fs::write(
            &markdown,
            "[ok](./existing.md)\n```md\n[skip](./missing.md)\n```\n",
        )
        .expect("write markdown");
        fs::write(root.join("existing.md"), "exists\n").expect("write existing");

        let failures = scan_markdown_links(&markdown).expect("scan");
        assert!(failures.is_empty());
    }

    #[test]
    fn collect_link_check_files_defaults_to_full_docs_tree() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-link-defaults-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
        fs::create_dir_all(root.join("docs/research")).expect("mkdir research");
        fs::write(root.join("README.md"), "# Root\n").expect("write root");
        fs::write(root.join("docs/README.md"), "# Docs\n").expect("write docs readme");
        fs::write(root.join("docs/logs/2026-03/example.md"), "# Log\n").expect("write log");
        fs::write(root.join("docs/research/example.md"), "# Research\n").expect("write research");

        let files = collect_link_check_files(&root, &[]);
        let rendered = files
            .iter()
            .filter_map(|path| path.strip_prefix(&root).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();

        assert!(rendered.contains(&"README.md".to_owned()));
        assert!(rendered.contains(&"docs/README.md".to_owned()));
        assert!(rendered.contains(&"docs/logs/2026-03/example.md".to_owned()));
        assert!(rendered.contains(&"docs/research/example.md".to_owned()));
    }

    #[test]
    fn normalize_log_index_relative_path_accepts_docs_logs_prefix() {
        let normalized =
            normalize_log_index_relative_path(Path::new("docs/logs/2026-03/02-160000-my-log.md"))
                .expect("normalize path");
        assert_eq!(normalized, "2026-03/02-160000-my-log.md");
    }

    #[test]
    fn insert_log_index_entry_places_new_entry_before_archive_marker() {
        let index = "# Logs\n\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Archived Validation Logs\n- older\n";
        let updated = insert_log_index_entry(
            index,
            "- [`2026-03/02-160000-my-log.md`](./2026-03/02-160000-my-log.md)",
        );
        let marker = updated.find("## Archived Validation Logs").expect("marker");
        let entry = updated.find("2026-03/02-160000-my-log.md").expect("entry");
        assert!(entry < marker);
    }

    #[test]
    fn collect_workflow_check_files_excludes_logs_for_default_docs_scope() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-workflow-paths-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
        fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
        fs::write(root.join("docs/guides/example.md"), "# Guide\n").expect("write guide");
        fs::write(root.join("docs/logs/2026-03/example.md"), "# Log\n").expect("write log");

        let files = collect_workflow_check_files(&root.join("docs"), &root.join("docs/logs"), true);
        let rendered = files
            .iter()
            .filter_map(|path| path.strip_prefix(&root).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();

        assert!(rendered.contains(&"docs/guides/example.md".to_owned()));
        assert!(!rendered.contains(&"docs/logs/2026-03/example.md".to_owned()));
    }

    #[test]
    fn collect_markdown_children_respects_excludes() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-index-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("history")).expect("mkdir history");
        fs::write(root.join("README.md"), "# Root\n").expect("write readme");
        fs::write(root.join("active.md"), "# Active\n").expect("write active");
        fs::write(root.join("history/old.md"), "# Old\n").expect("write old");

        let files = collect_markdown_children(&root, &[String::from("history/**")]);
        assert!(files.contains("active.md"));
        assert!(!files.contains("history/old.md"));
    }

    #[test]
    fn collect_index_markdown_links_can_scope_to_section() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-index-section-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        let index = root.join("README.md");
        fs::write(
            &index,
            "# Root\n\n## Vision Artifacts\n- [One](./one.md)\n\n## Other\n- [Two](./two.md)\n",
        )
        .expect("write index");

        let links = collect_index_markdown_links(&index, Some("Vision Artifacts")).expect("links");
        assert!(links.contains("one.md"));
        assert!(!links.contains("two.md"));
    }

    #[test]
    fn path_matches_exclude_supports_recursive_suffix() {
        assert!(path_matches_exclude("history/one.md", "history/**"));
        assert!(!path_matches_exclude("active/one.md", "history/**"));
    }

    #[test]
    fn resolve_docs_index_spec_loads_named_policy_index() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-policy-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(
            root.join("effigy.toml"),
            "[docs_policy.indexes.vision]\nfile = \"docs/vision/README.md\"\ndir = \"docs/vision\"\nsection = \"Vision Artifacts\"\nexclude = [\"history/**\"]\n",
        )
        .expect("write manifest");

        let spec = resolve_docs_index_spec(&root, Some("vision"), None, None).expect("spec");
        assert_eq!(spec.policy_name.as_deref(), Some("vision"));
        assert_eq!(spec.index, root.join("docs/vision/README.md"));
        assert_eq!(spec.dir, root.join("docs/vision"));
        assert_eq!(spec.section.as_deref(), Some("Vision Artifacts"));
        assert_eq!(spec.exclude, vec!["history/**"]);
    }

    #[test]
    fn first_non_empty_section_line_skips_heading_and_blank_lines() {
        let line = first_non_empty_section_line("## Next Task\n\nShip the thing.\n").expect("line");
        assert_eq!(line, "Ship the thing.");
    }

    #[test]
    fn extract_lead_verb_handles_bullets_and_numbering() {
        assert_eq!(extract_lead_verb("- Execute cleanup."), "execute");
        assert_eq!(extract_lead_verb("1. Review follow-up."), "review");
        assert_eq!(extract_lead_verb("(1) Ship parity."), "ship");
    }

    #[test]
    fn resolve_docs_next_action_spec_loads_named_policy() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-next-action-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("docs/scripts/fixtures")).expect("mkdir");
        fs::write(
            root.join("effigy.toml"),
            "[docs_policy.indexes.vision]\nfile = \"docs/vision/README.md\"\ndir = \"docs/vision\"\nsection = \"Vision Artifacts\"\n\n[docs_policy.next_actions.vision]\nindex = \"vision\"\nheading = \"## Next Task\"\nallowlist_file = \"docs/scripts/fixtures/verbs.txt\"\n",
        )
        .expect("write manifest");

        let spec = resolve_docs_next_action_spec(&root, Some("vision")).expect("spec");
        assert_eq!(spec.policy_name.as_deref(), Some("vision"));
        assert_eq!(spec.heading, "## Next Task");
        assert_eq!(spec.heading_without_hashes, "Next Task");
        assert_eq!(
            spec.allowlist_file,
            root.join("docs/scripts/fixtures/verbs.txt")
        );
        assert_eq!(spec.index.policy_name.as_deref(), Some("vision"));
    }
}
