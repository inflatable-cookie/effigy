use std::path::Path;

use serde_json::json;

use super::super::response::{
    render_optional_text_or_schema_json, render_optional_text_with_schema_text_fields_lazy,
};
use super::candidates::CompletionCandidatesResult;
use super::install::CompletionInstallResult;
use super::request::CompletionAction;
use super::scripts::{command_names, CompletionShell};
use crate::BuiltinError;

pub(super) fn render_completion_export_response(
    output_json: bool,
    shell: CompletionShell,
    prompted_shell: bool,
    prompted_action: bool,
    script: String,
) -> Result<Option<String>, BuiltinError> {
    let payload_script = script.clone();
    render_optional_text_or_schema_json(
        output_json,
        script,
        "effigy.completion.v2",
        json!({
            "shell": shell.as_str(),
            "action": CompletionAction::Export.as_str(),
            "script": payload_script,
            "commands": command_names(),
            "prompted_shell": prompted_shell,
            "prompted_action": prompted_action,
        }),
    )
}

pub(super) fn render_completion_install_response(
    output_json: bool,
    prompted_shell: bool,
    prompted_action: bool,
    result: &CompletionInstallResult,
) -> Result<Option<String>, BuiltinError> {
    let install_path = result.install_path.display().to_string();
    let startup_path = result
        .startup_path
        .as_ref()
        .map(|path| path.display().to_string());
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.completion.v2",
        || render_install_summary(result),
        || {
            json!({
                "shell": result.shell.as_str(),
                "action": CompletionAction::Install.as_str(),
                "script": result.script,
                "install_path": install_path,
                "startup_path": startup_path,
                "startup_changed": result.startup_changed,
                "startup_managed": result.startup_managed,
                "install_changed": result.install_changed,
                "follow_up_required": result.follow_up_required,
                "follow_up_message": result.follow_up_message,
                "commands": command_names(),
                "prompted_shell": prompted_shell,
                "prompted_action": prompted_action,
            })
        },
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

fn render_install_summary(result: &CompletionInstallResult) -> String {
    let mut lines = vec![
        format!("shell: {}", result.shell.as_str()),
        format!("install_path: {}", result.install_path.display()),
        format!(
            "install_changed: {}",
            if result.install_changed { "yes" } else { "no" }
        ),
    ];
    if let Some(startup_path) = result.startup_path.as_ref() {
        lines.push(format!("startup_path: {}", startup_path.display()));
        lines.push(format!(
            "startup_changed: {}",
            if result.startup_changed { "yes" } else { "no" }
        ));
    } else {
        lines.push("startup_path: none".to_owned());
    }
    if let Some(message) = result.follow_up_message.as_ref() {
        lines.push(format!("next: {message}"));
    }
    format!("{}\n", lines.join("\n"))
}
