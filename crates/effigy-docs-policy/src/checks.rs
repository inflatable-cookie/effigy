//! Higher-level docs check orchestration.
//!
//! Extracted from `src/runner/docs_command.rs` — these are docs-domain
//! validation operations, not CLI shell behavior.

use serde_json::json;

use crate::{
    collect_index_markdown_links, extract_h2_section, extract_lead_verb,
    first_non_empty_section_line, load_next_action_allowlist, DocsNextActionSpec, DocsPolicyError,
};

/// Result of a JSON examples validation check.
#[derive(Debug, Clone)]
pub struct JsonExamplesResult {
    /// Whether the check passed.
    pub ok: bool,
    /// Path to the checked file.
    pub file: String,
    /// Section title that was checked.
    pub section: String,
    /// Number of JSON blocks found.
    pub block_count: usize,
    /// Minimum blocks required.
    pub min_blocks: usize,
    /// Required needles.
    pub required: Vec<String>,
    /// Failure messages.
    pub failures: Vec<String>,
}

/// A single next-action finding.
#[derive(Debug, Clone)]
pub struct NextActionFinding {
    /// Path to the file.
    pub file: String,
    /// Relative path within the index.
    pub relative: String,
    /// Kind of finding.
    pub kind: String,
    /// Human-readable message.
    pub message: String,
    /// The problematic line (for non-actionable findings).
    pub line: Option<String>,
    /// The extracted verb (for non-actionable findings).
    pub verb: Option<String>,
    /// Path to the allowlist file.
    pub allowlist_file: Option<String>,
}

impl NextActionFinding {
    /// Convert to a JSON value.
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = json!({
            "file": self.file,
            "relative": self.relative,
            "kind": self.kind,
            "message": self.message,
        });
        if let Some(ref line) = self.line {
            obj["line"] = json!(line);
        }
        if let Some(ref verb) = self.verb {
            obj["verb"] = json!(verb);
        }
        if let Some(ref af) = self.allowlist_file {
            obj["allowlist_file"] = json!(af);
        }
        obj
    }
}

/// Validate JSON example blocks in a markdown section.
///
/// Checks that the section contains at least `min_blocks` fenced JSON
/// blocks, and that each block contains all `required` needles.
pub fn validate_json_examples(
    content: &str,
    file_display: &str,
    section_title: &str,
    min_blocks: usize,
    required: &[String],
    required_blocks: &[(usize, String)],
) -> JsonExamplesResult {
    let section = match extract_h2_section(content, section_title) {
        Some(s) => s,
        None => {
            return JsonExamplesResult {
                ok: false,
                file: file_display.to_string(),
                section: section_title.to_string(),
                block_count: 0,
                min_blocks,
                required: required.to_vec(),
                failures: vec![format!(
                    "section `{section_title}` not found in {file_display}"
                )],
            };
        }
    };

    let blocks = crate::extract_fenced_json_blocks(&section);
    let mut failures = Vec::new();

    if blocks.len() < min_blocks {
        failures.push(format!(
            "expected at least {min_blocks} JSON example block(s), found {}",
            blocks.len()
        ));
    }

    for needle in required {
        for (index, block) in blocks.iter().enumerate() {
            if !block.contains(needle.as_str()) {
                failures.push(format!(
                    "missing `{needle}` in JSON example block #{}",
                    index + 1
                ));
            }
        }
    }

    for (block_index, needle) in required_blocks {
        let block = blocks.get(block_index.saturating_sub(1));
        match block {
            Some(block) if block.contains(needle.as_str()) => {}
            Some(_) => failures.push(format!(
                "missing `{needle}` in JSON example block #{block_index}"
            )),
            None => failures.push(format!(
                "required JSON example block #{block_index} is missing"
            )),
        }
    }

    JsonExamplesResult {
        ok: failures.is_empty(),
        file: file_display.to_string(),
        section: section_title.to_string(),
        block_count: blocks.len(),
        min_blocks,
        required: required.to_vec(),
        failures,
    }
}

/// Run the next-action check across all indexed files.
///
/// Walks each file referenced in the index, checks for the required
/// heading, validates the lead verb against the allowlist.
pub fn check_next_action(
    spec: &DocsNextActionSpec,
) -> Result<Vec<NextActionFinding>, DocsPolicyError> {
    let referenced =
        collect_index_markdown_links(&spec.index.index, spec.index.section.as_deref())?;
    let allowlist = load_next_action_allowlist(&spec.allowlist_file)?;
    let mut findings = Vec::new();

    for relative in referenced {
        let file = spec.index.dir.join(&relative);
        let file_display = file.display().to_string();

        if !file.is_file() {
            findings.push(NextActionFinding {
                file: file_display,
                relative: relative.clone(),
                kind: "missing-file".to_string(),
                message: format!("missing indexed markdown file: {}", file.display()),
                line: None,
                verb: None,
                allowlist_file: None,
            });
            continue;
        }

        let content = std::fs::read_to_string(&file).map_err(|error| DocsPolicyError::Io {
            path: file.clone(),
            error,
        })?;

        let Some(section) = extract_h2_section(&content, &spec.heading_without_hashes) else {
            findings.push(NextActionFinding {
                file: file_display,
                relative: relative.clone(),
                kind: "missing-heading".to_string(),
                message: format!("missing `{}` section in {}", spec.heading, file.display()),
                line: None,
                verb: None,
                allowlist_file: None,
            });
            continue;
        };

        let Some(first_line) = first_non_empty_section_line(&section) else {
            findings.push(NextActionFinding {
                file: file_display,
                relative: relative.clone(),
                kind: "empty-section".to_string(),
                message: format!("empty `{}` section in {}", spec.heading, file.display()),
                line: None,
                verb: None,
                allowlist_file: None,
            });
            continue;
        };

        let verb = extract_lead_verb(&first_line);
        if verb.is_empty() || !allowlist.contains(&verb) {
            findings.push(NextActionFinding {
                file: file_display,
                relative: relative.clone(),
                kind: "non-actionable".to_string(),
                message: format!(
                    "non-actionable `{}` lead verb in {}: `{}`",
                    spec.heading,
                    file.display(),
                    first_line
                ),
                line: Some(first_line),
                verb: Some(verb),
                allowlist_file: Some(spec.allowlist_file.display().to_string()),
            });
        }
    }

    Ok(findings)
}

/// Default required JSON fields for the completion-candidates example check.
pub fn default_json_example_requirements() -> Vec<String> {
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

/// Default required block-specific checks for the completion-candidates example.
///
/// Returns `(block_index, needle)` pairs.
pub fn default_json_example_block_requirements() -> Vec<(usize, String)> {
    vec![
        (1, "\"cache_state\": \"hit\"".to_owned()),
        (2, "\"cache_hit\": false".to_owned()),
    ]
}

#[cfg(test)]
#[path = "checks/tests.rs"]
mod tests;
