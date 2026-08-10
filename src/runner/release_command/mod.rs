use std::path::{Path, PathBuf};

use effigy_cli::{ReleaseArgs, ReleaseEvidenceSubcommand, ReleaseSubcommand};
use effigy_distribution::{effective_brew_formula, effective_repo_url, load_distribution_policy};
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

use super::command_context::resolve_active_repo_root;
use super::distribution_command::ops as distribution_ops;
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

struct ReleaseStageRendered {
    ok: bool,
    json: String,
    text: String,
}

fn render_release_rendered_result(
    output_json: bool,
    rendered: ReleaseStageRendered,
) -> Result<String, RunnerError> {
    if output_json {
        if rendered.ok {
            Ok(rendered.json)
        } else {
            Err(RunnerError::CommandJsonFailure {
                rendered: rendered.json,
            })
        }
    } else if rendered.ok {
        Ok(rendered.text)
    } else {
        Err(RunnerError::task_invocation(rendered.text))
    }
}

fn reject_conflicting_release_stage_flags(
    command_name: &str,
    plan: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if plan && yes {
        return Err(RunnerError::task_invocation(format!(
            "`release {command_name}` cannot combine `--plan`/`--dry-run` and `--yes`"
        )));
    }
    Ok(())
}

struct ReleaseStageRequest<'a> {
    command_name: &'a str,
    plan: bool,
    yes: bool,
    output_json: bool,
    interactive_json_error: &'a str,
}

fn run_release_stage<PlanFn, YesFn, InteractiveFn>(
    request: ReleaseStageRequest<'_>,
    render_plan: PlanFn,
    render_yes: YesFn,
    interactive: InteractiveFn,
) -> Result<String, RunnerError>
where
    PlanFn: FnOnce() -> Result<ReleaseStageRendered, RunnerError>,
    YesFn: FnOnce() -> Result<ReleaseStageRendered, RunnerError>,
    InteractiveFn: FnOnce() -> Result<String, RunnerError>,
{
    let ReleaseStageRequest {
        command_name,
        plan,
        yes,
        output_json,
        interactive_json_error,
    } = request;
    reject_conflicting_release_stage_flags(command_name, plan, yes)?;
    if plan {
        return render_plan()
            .and_then(|rendered| render_release_rendered_result(output_json, rendered));
    }
    if yes {
        return render_yes()
            .and_then(|rendered| render_release_rendered_result(output_json, rendered));
    }
    if output_json {
        return Err(RunnerError::task_invocation(
            interactive_json_error.to_owned(),
        ));
    }
    interactive()
}

pub(super) fn run_release(args: ReleaseArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override)?;

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
        ReleaseSubcommand::Validate { tag } => {
            let distribution_policy = load_distribution_policy(&resolved.resolved_root)?;
            distribution_ops::run_validate_metadata(
                &resolved.resolved_root,
                &distribution_policy,
                tag.as_deref(),
                args.output_json,
            )
        }
        ReleaseSubcommand::CheckBinary {
            binary_path,
            glibc_floor,
        } => distribution_ops::run_check_glibc_floor(
            &resolve_repo_input(&resolved.resolved_root, binary_path),
            &glibc_floor,
            args.output_json,
        ),
        ReleaseSubcommand::Preflight {
            tag,
            skip_docs,
            skip_smoke,
            output_path,
        } => {
            let distribution_policy = load_distribution_policy(&resolved.resolved_root)?;
            distribution_ops::run_preflight(
                &resolved.resolved_root,
                &distribution_policy,
                tag.as_deref(),
                skip_docs,
                skip_smoke,
                output_path
                    .as_ref()
                    .map(|path| resolve_repo_input(&resolved.resolved_root, path.clone())),
                args.output_json,
            )
        }
        ReleaseSubcommand::Proof {
            tag,
            crate_version,
            repo_url,
            brew_formula,
            skip_homebrew,
            artifacts_dir,
        } => {
            let distribution_policy = load_distribution_policy(&resolved.resolved_root)?;
            distribution_ops::run_first_publish(distribution_ops::RunFirstPublishRequest {
                repo_root: &resolved.resolved_root,
                distribution_policy: &distribution_policy,
                tag: &tag,
                crate_version: crate_version.as_deref(),
                repo_url: &repo_url,
                brew_formula: &brew_formula,
                skip_homebrew,
                artifacts_dir: artifacts_dir
                    .as_ref()
                    .map(|path| resolve_repo_input(&resolved.resolved_root, path.clone())),
                output_json: args.output_json,
            })
        }
        ReleaseSubcommand::Evidence { subcommand } => {
            let distribution_policy = load_distribution_policy(&resolved.resolved_root)?;
            match subcommand {
                ReleaseEvidenceSubcommand::Validate {
                    artifacts_dir,
                    expect_homebrew,
                } => distribution_ops::run_validate_artifacts(
                    &resolved.resolved_root,
                    &distribution_policy,
                    &resolve_repo_input(&resolved.resolved_root, artifacts_dir),
                    expect_homebrew,
                    args.output_json,
                ),
                ReleaseEvidenceSubcommand::Closeout {
                    tag,
                    artifacts_dir,
                    output_path,
                    owner,
                    expect_homebrew,
                } => distribution_ops::run_generate_closeout(
                    distribution_ops::GenerateCloseoutRequest {
                    repo_root: &resolved.resolved_root,
                    distribution_policy: &distribution_policy,
                    tag: &tag,
                    artifacts_dir: &resolve_repo_input(&resolved.resolved_root, artifacts_dir),
                    output_path: output_path
                        .as_ref()
                        .map(|path| resolve_repo_input(&resolved.resolved_root, path.clone())),
                    owner: &owner,
                    expect_homebrew,
                    output_json: args.output_json,
                }),
                ReleaseEvidenceSubcommand::Summary {
                    tag,
                    artifacts_dir,
                    crate_version,
                    repo_url,
                    brew_formula,
                    homebrew_executed,
                    log_files,
                } => distribution_ops::run_write_summary(
                    effigy_distribution::DistributionSummaryRequest {
                        distribution_policy: &distribution_policy,
                        tag: &tag,
                        artifacts_dir: &resolve_repo_input(
                            &resolved.resolved_root,
                            artifacts_dir,
                        ),
                        crate_version: crate_version.as_deref(),
                        repo_url: &effective_repo_url(&distribution_policy, &repo_url),
                        brew_formula: &effective_brew_formula(
                            &distribution_policy,
                            &brew_formula,
                        ),
                        homebrew_executed,
                        log_files: &log_files,
                        output_json: args.output_json,
                    },
                ),
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
            let requested_version_override = parse_release_version_override(
                &resolved.resolved_root,
                version_override.as_deref(),
                "prepare",
            )?;
            let requested_version_override_for_plan = requested_version_override.clone();
            let requested_version_override_for_yes = requested_version_override.clone();
            run_release_stage(
                ReleaseStageRequest {
                    command_name: "prepare",
                    plan,
                    yes,
                    output_json: args.output_json,
                    interactive_json_error: "interactive release preparation is only available in text mode; use `effigy release prepare --plan` or `effigy release prepare --yes` when `--json` is enabled",
                },
                || {
                    let prepare_plan = collect_release_prepare_plan(
                        &resolved,
                        check_gates,
                        requested_version_override_for_plan.clone(),
                    )?;
                    Ok(ReleaseStageRendered {
                        ok: prepare_plan.ready,
                        json: render_release_prepare_plan_json_payload(&prepare_plan),
                        text: render_release_prepare_plan_text(&prepare_plan),
                    })
                },
                || {
                    let prepared = execute_release_prepare(
                        &resolved,
                        check_gates,
                        requested_version_override_for_yes.clone(),
                    )?;
                    Ok(ReleaseStageRendered {
                        ok: prepared.prepared,
                        json: render_release_prepared_json_payload(&prepared),
                        text: render_release_prepared_text(&prepared),
                    })
                },
                || {
                    if version_override.is_some() {
                        return Err(RunnerError::task_invocation(
                            "`release prepare --version` is only supported with `--plan` or `--yes`; plain interactive `release prepare` already supports custom version review".to_owned(),
                        ));
                    }
                    run_interactive_release_prepare(&resolved, check_gates)
                },
            )
        }
        ReleaseSubcommand::Execute {
            plan,
            yes,
            allow_stale,
        } => run_release_stage(
            ReleaseStageRequest {
                command_name: "execute",
                plan,
                yes,
                output_json: args.output_json,
                interactive_json_error: "interactive release execution is only available in text mode; use `effigy release execute --plan` or `effigy release execute --yes` when `--json` is enabled",
            },
            || {
                let execute_plan = collect_release_execute_plan(&resolved, allow_stale)?;
                Ok(ReleaseStageRendered {
                    ok: execute_plan.ready,
                    json: render_release_execute_plan_json_payload(&execute_plan),
                    text: render_release_execute_plan_text(&execute_plan),
                })
            },
            || {
                let executed = execute_release(&resolved, allow_stale)?;
                Ok(ReleaseStageRendered {
                    ok: executed.executed,
                    json: render_release_executed_json_payload(&executed),
                    text: render_release_executed_text(&executed),
                })
            },
            || run_interactive_release_execute(&resolved, allow_stale),
        ),
    }
}

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

#[cfg(test)]
mod tests;
