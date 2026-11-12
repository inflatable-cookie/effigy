use serde_json::json;

use super::report::APPLY_SCHEMA;
use crate::runner::error::RunnerError;

// Refuse before manifest/provider resolution: even preflight can acquire packages
// or execute scripts. No deployment effect or success record is permitted here.
pub(super) fn refuse_deploy_execution(
    operation: &str,
    env: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let message = format!(
        "`deploy {operation}` is not yet supported in v0.15; no deployment was executed. Full execution design is tracked in the deploy lane (Q-003)."
    );
    // The one next step, shared by the text `next:` line and the JSON
    // `next_action`. The first command is runnable as written in this checkout.
    let next_action = "run `effigy deploy model --json` to read the deployment model, or `effigy deploy export <PROVIDER> --path <DIR> --plan` to export a provider plan";
    let obligations = [
        "state: state stacks and migrations are not executed",
        "hooks: before_state, after_state and after_deploy hooks are not executed",
        "health: service health checks are not executed",
        "release gates: required release evidence is not verified",
        "digest pins: immutable artifact and code identities are not enforced",
        "provider: deployment and rollback are not executed",
    ];
    if output_json {
        Err(RunnerError::CommandJsonFailure {
            rendered: json!({
                "schema": APPLY_SCHEMA,
                "schema_version": 1,
                "ok": false,
                "status": "blocked",
                "reason": "not_supported",
                "operation": operation,
                "env": env,
                "message": message,
                "blockers": obligations,
                "next_action": next_action,
            })
            .to_string(),
        })
    } else {
        Err(RunnerError::task_invocation(format!(
            "{message}\n{}\nnext: {next_action}",
            obligations.join("\n")
        )))
    }
}
