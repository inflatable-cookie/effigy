use effigy_cli::{ReleaseArgs, ReleaseSubcommand};
use effigy_release::{
    load_release_config, remediation_hints_for_blockers,
    render_release_execute_plan_json as render_release_execute_plan_json_payload,
    render_release_execute_plan_text,
    render_release_executed_json as render_release_executed_json_payload,
    render_release_executed_text,
    render_release_gate_run_json as render_release_gate_run_json_payload,
    render_release_gate_run_text,
    render_release_prepare_plan_json as render_release_prepare_plan_json_payload,
    render_release_prepare_plan_text,
    render_release_prepared_json as render_release_prepared_json_payload,
    render_release_prepared_text, render_release_resume_json as render_release_resume_json_payload,
    render_release_resume_text,
    render_release_simulation_json as render_release_simulation_json_payload,
    render_release_simulation_text,
    render_release_status_json as render_release_status_json_payload, render_release_status_text,
    render_release_verify_install_json as render_release_verify_install_json_payload,
    render_release_verify_install_text, ReleaseBlockedStage,
};

use super::command_context::{current_working_dir, resolve_repo_root};
use super::RunnerError;
#[cfg(test)]
use interactive::parse_prepare_mutation_inspection_request;
use interactive::{
    run_interactive_release_execute, run_interactive_release_prepare,
    run_interactive_release_resume,
};
use ops::{
    collect_release_execute_plan, collect_release_prepare_plan, collect_release_simulation,
    collect_release_status, execute_release, execute_release_prepare,
    parse_release_version_override, run_release_verify_install, run_standalone_release_gates,
};
#[cfg(test)]
use ops::{resolve_verify_install_repo_url, validate_prepare_version_override};

mod interactive;
mod ops;

const RELEASE_PREPARED_STATE_FILE: &str = ".release-prepared.json";
const RELEASE_STATE_STALE_THRESHOLD_SECS: i64 = 60 * 60;

pub(super) fn run_release(args: ReleaseArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override)?;

    match args.subcommand {
        ReleaseSubcommand::Status { check_gates } => {
            let status = collect_release_status(&resolved, check_gates)?;
            if args.output_json {
                let rendered = render_release_status_json_payload(&status);
                if status.ready {
                    return Ok(rendered);
                }
                return Err(RunnerError::CommandJsonFailure { rendered });
            }

            let rendered = render_release_status_text(&status);
            if status.ready {
                Ok(rendered)
            } else {
                Err(RunnerError::task_invocation(rendered))
            }
        }
        ReleaseSubcommand::Gates => {
            let gate_run = run_standalone_release_gates(&resolved)?;
            if args.output_json {
                let rendered = render_release_gate_run_json_payload(&gate_run);
                if gate_run.passed {
                    return Ok(rendered);
                }
                return Err(RunnerError::CommandJsonFailure { rendered });
            }

            let rendered = render_release_gate_run_text(&gate_run);
            if gate_run.passed {
                Ok(rendered)
            } else {
                Err(RunnerError::task_invocation(rendered))
            }
        }
        ReleaseSubcommand::Resume { allow_stale } => {
            let resume_plan = collect_release_execute_plan(&resolved, allow_stale)?;
            if args.output_json {
                let rendered = render_release_resume_json_payload(
                    &resume_plan,
                    &remediation_hints_for_blockers(
                        &resume_plan.blockers,
                        ReleaseBlockedStage::Execute,
                    ),
                );
                if resume_plan.state_loaded {
                    return Ok(rendered);
                }
                return Err(RunnerError::CommandJsonFailure { rendered });
            }

            if !resume_plan.state_loaded {
                return Err(RunnerError::task_invocation(render_release_resume_text(
                    &resume_plan,
                )));
            }
            run_interactive_release_resume(&resolved, allow_stale)
        }
        ReleaseSubcommand::VerifyInstall { tag, repo_url } => {
            let verification = run_release_verify_install(&resolved, tag, repo_url)?;
            if args.output_json {
                let rendered = render_release_verify_install_json_payload(&verification);
                if verification.verified {
                    return Ok(rendered);
                }
                return Err(RunnerError::CommandJsonFailure { rendered });
            }

            let rendered = render_release_verify_install_text(&verification);
            if verification.verified {
                Ok(rendered)
            } else {
                Err(RunnerError::task_invocation(rendered))
            }
        }
        ReleaseSubcommand::Simulate { version_override } => {
            let requested_version_override = parse_release_version_override(
                &resolved.resolved_root,
                version_override.as_deref(),
                "simulate",
            )?;
            let simulation = collect_release_simulation(&resolved, requested_version_override)?;
            if args.output_json {
                let rendered = render_release_simulation_json_payload(&simulation);
                if simulation.ready {
                    return Ok(rendered);
                }
                return Err(RunnerError::CommandJsonFailure { rendered });
            }

            let rendered = render_release_simulation_text(&simulation);
            if simulation.ready {
                Ok(rendered)
            } else {
                Err(RunnerError::task_invocation(rendered))
            }
        }
        ReleaseSubcommand::Prepare {
            plan,
            check_gates,
            yes,
            version_override,
        } => {
            if plan && yes {
                return Err(RunnerError::task_invocation(
                    "`release prepare` cannot combine `--plan`/`--dry-run` and `--yes`".to_owned(),
                ));
            }
            let requested_version_override = parse_release_version_override(
                &resolved.resolved_root,
                version_override.as_deref(),
                "prepare",
            )?;
            if plan {
                let prepare_plan = collect_release_prepare_plan(
                    &resolved,
                    check_gates,
                    requested_version_override,
                )?;
                if args.output_json {
                    let rendered = render_release_prepare_plan_json_payload(&prepare_plan);
                    if prepare_plan.ready {
                        return Ok(rendered);
                    }
                    return Err(RunnerError::CommandJsonFailure { rendered });
                }

                let rendered = render_release_prepare_plan_text(&prepare_plan);
                if prepare_plan.ready {
                    Ok(rendered)
                } else {
                    Err(RunnerError::task_invocation(rendered))
                }
            } else if yes {
                let prepared =
                    execute_release_prepare(&resolved, check_gates, requested_version_override)?;
                if args.output_json {
                    let rendered = render_release_prepared_json_payload(&prepared);
                    if prepared.prepared {
                        return Ok(rendered);
                    }
                    return Err(RunnerError::CommandJsonFailure { rendered });
                }

                let rendered = render_release_prepared_text(&prepared);
                if prepared.prepared {
                    Ok(rendered)
                } else {
                    Err(RunnerError::task_invocation(rendered))
                }
            } else if args.output_json {
                Err(RunnerError::task_invocation(
                    "interactive release preparation is only available in text mode; use `effigy release prepare --plan` or `effigy release prepare --yes` when `--json` is enabled"
                        .to_owned(),
                ))
            } else {
                if version_override.is_some() {
                    return Err(RunnerError::task_invocation(
                        "`release prepare --version` is only supported with `--plan` or `--yes`; plain interactive `release prepare` already supports custom version review".to_owned(),
                    ));
                }
                run_interactive_release_prepare(&resolved, check_gates)
            }
        }
        ReleaseSubcommand::Execute {
            plan,
            yes,
            allow_stale,
        } => {
            if plan && yes {
                return Err(RunnerError::task_invocation(
                    "`release execute` cannot combine `--plan`/`--dry-run` and `--yes`".to_owned(),
                ));
            }
            if plan {
                let execute_plan = collect_release_execute_plan(&resolved, allow_stale)?;
                if args.output_json {
                    let rendered = render_release_execute_plan_json_payload(&execute_plan);
                    if execute_plan.ready {
                        return Ok(rendered);
                    }
                    return Err(RunnerError::CommandJsonFailure { rendered });
                }

                let rendered = render_release_execute_plan_text(&execute_plan);
                if execute_plan.ready {
                    Ok(rendered)
                } else {
                    Err(RunnerError::task_invocation(rendered))
                }
            } else if yes {
                let executed = execute_release(&resolved, allow_stale)?;
                if args.output_json {
                    let rendered = render_release_executed_json_payload(&executed);
                    if executed.executed {
                        return Ok(rendered);
                    }
                    return Err(RunnerError::CommandJsonFailure { rendered });
                }

                let rendered = render_release_executed_text(&executed);
                if executed.executed {
                    Ok(rendered)
                } else {
                    Err(RunnerError::task_invocation(rendered))
                }
            } else if args.output_json {
                Err(RunnerError::task_invocation(
                    "interactive release execution is only available in text mode; use `effigy release execute --plan` or `effigy release execute --yes` when `--json` is enabled"
                        .to_owned(),
                ))
            } else {
                run_interactive_release_execute(&resolved, allow_stale)
            }
        }
    }
}

#[cfg(test)]
mod tests;
