use std::path::Path;

use effigy_cli::{ReleaseArgs, ReleaseSubcommand};
use effigy_release::{
    load_release_config, release_gate_run_payload, release_plan_payload, release_run_payload,
    release_status_payload, render_release_abort_json, render_release_abort_text,
    render_release_gate_run_text, render_release_pipeline_plan_text, render_release_receipt_json,
    render_release_receipt_text, render_release_status_text, ReleaseAction, ReleaseRunOutcome,
    ReleaseStage, TargetDecision,
};

use super::command_context::resolve_active_repo_root;
use super::RunnerError;
use ops::{
    collect_release_plan, collect_release_status, release_receipt, release_run_action,
    run_standalone_release_gates,
};

mod ops;

pub(super) fn run_release(args: ReleaseArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override)?;

    match args.subcommand {
        ReleaseSubcommand::Status { check_gates } => {
            let status = collect_release_status(&resolved, check_gates)?;
            // With `--check-gates`, the exit status reports the gate check
            // verdict independently of the optional next-version proposal: an
            // empty `[Unreleased]` section keeps `ready` false and stays
            // listed under blockers, but does not fail passed gates.
            let succeeded = if check_gates {
                status.gate_check_passed()
            } else {
                status.ready
            };
            if args.output_json {
                let payload = release_status_payload(&status);
                if succeeded {
                    return Ok(serde_json::to_string_pretty(&payload).expect("JSON value encodes"));
                }
                return Err(release_failure(
                    payload,
                    "release status is blocked",
                    &status.blockers,
                    &status.gate_results,
                ));
            }

            let rendered = render_release_status_text(&status);
            if succeeded {
                Ok(rendered)
            } else {
                Err(RunnerError::task_invocation(rendered))
            }
        }
        ReleaseSubcommand::Gates => {
            let gate_run = run_standalone_release_gates(&resolved)?;
            if args.output_json {
                let payload = release_gate_run_payload(&gate_run);
                if gate_run.passed {
                    return Ok(serde_json::to_string_pretty(&payload).expect("JSON value encodes"));
                }
                return Err(release_failure(
                    payload,
                    "release gates failed",
                    &gate_run.blockers,
                    &gate_run.gate_results,
                ));
            }

            let rendered = render_release_gate_run_text(&gate_run);
            if gate_run.passed {
                Ok(rendered)
            } else {
                Err(RunnerError::task_invocation(rendered))
            }
        }
        ReleaseSubcommand::Plan { version_override } => {
            let requested_version_override = parse_release_plan_version_override(
                &resolved.resolved_root,
                version_override.as_deref(),
            )?;
            let plan = collect_release_plan(&resolved, requested_version_override)?;
            if args.output_json {
                let payload = release_plan_payload(&plan);
                if plan.ready {
                    return Ok(serde_json::to_string_pretty(&payload).expect("JSON value encodes"));
                }
                return Err(release_failure(
                    payload,
                    "release plan is blocked",
                    &plan.blockers,
                    &[],
                ));
            }

            let rendered = render_release_pipeline_plan_text(&plan);
            if plan.ready {
                Ok(rendered)
            } else {
                Err(RunnerError::task_invocation(rendered))
            }
        }
        ReleaseSubcommand::Run { to } => {
            let stage = match to.as_deref() {
                None => None,
                Some(value) => Some(ReleaseStage::parse(value).ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "`release run --to` names `{value}`, which is not a release stage boundary; the boundaries are validate, pre-gates, prepare, certify, publish, verify and closeout (contract 054 §3)"
                    ))
                })?),
            };
            render_release_run(
                args.output_json,
                release_run_action(&resolved, ReleaseAction::Run, stage)?,
            )
        }
        ReleaseSubcommand::ResumeRun { run } => render_release_run(
            args.output_json,
            release_run_action(&resolved, ReleaseAction::Resume { run }, None)?,
        ),
        ReleaseSubcommand::Settle { run } => {
            let home = effigy_release::release_home()?;
            let record = effigy_release::settle_release(&home, &resolved.resolved_root, &run)?;
            let receipt = effigy_release::ReleaseReceipt::from_record(&record);
            if args.output_json {
                Ok(render_release_receipt_json(&receipt))
            } else {
                Ok(render_release_receipt_text(&receipt))
            }
        }
        ReleaseSubcommand::Approve { run } => render_release_run(
            args.output_json,
            release_run_action(&resolved, ReleaseAction::Approve { run }, None)?,
        ),
        ReleaseSubcommand::Decide { run, published } => render_release_run(
            args.output_json,
            release_run_action(
                &resolved,
                ReleaseAction::Decide {
                    run,
                    decision: if published {
                        TargetDecision::Published
                    } else {
                        TargetDecision::NotPublished
                    },
                },
                None,
            )?,
        ),
        ReleaseSubcommand::Receipt { run } => {
            let receipt = release_receipt(&resolved, run.as_deref())?;
            if args.output_json {
                return Ok(render_release_receipt_json(&receipt));
            }
            Ok(render_release_receipt_text(&receipt))
        }
        ReleaseSubcommand::Abort { run, yes } => {
            let home = effigy_release::release_home()?;
            let report =
                effigy_release::abort_release(&home, &resolved.resolved_root, run.as_deref(), yes)?;
            if args.output_json {
                Ok(render_release_abort_json(&report))
            } else {
                Ok(render_release_abort_text(&report))
            }
        }
    }
}

fn release_failure(
    payload: serde_json::Value,
    message: &str,
    blockers: &[String],
    gates: &[effigy_release::GateResult],
) -> RunnerError {
    let human = blockers
        .iter()
        .fold(effigy_ui::HumanError::new(message), |human, blocker| {
            human.evidence(blocker)
        });
    let human = gates
        .iter()
        .filter(|gate| !gate.passed)
        .fold(human, |human, gate| {
            let human = human.evidence(format!("gate `{}` failed", gate.name));
            let human = if let Some(error) = &gate.launch_error {
                human.evidence(error)
            } else {
                human
            };
            human
                .evidence_output("stderr", &gate.stderr)
                .evidence_output("stdout", &gate.stdout)
        });
    RunnerError::structured_json(payload, true, human, None)
}

/// A release Run reports its receipt plus the lines the Run printed.
fn render_release_run(
    output_json: bool,
    outcome: ReleaseRunOutcome,
) -> Result<String, RunnerError> {
    let receipt = outcome.receipt();
    let succeeded = outcome.succeeded();
    if output_json {
        let payload = release_run_payload(
            &receipt,
            &outcome.messages,
            &outcome.receipt_path.display().to_string(),
        );
        if succeeded {
            return Ok(serde_json::to_string_pretty(&payload).expect("JSON value encodes"));
        }
        return Err(release_failure(
            payload,
            "release run failed",
            &receipt.errors,
            &[],
        ));
    }
    let mut lines = Vec::new();
    lines.extend(outcome.messages.iter().cloned());
    lines.push(render_release_receipt_text(&receipt));
    let rendered = lines.join("\n");
    if succeeded {
        Ok(rendered)
    } else {
        Err(RunnerError::task_invocation(rendered))
    }
}

/// Parse `release --plan --version` before any resolution, so an unparseable
/// version names the flag rather than a version file.
fn parse_release_plan_version_override(
    repo_root: &Path,
    raw_version: Option<&str>,
) -> Result<Option<semver::Version>, RunnerError> {
    let _ = repo_root;
    let Some(raw_version) = raw_version else {
        return Ok(None);
    };
    semver::Version::parse(raw_version.trim())
        .map(Some)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "invalid `release --plan --version`: `{}` is not valid semver: {error}",
                raw_version.trim()
            ))
        })
}

#[cfg(test)]
mod tests;
