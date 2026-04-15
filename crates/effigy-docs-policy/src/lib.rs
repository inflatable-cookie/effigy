use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_manifest::ManifestDocsPolicyConfig;
use regex::Regex;
use walkdir::WalkDir;

#[derive(Debug)]
pub enum DocsPolicyError {
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    Message(String),
}

impl std::fmt::Display for DocsPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "failed to read {}: {error}", path.display()),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for DocsPolicyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenLink {
    pub file: PathBuf,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingHeadingFinding {
    pub file: PathBuf,
    pub heading: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFinding {
    pub file: PathBuf,
    pub needle: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingPathFinding {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowPathFinding {
    pub file: PathBuf,
    pub line: usize,
    pub workflow_path: String,
    pub reason: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsIndexSpec {
    pub policy_name: Option<String>,
    pub dir: PathBuf,
    pub index: PathBuf,
    pub section: Option<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsNextActionSpec {
    pub policy_name: Option<String>,
    pub index: DocsIndexSpec,
    pub heading: String,
    pub heading_without_hashes: String,
    pub allowlist_file: PathBuf,
}

pub fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

pub fn collect_link_check_files(repo_root: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    if !paths.is_empty() {
        return paths
            .iter()
            .map(|path| resolve_repo_input(repo_root, path.clone()))
            .filter(|path| path.is_file())
            .collect();
    }

    let mut defaults = ["README.md"]
        .iter()
        .map(|path| repo_root.join(path))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();

    let docs_dir = repo_root.join("docs");
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

pub fn scan_markdown_links(file: &Path) -> Result<Vec<BrokenLink>, BrokenLink> {
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

pub fn resolve_docs_index_spec(
    repo_root: &Path,
    policy: &ManifestDocsPolicyConfig,
    policy_index: Option<&str>,
    dir_override: Option<&PathBuf>,
    index_override: Option<&PathBuf>,
) -> Result<DocsIndexSpec, DocsPolicyError> {
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
            return Err(DocsPolicyError::Message(format!(
                "unknown docs policy index `{name}` in `effigy.toml`; {suffix}"
            )));
        }
    }

    let dir = resolve_repo_input(
        repo_root,
        dir_override
            .cloned()
            .or_else(|| configured.map(|(_, entry)| PathBuf::from(entry.dir.clone())))
            .unwrap_or_else(|| PathBuf::from("docs/logs")),
    );
    let index = resolve_repo_input(
        repo_root,
        index_override
            .cloned()
            .or_else(|| configured.map(|(_, entry)| PathBuf::from(entry.file.clone())))
            .unwrap_or_else(|| PathBuf::from("docs/logs/README.md")),
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

pub fn resolve_docs_next_action_spec(
    repo_root: &Path,
    policy: &ManifestDocsPolicyConfig,
    policy_name: Option<&str>,
) -> Result<DocsNextActionSpec, DocsPolicyError> {
    let name = policy_name.unwrap_or("vision");
    let Some(entry) = policy.next_actions.get(name) else {
        let available = policy.next_actions.keys().cloned().collect::<Vec<_>>();
        let suffix = if available.is_empty() {
            "no `[docs_policy.next_actions]` entries are configured".to_owned()
        } else {
            format!("available next-action policies: {}", available.join(", "))
        };
        return Err(DocsPolicyError::Message(format!(
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
        return Err(DocsPolicyError::Message(format!(
            "invalid docs next-action heading for policy `{name}`: heading cannot be empty"
        )));
    }

    Ok(DocsNextActionSpec {
        policy_name: Some(name.to_owned()),
        index: resolve_docs_index_spec(repo_root, policy, Some(&entry.index), None, None)?,
        heading: format!("## {heading_without_hashes}"),
        heading_without_hashes,
        allowlist_file: resolve_repo_input(repo_root, PathBuf::from(&entry.allowlist_file)),
    })
}

pub fn load_next_action_allowlist(path: &Path) -> Result<BTreeSet<String>, DocsPolicyError> {
    let content = std::fs::read_to_string(path).map_err(|error| DocsPolicyError::Io {
        path: path.to_path_buf(),
        error,
    })?;
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
        return Err(DocsPolicyError::Message(format!(
            "actionable verb allowlist is empty: {}",
            path.display()
        )));
    }
    Ok(verbs)
}

pub fn check_headings(
    repo_root: &Path,
    paths: &[PathBuf],
    required_headings: &[String],
) -> Result<(Vec<PathBuf>, Vec<MissingHeadingFinding>), DocsPolicyError> {
    let files = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for file in &files {
        let content = std::fs::read_to_string(file).map_err(|error| DocsPolicyError::Io {
            path: file.clone(),
            error,
        })?;
        for heading in required_headings {
            if !content.lines().any(|line| line.trim() == heading.trim()) {
                findings.push(MissingHeadingFinding {
                    file: file.clone(),
                    heading: heading.clone(),
                });
            }
        }
    }
    Ok((files, findings))
}

pub fn check_contains(
    repo_root: &Path,
    paths: &[PathBuf],
    required_text: &[String],
) -> Result<(Vec<PathBuf>, Vec<TextFinding>), DocsPolicyError> {
    let files = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for file in &files {
        let content = std::fs::read_to_string(file).map_err(|error| DocsPolicyError::Io {
            path: file.clone(),
            error,
        })?;
        for needle in required_text {
            if !content.contains(needle) {
                findings.push(TextFinding {
                    file: file.clone(),
                    needle: needle.clone(),
                });
            }
        }
    }
    Ok((files, findings))
}

pub fn check_forbidden(
    repo_root: &Path,
    paths: &[PathBuf],
    forbidden_text: &[String],
) -> Result<(Vec<PathBuf>, Vec<TextFinding>), DocsPolicyError> {
    let files = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for file in &files {
        let content = std::fs::read_to_string(file).map_err(|error| DocsPolicyError::Io {
            path: file.clone(),
            error,
        })?;
        for needle in forbidden_text {
            if content.contains(needle) {
                findings.push(TextFinding {
                    file: file.clone(),
                    needle: needle.clone(),
                });
            }
        }
    }
    Ok((files, findings))
}

pub fn check_paths(repo_root: &Path, paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<MissingPathFinding>) {
    let resolved_paths = paths
        .iter()
        .map(|path| resolve_repo_input(repo_root, path.clone()))
        .collect::<Vec<_>>();
    let findings = resolved_paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| MissingPathFinding { path: path.clone() })
        .collect::<Vec<_>>();
    (resolved_paths, findings)
}

pub fn normalize_log_index_relative_path(log_path: &Path) -> Result<String, DocsPolicyError> {
    let raw = log_path.to_string_lossy().replace('\\', "/");
    let trimmed = raw.trim_start_matches("./");
    let relative = trimmed
        .strip_prefix("docs/logs/")
        .unwrap_or(trimmed)
        .to_owned();

    if relative == "README.md" {
        return Err(DocsPolicyError::Message(
            "README.md is not a log artifact".to_owned(),
        ));
    }
    if !relative.ends_with(".md") {
        return Err(DocsPolicyError::Message(format!(
            "log must be a .md file: {relative}"
        )));
    }
    let path_re =
        Regex::new(r"^[0-9]{4}-[0-9]{2}/[0-9]{2}-[0-9]{6}-.+\.md$").expect("logs path regex");
    if !path_re.is_match(&relative) {
        return Err(DocsPolicyError::Message(format!(
            "log path must match YYYY-MM/DD-HHMMSS-slug.md: {relative}"
        )));
    }

    Ok(relative)
}

pub fn insert_log_index_entry(index_contents: &str, entry: &str) -> String {
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

pub fn collect_workflow_check_files(
    dir: &Path,
    logs_dir: &Path,
    exclude_logs: bool,
) -> Vec<PathBuf> {
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

pub fn check_workflow_paths(
    repo_root: &Path,
    dir: &Path,
    logs_dir: &Path,
    exclude_logs: bool,
) -> Result<Vec<WorkflowPathFinding>, DocsPolicyError> {
    let workflow_re = Regex::new(r"\.github(-bak)?/workflows/[A-Za-z0-9._-]+\.ya?ml")
        .expect("workflow path regex");
    let mut findings = Vec::new();

    for path in collect_workflow_check_files(dir, logs_dir, exclude_logs) {
        let content = std::fs::read_to_string(&path).map_err(|error| DocsPolicyError::Io {
            path: path.clone(),
            error,
        })?;
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

                findings.push(WorkflowPathFinding {
                    file: path.clone(),
                    line: line_index + 1,
                    workflow_path: workflow_path.to_owned(),
                    reason,
                    suggestion,
                });
            }
        }
    }

    Ok(findings)
}

pub fn extract_h2_section(content: &str, section_title: &str) -> Option<String> {
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

pub fn extract_fenced_json_blocks(section: &str) -> Vec<String> {
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

pub fn first_non_empty_section_line(section: &str) -> Option<String> {
    section
        .lines()
        .skip(1)
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

pub fn extract_lead_verb(line: &str) -> String {
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

pub fn collect_markdown_children(dir: &Path, exclude: &[String]) -> BTreeSet<String> {
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

pub fn collect_index_markdown_links(
    index: &Path,
    section: Option<&str>,
) -> Result<BTreeSet<String>, DocsPolicyError> {
    let content = std::fs::read_to_string(index).map_err(|error| DocsPolicyError::Io {
        path: index.to_path_buf(),
        error,
    })?;
    let content = if let Some(section_name) = section {
        extract_h2_section(&content, section_name).ok_or_else(|| {
            DocsPolicyError::Message(format!(
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

pub fn path_matches_exclude(relative: &str, pattern: &str) -> bool {
    let normalized = pattern.trim_start_matches("./").replace('\\', "/");
    if let Some(prefix) = normalized.strip_suffix("/**") {
        relative == prefix || relative.starts_with(&format!("{prefix}/"))
    } else {
        relative == normalized || relative.starts_with(&format!("{normalized}/"))
    }
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

fn normalize_section_title(title: &str) -> &str {
    let trimmed = title.trim();
    let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
    let rest = rest
        .strip_prefix(") ")
        .or_else(|| rest.strip_prefix(". "))
        .unwrap_or(trimmed);
    rest.trim()
}

#[cfg(test)]
mod tests {
    use super::{
        check_contains, check_headings, check_paths, check_workflow_paths,
        collect_index_markdown_links, collect_link_check_files, collect_markdown_children,
        collect_workflow_check_files, extract_fenced_json_blocks, extract_h2_section,
        extract_lead_verb, first_non_empty_section_line, insert_log_index_entry,
        normalize_log_index_relative_path, path_matches_exclude, resolve_docs_index_spec,
        resolve_docs_next_action_spec, scan_markdown_links,
    };
    use effigy_manifest::config_sections::{
        ManifestDocsPolicyIndexConfig, ManifestDocsPolicyNextActionConfig,
    };
    use effigy_manifest::ManifestDocsPolicyConfig;
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
    fn check_headings_reports_missing_heading() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-headings-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("README.md"), "# Root\n").expect("write readme");

        let (_, findings) = check_headings(
            &root,
            &[Path::new("README.md").to_path_buf()],
            &[String::from("## Vision Alignment")],
        )
        .expect("check");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].heading, "## Vision Alignment");
    }

    #[test]
    fn check_contains_and_paths_report_missing_items() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-contains-paths-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("README.md"), "# Root\n").expect("write readme");

        let (_, contains_findings) = check_contains(
            &root,
            &[Path::new("README.md").to_path_buf()],
            &[String::from("Vision")],
        )
        .expect("contains");
        assert_eq!(contains_findings.len(), 1);

        let (_, path_findings) = check_paths(&root, &[Path::new("missing.md").to_path_buf()]);
        assert_eq!(path_findings.len(), 1);
    }

    #[test]
    fn check_workflow_paths_reports_stale_reference() {
        let root = std::env::temp_dir().join(format!(
            "effigy-doc-workflow-stale-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".github-bak/workflows")).expect("mkdir workflow");
        fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
        fs::write(
            root.join(".github-bak/workflows/example.yml"),
            "name: Example\n",
        )
        .expect("write workflow");
        fs::write(
            root.join("docs/guides/example.md"),
            "See `.github/workflows/example.yml`.\n",
        )
        .expect("write guide");

        let findings =
            check_workflow_paths(&root, &root.join("docs"), &root.join("docs/logs"), true)
                .expect("workflow check");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, "stale workflow path");
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

        let mut policy = ManifestDocsPolicyConfig::default();
        policy.indexes.insert(
            "vision".to_owned(),
            ManifestDocsPolicyIndexConfig {
                file: "docs/vision/README.md".to_owned(),
                dir: "docs/vision".to_owned(),
                section: Some("Vision Artifacts".to_owned()),
                exclude: vec!["history/**".to_owned()],
            },
        );

        let spec =
            resolve_docs_index_spec(&root, &policy, Some("vision"), None, None).expect("spec");
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

        let mut policy = ManifestDocsPolicyConfig::default();
        policy.indexes.insert(
            "vision".to_owned(),
            ManifestDocsPolicyIndexConfig {
                file: "docs/vision/README.md".to_owned(),
                dir: "docs/vision".to_owned(),
                section: Some("Vision Artifacts".to_owned()),
                exclude: Vec::new(),
            },
        );
        policy.next_actions.insert(
            "vision".to_owned(),
            ManifestDocsPolicyNextActionConfig {
                index: "vision".to_owned(),
                heading: "## Next Task".to_owned(),
                allowlist_file: "docs/scripts/fixtures/verbs.txt".to_owned(),
            },
        );

        let spec = resolve_docs_next_action_spec(&root, &policy, Some("vision")).expect("spec");
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
