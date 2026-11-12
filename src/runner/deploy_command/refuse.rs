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

#[cfg(test)]
mod tests {
    use super::refuse_deploy_execution;

    #[test]
    fn the_json_refusal_names_every_obligation_and_one_next_action() {
        let error = refuse_deploy_execution("apply", "production", true)
            .expect_err("deploy execution is refused");
        let rendered = error.rendered_output().expect("a JSON refusal body");
        let body: serde_json::Value = serde_json::from_str(rendered).expect("parse refusal");
        assert_eq!(body["schema"], "effigy.deploy.apply.v1");
        assert_eq!(body["ok"], false);
        assert_eq!(body["reason"], "not_supported");
        assert_eq!(body["operation"], "apply");
        assert_eq!(body["env"], "production");
        let blockers = body["blockers"].as_array().expect("obligation list");
        assert_eq!(blockers.len(), 6);
        assert!(body["next_action"]
            .as_str()
            .expect("next action")
            .contains("effigy deploy model --json"));
    }

    #[test]
    fn the_text_refusal_lists_the_obligations_and_a_runnable_next_step() {
        let error = refuse_deploy_execution("redeploy", "uat", false)
            .expect_err("deploy execution is refused");
        let message = error.to_string();
        assert!(message.contains("`deploy redeploy` is not yet supported in v0.15"));
        assert!(message.contains("hooks: before_state, after_state"));
        assert!(message.contains("\nnext: run `effigy deploy model --json`"));
    }
}
