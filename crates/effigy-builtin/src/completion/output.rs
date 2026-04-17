use std::path::Path;

use serde_json::json;

use super::super::response::{
    render_optional_text_or_schema_json, render_optional_text_with_schema_text_fields_lazy,
};
use super::candidates::CompletionCandidatesResult;
use super::scripts::{command_names, CompletionShell};
use crate::BuiltinError;

pub(super) fn render_completion_script_response(
    output_json: bool,
    shell: CompletionShell,
    script: String,
) -> Result<Option<String>, BuiltinError> {
    let payload_script = script.clone();
    render_optional_text_or_schema_json(
        output_json,
        script,
        "effigy.completion.v1",
        json!({
            "shell": shell.as_str(),
            "script": payload_script,
            "commands": command_names(),
        }),
    )
}

pub(super) fn render_completion_candidates_response(
    output_json: bool,
    repo_root: &Path,
    prefix: Option<&str>,
    result: &CompletionCandidatesResult,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.completion.candidates.v1",
        || result.candidates.join("\n"),
        || {
            json!({
                "repo": repo_root.display().to_string(),
                "prefix": prefix,
                "candidates": &result.candidates,
                "cache_hit": result.cache_hit,
                "cache_state": result.cache_state,
                "manifest_count": result.manifest_count,
                "cache_age_ms": result.cache_age_ms,
                "cache_ttl_ms": result.cache_ttl_ms,
                "effective_cache_ttl_ms": result.effective_cache_ttl_ms,
                "cache_ttl_source": result.cache_ttl_source,
            })
        },
    )
}
