use std::path::Path;

use serde_json::json;

use crate::TaskInvocation;

use super::super::locking::{unlock_all, unlock_scopes, LockScope};
use super::super::RunnerError;

pub(super) fn run_builtin_unlock(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let mut output_json = false;
    let mut unlock_all_flag = false;
    let mut scopes = Vec::<LockScope>::new();

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--all" => unlock_all_flag = true,
            "--help" | "-h" => {
                return Ok(Some(render_unlock_help()));
            }
            value => {
                let Some(scope) = LockScope::parse(value) else {
                    return Err(RunnerError::TaskInvocation(format!(
                        "`{}` unlock target `{value}` is invalid; expected `workspace`, `task:<name>`, or `profile:<task>/<profile>`",
                        task.name
                    )));
                };
                scopes.push(scope);
            }
        }
    }

    if unlock_all_flag && !scopes.is_empty() {
        return Err(RunnerError::TaskInvocation(
            "`unlock` accepts either `--all` or explicit scope values, not both".to_owned(),
        ));
    }
    if !unlock_all_flag && scopes.is_empty() {
        return Err(RunnerError::TaskInvocation(
            "`unlock` requires at least one scope (or `--all`)".to_owned(),
        ));
    }

    let result = if unlock_all_flag {
        unlock_all(target_root)?
    } else {
        unlock_scopes(target_root, &scopes)?
    };

    if output_json {
        let payload = json!({
            "schema": "effigy.unlock.v1",
            "schema_version": 1,
            "ok": true,
            "root": target_root.display().to_string(),
            "removed": result.removed,
            "missing": result.missing,
            "all": unlock_all_flag,
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    let mut lines = vec![format!("unlock root: {}", target_root.display())];
    if unlock_all_flag {
        lines.push("mode: all".to_owned());
    } else {
        lines.push("mode: scopes".to_owned());
    }
    lines.push(format!("removed: {}", result.removed.len()));
    for entry in result.removed {
        lines.push(format!("- {entry}"));
    }
    if !result.missing.is_empty() {
        lines.push(format!("missing: {}", result.missing.len()));
        for entry in result.missing {
            lines.push(format!("- {entry}"));
        }
    }
    Ok(Some(lines.join("\n")))
}

fn render_unlock_help() -> String {
    [
        "unlock Help",
        "",
        "Usage",
        "effigy unlock [--all | <scope>...] [--json]",
        "",
        "Scopes",
        "- workspace",
        "- task:<name>",
        "- profile:<task>/<profile>",
        "",
        "Examples",
        "- effigy unlock workspace",
        "- effigy unlock task:dev profile:dev/admin",
        "- effigy unlock --all",
        "- effigy unlock --all --json",
    ]
    .join("\n")
}
