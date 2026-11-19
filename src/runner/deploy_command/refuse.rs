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
        "hooks: after_deploy hooks are not executed",
        "health: service health checks are not executed",
        "release gates: required release evidence is not verified",
        "digest pins: immutable artifact and code identities are not enforced",
        "provider: deployment and rollback are not executed",
    ];
    if output_json {
        let human = obligations
            .iter()
            .fold(effigy_ui::HumanError::new(&message), |human, item| {
                human.evidence(*item)
            })
            .next_action(next_action);
        Err(RunnerError::structured_json(
            json!({
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
            }),
            false,
            human,
            None,
        ))
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
    fn refusal_keeps_obligations_and_human_facts_in_text_and_json() {
        let json = refuse_deploy_execution("apply", "production", true).expect_err("refused");
        let text = refuse_deploy_execution("apply", "production", false).expect_err("refused");
        let body: serde_json::Value =
            serde_json::from_str(json.rendered_output().unwrap()).unwrap();
        assert_eq!(body["schema"], "effigy.deploy.apply.v1");
        assert_eq!(body["ok"], false);
        assert_eq!(body["reason"], "not_supported");
        assert_eq!(body["operation"], "apply");
        assert_eq!(body["env"], "production");
        assert_eq!(body["blockers"].as_array().unwrap().len(), 5);
        assert_eq!(json.human_error().to_json(), text.human_error().to_json());
        assert!(text
            .to_string()
            .contains("hooks: after_deploy hooks are not executed"));
        assert!(text
            .to_string()
            .contains("next: run `effigy deploy model --json`"));
    }
}
