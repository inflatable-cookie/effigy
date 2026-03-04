use std::path::Path;

use serde_json::json;

use crate::TaskInvocation;

use super::super::locking::{unlock_all, unlock_scopes};
use super::super::RunnerError;
use super::command_spec::run_builtin_command;
use super::help_text::{render_titled_help, HelpSection};
use super::render_builtin_help_text;
use super::response::render_optional_text_or_schema_json_lazy;
use super::text_doc::TextDoc;

#[path = "unlock/request.rs"]
mod request;
use request::{parse_unlock_request, UnlockRequest};

pub(super) fn run_builtin_unlock(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_text("unlock", render_unlock_help(), output_json),
        || parse_unlock_request(task, args),
        |request: UnlockRequest| run_unlock_request(request, target_root),
    )
}

fn run_unlock_request(
    request: UnlockRequest,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let result = if request.unlock_all_flag {
        unlock_all(target_root)?
    } else {
        unlock_scopes(target_root, &request.scopes)?
    };
    let removed = result.removed;
    let missing = result.missing;

    render_optional_text_or_schema_json_lazy(
        request.output_json,
        "effigy.unlock.v1",
        || render_unlock_text(target_root, request.unlock_all_flag, &removed, &missing),
        || {
            json!({
                "root": target_root.display().to_string(),
                "removed": &removed,
                "missing": &missing,
                "all": request.unlock_all_flag,
            })
        },
    )
}

#[cfg(test)]
pub(in crate::runner) use request::parse_unlock_contract_request;

fn render_unlock_help() -> String {
    render_titled_help(
        "unlock",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &["effigy unlock [--all | <scope>...] [--json]"],
            },
            HelpSection::Bulleted {
                heading: "Scopes",
                items: &["workspace", "task:<name>", "profile:<task>/<profile>"],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy unlock workspace",
                    "effigy unlock task:dev profile:dev/admin",
                    "effigy unlock --all",
                    "effigy unlock --all --json",
                ],
            },
        ],
    )
}

fn render_unlock_text(
    target_root: &Path,
    unlock_all_flag: bool,
    removed: &[String],
    missing: &[String],
) -> String {
    let mut doc = TextDoc::new();
    doc.kv("unlock root", target_root.display());
    if unlock_all_flag {
        doc.kv("mode", "all");
    } else {
        doc.kv("mode", "scopes");
    }
    doc.kv("removed", removed.len());
    for entry in removed {
        doc.bullet(entry);
    }
    if !missing.is_empty() {
        doc.kv("missing", missing.len());
        for entry in missing {
            doc.bullet(entry);
        }
    }
    doc.finish()
}
