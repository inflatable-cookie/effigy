use std::io::{IsTerminal, Write};
use std::path::Path;

use crate::resolver::ResolvedTarget;
use crate::{ReleaseArgs, ReleaseSubcommand};
use effigy_release::{
    append_indexed_review_hint, build_execute_stale_review_items,
    build_execute_working_tree_review_items,
    build_release_prepare_plan as build_release_prepare_plan_via_release,
    collect_release_execute_plan as collect_release_execute_plan_via_release,
    collect_release_gate_run, collect_release_simulation as collect_release_simulation_via_release,
    collect_release_status as collect_release_status_via_release,
    execute_release as execute_release_via_release,
    execute_release_prepare as execute_release_prepare_via_release,
    git_remote_url as git_remote_url_via_release, load_release_config,
    load_release_context as load_release_context_via_release,
    parse_indexed_review_inspection_request, remediation_hints_for_blockers,
    render_execute_final_review_lines, render_execute_review_item_detail_lines,
    render_execute_review_menu_lines, render_execute_stale_review_lines,
    render_execute_state_review_lines, render_execute_working_tree_review_lines,
    render_prepare_final_review_lines, render_prepare_gate_review_lines,
    render_prepare_mutation_detail_lines, render_prepare_mutation_review_lines,
    render_prepare_review_menu_lines, render_prepare_version_review_lines,
    render_release_execute_plan_json as render_release_execute_plan_json_payload,
    render_release_execute_plan_text,
    render_release_executed_json as render_release_executed_json_payload,
    render_release_executed_text,
    render_release_gate_run_json as render_release_gate_run_json_payload,
    render_release_gate_run_lines, render_release_gate_run_text,
    render_release_prepare_plan_json as render_release_prepare_plan_json_payload,
    render_release_prepare_plan_text,
    render_release_prepared_json as render_release_prepared_json_payload,
    render_release_prepared_text, render_release_reprepare_handoff_lines,
    render_release_resume_drift_lines,
    render_release_resume_json as render_release_resume_json_payload,
    render_release_resume_menu_lines, render_release_resume_text,
    render_release_simulation_json as render_release_simulation_json_payload,
    render_release_simulation_text, render_release_state_discard_confirmation_lines,
    render_release_state_discarded_text,
    render_release_status_json as render_release_status_json_payload, render_release_status_text,
    render_release_verify_install_json as render_release_verify_install_json_payload,
    render_release_verify_install_text,
    resolve_verify_install_tag as resolve_verify_install_tag_via_release,
    run_release_gates_with_progress,
    run_release_verify_install as run_release_verify_install_via_release, BlockedPreflightAction,
    ExecuteMenuAction, ExecuteReviewState, GateExecutionReport, PrepareMenuAction,
    PrepareReviewState, ReleaseBlockedStage, ReleaseContext, ReleaseError, ReleaseExecutePlan,
    ReleaseExecuted, ReleaseGateRun, ReleasePreparePlan, ReleasePrepared, ReleaseSimulation,
    ReleaseStatus, ReleaseVerifyInstall, ResolvedGate, ResumeMenuAction,
};

use super::command_context::{current_working_dir, resolve_repo_root};
use super::RunnerError;

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

fn run_interactive_release_prepare(
    resolved: &ResolvedTarget,
    requested_check_gates: bool,
) -> Result<String, RunnerError> {
    let context =
        load_release_context_via_release(&resolved.resolved_root).map_err(map_release_error)?;
    let check_gates = requested_check_gates || !context.config.gates.is_empty();
    let gate_report = if check_gates {
        run_release_gates(&resolved.resolved_root, &context.config.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    let mut plan =
        build_release_prepare_plan_via_release(&context, check_gates, gate_report.clone(), None)
            .map_err(map_release_error)?;
    let rendered_plan = render_release_prepare_plan_text(&plan);
    if !plan.ready {
        return Err(RunnerError::task_invocation(rendered_plan));
    }
    let mut review_state = PrepareReviewState::default();

    loop {
        match prompt_prepare_review_menu(&plan, check_gates, review_state)? {
            PrepareMenuAction::Version => {
                let selected_version = prompt_prepare_version_selection_from_menu(&context, &plan)?;
                review_state.version_reviewed = true;
                if plan.planned_version.as_ref() != Some(&selected_version) {
                    plan = build_release_prepare_plan_via_release(
                        &context,
                        check_gates,
                        gate_report.clone(),
                        Some(selected_version),
                    )
                    .map_err(map_release_error)?;
                }
            }
            PrepareMenuAction::Mutations => {
                prompt_prepare_mutation_browser(&plan)?;
                review_state.mutations_reviewed = true;
            }
            PrepareMenuAction::Gates => {
                if check_gates && !context.config.gates.is_empty() {
                    browse_release_section(
                        "Release Prepare Step 3: Gate Review",
                        &render_prepare_gate_review_lines(&plan),
                        "Press Enter to return to the review menu",
                    )?;
                    review_state.gates_reviewed = true;
                } else {
                    print_release_interactive_notice(
                        "No configured gate results are available for this release prepare review.",
                    )?;
                    review_state.gates_reviewed = true;
                }
            }
            PrepareMenuAction::Final => {
                browse_release_section(
                    "Release Prepare Step 4: Final Approval Preview",
                    &render_prepare_final_review_lines(&plan),
                    "Press Enter to return to the review menu",
                )?;
                review_state.final_reviewed = true;
            }
            PrepareMenuAction::Apply => {
                let prompt = if check_gates && !context.config.gates.is_empty() {
                    "Apply release preparation, write `.release-prepared.json`, and keep the reviewed gate results?"
                } else {
                    "Apply release preparation and write `.release-prepared.json`?"
                };
                if prompt_release_confirmation(
                    "Release Prepare Step 4: Final Approval",
                    &render_prepare_final_review_lines(&plan),
                    prompt,
                )? {
                    break;
                }
                print_release_interactive_notice(
                    "Release preparation not applied. Returning to the review menu.",
                )?;
            }
            PrepareMenuAction::Cancel => {
                return Err(RunnerError::task_invocation(
                    "release preparation cancelled from review menu".to_owned(),
                ));
            }
        }
    }

    let prepared = execute_release_prepare(resolved, check_gates, plan.planned_version.clone())?;
    let rendered = render_release_prepared_text(&prepared);
    if prepared.prepared {
        Ok(rendered)
    } else {
        Err(RunnerError::task_invocation(rendered))
    }
}

fn run_interactive_release_execute(
    resolved: &ResolvedTarget,
    allow_stale: bool,
) -> Result<String, RunnerError> {
    let mut stale_acknowledged = allow_stale;
    let mut review_state = ExecuteReviewState::default();
    loop {
        let plan = collect_release_execute_plan(resolved, stale_acknowledged)?;
        match prompt_execute_review_menu(&plan, stale_acknowledged, review_state)? {
            ExecuteMenuAction::Stale => {
                if plan.stale_override_required || plan.stale {
                    stale_acknowledged =
                        prompt_execute_stale_review_menu(&plan, stale_acknowledged)?;
                    review_state.stale_reviewed = true;
                } else {
                    print_release_interactive_notice(
                        "No stale-state acknowledgement is required for this release execute review.",
                    )?;
                    review_state.stale_reviewed = true;
                }
            }
            ExecuteMenuAction::State => {
                browse_release_section(
                    "Release Execute Step 1: Prepared State Review",
                    &render_execute_state_review_lines(&plan),
                    "Press Enter to return to the review menu",
                )?;
                review_state.state_reviewed = true;
            }
            ExecuteMenuAction::WorkingTree => {
                prompt_execute_working_tree_browser(&plan)?;
                review_state.working_tree_reviewed = true;
            }
            ExecuteMenuAction::Final => {
                browse_release_section(
                    "Release Execute Step 3: Final Approval Preview",
                    &render_execute_final_review_lines(&plan),
                    "Press Enter to return to the review menu",
                )?;
                review_state.final_reviewed = true;
            }
            ExecuteMenuAction::Gates => {
                browse_release_section(
                    "Release Execute Recovery: Gate Check",
                    &render_release_gate_run_lines(&run_standalone_release_gates(resolved)?),
                    "Press Enter to return to the review menu",
                )?;
            }
            ExecuteMenuAction::Reprepare => {
                if prompt_release_reprepare_handoff(&plan)? {
                    discard_release_prepared_state_file(&plan.state_file)?;
                    return run_interactive_release_prepare(resolved, false);
                }
                print_release_interactive_notice(
                    "Reprepare cancelled. Returning to the execute review menu.",
                )?;
            }
            ExecuteMenuAction::Discard => {
                if prompt_release_state_discard_confirmation(
                    "Release Execute Recovery: Discard Prepared State",
                    &plan.repo_root,
                    &plan.state_file,
                    "Discard the prepared state file and stop execute recovery?",
                )? {
                    discard_release_prepared_state_file(&plan.state_file)?;
                    return Ok(render_release_state_discarded_text(
                        &plan.repo_root,
                        &plan.state_file,
                    ));
                }
                print_release_interactive_notice(
                    "Prepared state was kept. Returning to the execute review menu.",
                )?;
            }
            ExecuteMenuAction::Execute => {
                if plan.stale_override_required {
                    print_release_interactive_notice(
                        "A stale prepared state still requires acknowledgement before execute can continue.",
                    )?;
                    continue;
                }
                if !plan.ready {
                    let rendered_plan = render_release_execute_plan_text(&plan);
                    match prompt_execute_blocked_preflight_review(&plan)? {
                        BlockedPreflightAction::Stop => {
                            return Err(RunnerError::task_invocation(rendered_plan));
                        }
                        BlockedPreflightAction::Gates => {
                            browse_release_section(
                                "Release Execute Recovery: Gate Check",
                                &render_release_gate_run_lines(&run_standalone_release_gates(
                                    resolved,
                                )?),
                                "Press Enter to return to the review menu",
                            )?;
                            continue;
                        }
                        BlockedPreflightAction::Reprepare => {
                            if prompt_release_reprepare_handoff(&plan)? {
                                discard_release_prepared_state_file(&plan.state_file)?;
                                return run_interactive_release_prepare(resolved, false);
                            }
                            print_release_interactive_notice(
                                "Reprepare cancelled. Returning to the execute review menu.",
                            )?;
                            continue;
                        }
                        BlockedPreflightAction::Discard => {
                            if prompt_release_state_discard_confirmation(
                                "Release Execute Recovery: Discard Prepared State",
                                &plan.repo_root,
                                &plan.state_file,
                                "Discard the prepared state file and stop execute recovery?",
                            )? {
                                discard_release_prepared_state_file(&plan.state_file)?;
                                return Ok(render_release_state_discarded_text(
                                    &plan.repo_root,
                                    &plan.state_file,
                                ));
                            }
                            print_release_interactive_notice(
                                "Prepared state was kept. Returning to the execute review menu.",
                            )?;
                            continue;
                        }
                    }
                }
                if !prompt_release_confirmation(
                    "Release Execute Step 3: Final Approval",
                    &render_execute_final_review_lines(&plan),
                    "Create the release commit and tag, push to `origin`, and remove `.release-prepared.json` on success?",
                )? {
                    print_release_interactive_notice(
                        "Release execution not applied. Returning to the review menu.",
                    )?;
                    continue;
                }

                let executed = execute_release(resolved, plan.stale_override_used)?;
                let rendered = render_release_executed_text(&executed);
                if executed.executed {
                    return Ok(rendered);
                }
                return Err(RunnerError::task_invocation(rendered));
            }
            ExecuteMenuAction::Cancel => {
                return Err(RunnerError::task_invocation(
                    "release execution cancelled from review menu".to_owned(),
                ));
            }
        }
    }
}

fn run_interactive_release_resume(
    resolved: &ResolvedTarget,
    allow_stale: bool,
) -> Result<String, RunnerError> {
    loop {
        let plan = collect_release_execute_plan(resolved, allow_stale)?;
        match prompt_release_resume_menu(&plan)? {
            ResumeMenuAction::State => {
                browse_release_section(
                    "Release Resume Step 1: Prepared State Summary",
                    &render_execute_state_review_lines(&plan),
                    "Press Enter to return to the recovery menu",
                )?;
            }
            ResumeMenuAction::Drift => {
                prompt_release_resume_drift_browser(&plan)?;
            }
            ResumeMenuAction::Gates => {
                browse_release_section(
                    "Release Resume Recovery: Gate Check",
                    &render_release_gate_run_lines(&run_standalone_release_gates(resolved)?),
                    "Press Enter to return to the recovery menu",
                )?;
            }
            ResumeMenuAction::Reprepare => {
                if prompt_release_reprepare_handoff(&plan)? {
                    discard_release_prepared_state_file(&plan.state_file)?;
                    return run_interactive_release_prepare(resolved, false);
                }
                print_release_interactive_notice(
                    "Reprepare cancelled. Returning to the recovery menu.",
                )?;
            }
            ResumeMenuAction::Discard => {
                if prompt_release_state_discard_confirmation(
                    "Release Resume Recovery: Discard Prepared State",
                    &plan.repo_root,
                    &plan.state_file,
                    "Discard the prepared state file and stop resume recovery?",
                )? {
                    discard_release_prepared_state_file(&plan.state_file)?;
                    return Ok(render_release_state_discarded_text(
                        &plan.repo_root,
                        &plan.state_file,
                    ));
                }
                print_release_interactive_notice(
                    "Prepared state was kept. Returning to the recovery menu.",
                )?;
            }
            ResumeMenuAction::Review => {
                return run_interactive_release_execute(resolved, allow_stale);
            }
            ResumeMenuAction::Cancel => {
                return Err(RunnerError::task_invocation(
                    "release resume cancelled from recovery menu".to_owned(),
                ));
            }
        }
    }
}

fn prompt_release_confirmation(
    title: &str,
    preview_lines: &[String],
    prompt: &str,
) -> Result<bool, RunnerError> {
    let input = prompt_release_input(title, preview_lines, prompt)?;
    Ok(matches!(input.to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn prompt_release_input(
    title: &str,
    preview_lines: &[String],
    prompt: &str,
) -> Result<String, RunnerError> {
    prompt_release_text_input(title, preview_lines, &format!("{prompt} [y/N]"))
}

fn prompt_release_text_input(
    title: &str,
    preview_lines: &[String],
    prompt: &str,
) -> Result<String, RunnerError> {
    let mut stdout = std::io::stdout();
    writeln!(stdout, "{title}")
        .and_then(|_| {
            for line in preview_lines {
                writeln!(stdout, "{line}")?;
            }
            Ok(())
        })
        .and_then(|_| writeln!(stdout))
        .and_then(|_| write!(stdout, "{prompt}: "))
        .and_then(|_| stdout.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive release prompt: {error}"
            ))
        })?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive release confirmation: {error}"
        ))
    })?;
    Ok(input.trim().to_owned())
}

fn browse_release_section(
    title: &str,
    preview_lines: &[String],
    prompt: &str,
) -> Result<(), RunnerError> {
    prompt_release_text_input(title, preview_lines, prompt)?;
    Ok(())
}

fn prompt_release_resume_menu(plan: &ReleaseExecutePlan) -> Result<ResumeMenuAction, RunnerError> {
    loop {
        let response = prompt_release_text_input(
            "Release Resume Recovery Menu",
            &render_release_resume_menu_lines(plan),
            "Recovery menu choice",
        )?;
        match response.trim().to_ascii_lowercase().as_str() {
            "1" | "state" | "prepared" => return Ok(ResumeMenuAction::State),
            "2" | "drift" | "changes" | "working-tree" | "workingtree" => {
                return Ok(ResumeMenuAction::Drift)
            }
            "3" | "gates" | "gate" | "g" => return Ok(ResumeMenuAction::Gates),
            "4" | "reprepare" | "prepare" | "p" | "regen" => {
                return Ok(ResumeMenuAction::Reprepare)
            }
            "5" | "discard" | "drop" | "clear" | "d" => return Ok(ResumeMenuAction::Discard),
            "review" | "resume" | "execute" | "r" => return Ok(ResumeMenuAction::Review),
            "cancel" | "c" | "q" | "quit" | "exit" => return Ok(ResumeMenuAction::Cancel),
            _ => print_release_interactive_notice(
                "Choose `1`, `2`, `3`, `4`, `5`, `review`, or `cancel` from the recovery menu.",
            )?,
        }
    }
}

fn prompt_prepare_review_menu(
    plan: &ReleasePreparePlan,
    check_gates: bool,
    review_state: PrepareReviewState,
) -> Result<PrepareMenuAction, RunnerError> {
    loop {
        let response = prompt_release_text_input(
            "Release Prepare Review Menu",
            &render_prepare_review_menu_lines(plan, check_gates, review_state),
            "Review menu choice",
        )?;
        match response.trim().to_ascii_lowercase().as_str() {
            "1" | "version" | "v" => return Ok(PrepareMenuAction::Version),
            "2" | "mutations" | "mutation" | "m" => return Ok(PrepareMenuAction::Mutations),
            "3" | "gates" | "gate" | "g" => return Ok(PrepareMenuAction::Gates),
            "4" | "final" | "summary" | "f" => return Ok(PrepareMenuAction::Final),
            "apply" | "a" => return Ok(PrepareMenuAction::Apply),
            "cancel" | "c" | "q" | "quit" | "exit" => return Ok(PrepareMenuAction::Cancel),
            _ => print_release_interactive_notice(
                "Choose `1`, `2`, `3`, `4`, `apply`, or `cancel` from the review menu.",
            )?,
        }
    }
}

fn prompt_prepare_version_selection_from_menu(
    context: &ReleaseContext,
    plan: &ReleasePreparePlan,
) -> Result<semver::Version, RunnerError> {
    let selected_version = plan.planned_version.clone().ok_or_else(|| {
        RunnerError::task_invocation("no proposed release version was available".to_owned())
    })?;
    loop {
        let response = prompt_release_text_input(
            "Release Prepare Step 1: Version Review",
            &render_prepare_version_review_lines(
                &context.repo_root,
                &context.current_version,
                &context.suggested_bump.to_string(),
                &context.unreleased_counts,
                plan,
            ),
            &format!(
                "Press Enter to keep {selected_version}, enter a semver override, or type `custom`"
            ),
        )?;
        let normalized = response.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Ok(selected_version.clone());
        }
        if matches!(normalized.as_str(), "back" | "menu") {
            return Ok(selected_version.clone());
        }
        let candidate = if matches!(normalized.as_str(), "c" | "custom" | "edit") {
            let custom = prompt_release_text_input(
                "Release Prepare Step 1a: Custom Version",
                &[
                    format!("  Current version: {}", context.current_version),
                    format!("  Suggested version: {selected_version}"),
                ],
                "Enter a custom release version (semver) or press Enter to keep the current selection",
            )?;
            if custom.trim().is_empty() {
                return Ok(selected_version.clone());
            }
            custom
        } else {
            response
        };

        match validate_prepare_version_override(context, &selected_version, &candidate) {
            Ok(version) => return Ok(version),
            Err(message) => print_release_interactive_notice(&format!(
                "Invalid custom release version: {message}"
            ))?,
        }
    }
}

fn prompt_prepare_mutation_browser(plan: &ReleasePreparePlan) -> Result<(), RunnerError> {
    loop {
        let mut preview_lines = render_prepare_mutation_review_lines(plan);
        preview_lines
            .push("  Inspect a single mutation with `inspect <n>` or a bare number.".to_owned());
        let response = prompt_release_text_input(
            "Release Prepare Step 2: Mutation Review",
            &preview_lines,
            "Press Enter to return to the review menu or use `inspect <n>`",
        )?;
        let normalized = response.trim().to_ascii_lowercase();
        if normalized.is_empty() || matches!(normalized.as_str(), "back" | "menu") {
            return Ok(());
        }
        if let Some(index) =
            parse_prepare_mutation_inspection_request(&normalized, plan.mutations.len())
        {
            prompt_release_text_input(
                "Release Prepare Step 2a: Mutation Inspect",
                &render_prepare_mutation_detail_lines(plan, index),
                "Press Enter to return to mutation review",
            )?;
            continue;
        }

        print_release_interactive_notice(
            "Press Enter to return or use `inspect <n>` to review one mutation.",
        )?;
    }
}

fn parse_prepare_mutation_inspection_request(input: &str, mutation_count: usize) -> Option<usize> {
    parse_indexed_review_inspection_request(input, mutation_count)
}

fn prompt_execute_review_menu(
    plan: &ReleaseExecutePlan,
    stale_acknowledged: bool,
    review_state: ExecuteReviewState,
) -> Result<ExecuteMenuAction, RunnerError> {
    loop {
        let response = prompt_release_text_input(
            "Release Execute Review Menu",
            &render_execute_review_menu_lines(plan, stale_acknowledged, review_state),
            "Review menu choice",
        )?;
        match response.trim().to_ascii_lowercase().as_str() {
            "1" | "stale" | "warning" | "warnings" => return Ok(ExecuteMenuAction::Stale),
            "2" | "state" | "prepared" => return Ok(ExecuteMenuAction::State),
            "3" | "working-tree" | "workingtree" | "files" | "tree" => {
                return Ok(ExecuteMenuAction::WorkingTree)
            }
            "4" | "final" | "summary" => return Ok(ExecuteMenuAction::Final),
            "5" | "gates" | "gate" | "g" => return Ok(ExecuteMenuAction::Gates),
            "6" | "reprepare" | "prepare" | "p" | "regen" => {
                return Ok(ExecuteMenuAction::Reprepare)
            }
            "7" | "discard" | "drop" | "clear" | "d" => return Ok(ExecuteMenuAction::Discard),
            "execute" | "apply" | "run" | "x" => return Ok(ExecuteMenuAction::Execute),
            "cancel" | "c" | "q" | "quit" | "exit" => return Ok(ExecuteMenuAction::Cancel),
            _ => print_release_interactive_notice(
                "Choose `1`, `2`, `3`, `4`, `5`, `6`, `7`, `execute`, or `cancel` from the review menu.",
            )?,
        }
    }
}

fn prompt_execute_stale_review_menu(
    plan: &ReleaseExecutePlan,
    stale_acknowledged: bool,
) -> Result<bool, RunnerError> {
    let items = build_execute_stale_review_items(plan);
    loop {
        let mut preview_lines = render_execute_stale_review_lines(plan);
        append_indexed_review_hint(&mut preview_lines, &items, "warning");
        preview_lines.push(format!(
            "  Stale acknowledgement already recorded: {}",
            if stale_acknowledged { "yes" } else { "no" }
        ));
        let response = prompt_release_text_input(
            "Release Execute Step 0: Stale State Acknowledgement",
            &preview_lines,
            "This prepared state is stale. Acknowledge and continue with execution? [y/N/inspect <n>]",
        )?;
        let normalized = response.trim().to_ascii_lowercase();
        if normalized.is_empty() || matches!(normalized.as_str(), "n" | "no" | "back" | "menu") {
            return Ok(stale_acknowledged);
        }
        if matches!(normalized.as_str(), "y" | "yes") {
            return Ok(true);
        }
        if let Some(index) = parse_indexed_review_inspection_request(&normalized, items.len()) {
            prompt_release_text_input(
                "Release Execute Step 0a: Stale Warning Inspect",
                &render_execute_review_item_detail_lines(&items, index),
                "Press Enter to return to stale-state acknowledgement",
            )?;
            continue;
        }

        print_release_interactive_notice(
            "Enter `y` to continue, `n` to cancel, or `inspect <n>` to review one stale warning.",
        )?;
    }
}

fn prompt_execute_working_tree_browser(plan: &ReleaseExecutePlan) -> Result<(), RunnerError> {
    let items = build_execute_working_tree_review_items(plan);
    loop {
        let mut preview_lines = render_execute_working_tree_review_lines(plan);
        append_indexed_review_hint(&mut preview_lines, &items, "file or warning");
        let response = prompt_release_text_input(
            "Release Execute Step 2: Working Tree Review",
            &preview_lines,
            "Press Enter to return to the review menu or use `inspect <n>`",
        )?;
        let normalized = response.trim().to_ascii_lowercase();
        if normalized.is_empty() || matches!(normalized.as_str(), "back" | "menu") {
            return Ok(());
        }
        if let Some(index) = parse_indexed_review_inspection_request(&normalized, items.len()) {
            prompt_release_text_input(
                "Release Execute Step 2a: Working Tree Inspect",
                &render_execute_review_item_detail_lines(&items, index),
                "Press Enter to return to working tree review",
            )?;
            continue;
        }

        print_release_interactive_notice(
            "Press Enter to return or use `inspect <n>` to review one working tree item.",
        )?;
    }
}

fn prompt_release_resume_drift_browser(plan: &ReleaseExecutePlan) -> Result<(), RunnerError> {
    let items = build_execute_stale_review_items(plan)
        .into_iter()
        .chain(build_execute_working_tree_review_items(plan))
        .collect::<Vec<_>>();
    loop {
        let mut preview_lines = render_release_resume_drift_lines(plan);
        append_indexed_review_hint(&mut preview_lines, &items, "drift item");
        let response = prompt_release_text_input(
            "Release Resume Step 2: Drift Since Prepare",
            &preview_lines,
            "Press Enter to return to the recovery menu or use `inspect <n>`",
        )?;
        let normalized = response.trim().to_ascii_lowercase();
        if normalized.is_empty() || matches!(normalized.as_str(), "back" | "menu") {
            return Ok(());
        }
        if let Some(index) = parse_indexed_review_inspection_request(&normalized, items.len()) {
            prompt_release_text_input(
                "Release Resume Step 2a: Drift Inspect",
                &render_execute_review_item_detail_lines(&items, index),
                "Press Enter to return to drift review",
            )?;
            continue;
        }

        print_release_interactive_notice(
            "Press Enter to return or use `inspect <n>` to review one drift item.",
        )?;
    }
}

fn prompt_execute_blocked_preflight_review(
    plan: &ReleaseExecutePlan,
) -> Result<BlockedPreflightAction, RunnerError> {
    let items = build_execute_stale_review_items(plan)
        .into_iter()
        .chain(build_execute_working_tree_review_items(plan))
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Ok(BlockedPreflightAction::Stop);
    }

    loop {
        let mut preview_lines = render_release_execute_plan_text(plan)
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        preview_lines.push(String::new());
        preview_lines
            .push("  Inspect a stale warning or working tree issue with `inspect <n>`.".to_owned());
        preview_lines.push(
            "  Recovery shortcuts: `gates`, `reprepare`, `discard`, or press Enter to stop."
                .to_owned(),
        );
        for (index, item) in items.iter().enumerate() {
            preview_lines.push(format!("  [{}] {}", index + 1, item.summary));
        }
        let response = prompt_release_text_input(
            "Release Execute Preflight: Blocked Review",
            &preview_lines,
            "Press Enter to stop, `inspect <n>`, or choose `gates`, `reprepare`, or `discard`",
        )?;
        let normalized = response.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Ok(BlockedPreflightAction::Stop);
        }
        if matches!(normalized.as_str(), "gates" | "gate" | "g") {
            return Ok(BlockedPreflightAction::Gates);
        }
        if matches!(normalized.as_str(), "reprepare" | "prepare" | "regen" | "p") {
            return Ok(BlockedPreflightAction::Reprepare);
        }
        if matches!(normalized.as_str(), "discard" | "drop" | "clear" | "d") {
            return Ok(BlockedPreflightAction::Discard);
        }
        if let Some(index) = parse_indexed_review_inspection_request(&normalized, items.len()) {
            prompt_release_text_input(
                "Release Execute Preflight: Item Inspect",
                &render_execute_review_item_detail_lines(&items, index),
                "Press Enter to return to blocked review",
            )?;
            continue;
        }

        print_release_interactive_notice(
            "Press Enter to stop, use `inspect <n>`, or choose `gates`, `reprepare`, or `discard`.",
        )?;
    }
}

fn prompt_release_reprepare_handoff(plan: &ReleaseExecutePlan) -> Result<bool, RunnerError> {
    prompt_release_confirmation(
        "Release Recovery: Reprepare",
        &render_release_reprepare_handoff_lines(plan),
        "Discard the current prepared state and re-enter release prepare?",
    )
}

fn prompt_release_state_discard_confirmation(
    title: &str,
    repo_root: &Path,
    state_file: &Path,
    prompt: &str,
) -> Result<bool, RunnerError> {
    prompt_release_confirmation(
        title,
        &render_release_state_discard_confirmation_lines(repo_root, state_file),
        prompt,
    )
}

fn discard_release_prepared_state_file(state_file: &Path) -> Result<(), RunnerError> {
    if !state_file.exists() {
        return Ok(());
    }
    std::fs::remove_file(state_file)
        .map_err(|error| RunnerError::task_invocation_failed_write(state_file, error))
}

fn print_release_interactive_notice(message: &str) -> Result<(), RunnerError> {
    let mut stdout = std::io::stdout();
    writeln!(stdout, "{message}")
        .and_then(|_| writeln!(stdout))
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive release prompt: {error}"
            ))
        })
}

fn validate_prepare_version_override(
    context: &ReleaseContext,
    suggested_version: &semver::Version,
    raw_version: &str,
) -> Result<semver::Version, String> {
    let version = semver::Version::parse(raw_version.trim())
        .map_err(|error| format!("`{}` is not valid semver: {error}", raw_version.trim()))?;
    if version <= context.current_version {
        return Err(format!(
            "{version} must be greater than current version {}",
            context.current_version
        ));
    }
    if context
        .parsed_changelog
        .find_version(&version.to_string())
        .is_some()
    {
        return Err(format!(
            "changelog already contains release version {version}"
        ));
    }
    if version == *suggested_version {
        return Ok(version);
    }
    Ok(version)
}

fn parse_release_version_override(
    repo_root: &Path,
    raw_version: Option<&str>,
    subcommand: &str,
) -> Result<Option<semver::Version>, RunnerError> {
    let Some(raw_version) = raw_version else {
        return Ok(None);
    };
    let context = load_release_context_via_release(repo_root).map_err(map_release_error)?;
    let suggested_version = context.next_version.clone().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "release {subcommand} `--version` requires a changelog-derived suggested version"
        ))
    })?;
    let version = validate_prepare_version_override(&context, &suggested_version, raw_version)
        .map_err(|message| {
            RunnerError::task_invocation(format!(
                "invalid `release {subcommand} --version`: {message}"
            ))
        })?;
    Ok(Some(version))
}

fn collect_release_status(
    resolved: &ResolvedTarget,
    check_gates: bool,
) -> Result<ReleaseStatus, RunnerError> {
    let context =
        load_release_context_via_release(&resolved.resolved_root).map_err(map_release_error)?;
    if check_gates && !context.config.gates.is_empty() {
        emit_release_progress_line("checking release gates for status");
    }
    let gate_report = if check_gates {
        run_release_gates(&resolved.resolved_root, &context.config.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    Ok(collect_release_status_via_release(
        &context,
        check_gates,
        gate_report,
    ))
}

fn collect_release_prepare_plan(
    resolved: &ResolvedTarget,
    check_gates: bool,
    version_override: Option<semver::Version>,
) -> Result<ReleasePreparePlan, RunnerError> {
    let context =
        load_release_context_via_release(&resolved.resolved_root).map_err(map_release_error)?;
    build_release_prepare_plan(&context, check_gates, version_override)
}

fn build_release_prepare_plan(
    context: &ReleaseContext,
    check_gates: bool,
    version_override: Option<semver::Version>,
) -> Result<ReleasePreparePlan, RunnerError> {
    if check_gates && !context.config.gates.is_empty() {
        emit_release_progress_line("checking release gates for prepare plan");
    }
    let gate_report = if check_gates {
        run_release_gates(&context.repo_root, &context.config.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    build_release_prepare_plan_via_release(context, check_gates, gate_report, version_override)
        .map_err(map_release_error)
}

fn collect_release_simulation(
    resolved: &ResolvedTarget,
    version_override: Option<semver::Version>,
) -> Result<ReleaseSimulation, RunnerError> {
    let context =
        load_release_context_via_release(&resolved.resolved_root).map_err(map_release_error)?;
    if !context.config.gates.is_empty() {
        emit_release_progress_line("checking release gates for simulation");
    }
    let gate_report = run_release_gates(&resolved.resolved_root, &context.config.gates, true);
    let prepare_plan = build_release_prepare_plan_via_release(
        &context,
        true,
        gate_report.clone(),
        version_override,
    )
    .map_err(map_release_error)?;
    Ok(collect_release_simulation_via_release(
        &resolved.resolved_root,
        RELEASE_PREPARED_STATE_FILE,
        prepare_plan,
        &gate_report,
    ))
}

fn execute_release_prepare(
    resolved: &ResolvedTarget,
    check_gates: bool,
    version_override: Option<semver::Version>,
) -> Result<ReleasePrepared, RunnerError> {
    execute_release_prepare_via_release(
        resolved.resolved_root.clone(),
        RELEASE_PREPARED_STATE_FILE,
        check_gates,
        version_override,
        emit_release_progress_line,
    )
    .map_err(map_release_error)
}

fn run_standalone_release_gates(resolved: &ResolvedTarget) -> Result<ReleaseGateRun, RunnerError> {
    let config = load_release_config(&resolved.resolved_root).map_err(map_release_error)?;
    if !config.gates.is_empty() {
        emit_release_progress_line("running standalone release gates");
    }
    let report = run_release_gates(&resolved.resolved_root, &config.gates, true);
    Ok(collect_release_gate_run(
        resolved.resolved_root.clone(),
        config.gates.len(),
        report,
    ))
}

fn run_release_verify_install(
    resolved: &ResolvedTarget,
    tag: Option<String>,
    repo_url: Option<String>,
) -> Result<ReleaseVerifyInstall, RunnerError> {
    let tag = resolve_verify_install_tag_via_release(tag, std::env::var("GITHUB_REF_NAME").ok())
        .map_err(map_release_error)?;
    let repo_url = resolve_verify_install_repo_url(resolved, repo_url)?;
    run_release_verify_install_via_release(resolved.resolved_root.clone(), tag, repo_url)
        .map_err(map_release_error)
}

fn collect_release_execute_plan(
    resolved: &ResolvedTarget,
    allow_stale: bool,
) -> Result<ReleaseExecutePlan, RunnerError> {
    collect_release_execute_plan_via_release(
        resolved.resolved_root.clone(),
        RELEASE_PREPARED_STATE_FILE,
        RELEASE_STATE_STALE_THRESHOLD_SECS,
        allow_stale,
    )
    .map_err(map_release_error)
}

fn execute_release(
    resolved: &ResolvedTarget,
    allow_stale: bool,
) -> Result<ReleaseExecuted, RunnerError> {
    execute_release_via_release(
        resolved.resolved_root.clone(),
        RELEASE_PREPARED_STATE_FILE,
        RELEASE_STATE_STALE_THRESHOLD_SECS,
        allow_stale,
        emit_release_progress_line,
    )
    .map_err(map_release_error)
}

fn map_release_error(error: ReleaseError) -> RunnerError {
    match error {
        ReleaseError::Manifest(error) => RunnerError::task_invocation(error.to_string()),
        ReleaseError::TaskInvocation(message) => RunnerError::task_invocation(message),
    }
}

fn run_release_gates(root: &Path, gates: &[ResolvedGate], fail_fast: bool) -> GateExecutionReport {
    run_release_gates_with_progress(root, gates, fail_fast, emit_release_progress_line)
}

fn resolve_verify_install_repo_url(
    resolved: &ResolvedTarget,
    repo_url: Option<String>,
) -> Result<String, RunnerError> {
    if let Some(repo_url) = repo_url {
        let trimmed = repo_url.trim().to_owned();
        if trimmed.is_empty() {
            return Err(RunnerError::task_invocation(
                "release verify-install `--repo-url` must not be empty".to_owned(),
            ));
        }
        return Ok(effigy_release::normalize_verify_install_repo_url(&trimmed));
    }

    let detected =
        git_remote_url_via_release(&resolved.resolved_root, "origin").map_err(map_release_error)?;
    Ok(effigy_release::normalize_verify_install_repo_url(&detected))
}

fn release_progress_enabled() -> bool {
    std::io::stderr().is_terminal()
}

fn emit_release_progress_line(message: &str) {
    if release_progress_enabled() {
        eprintln!("[release] {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        load_release_config, parse_prepare_mutation_inspection_request,
        remediation_hints_for_blockers, resolve_verify_install_repo_url,
        validate_prepare_version_override, ReleaseBlockedStage,
    };
    use crate::resolver::ResolvedTarget;
    use crate::tasks::ResolutionMode;
    use effigy_release::normalize_verify_install_repo_url;
    use effigy_release::{
        build_diff_preview, detect_pyproject_version_path, detect_version_file_kind,
        format_release_tag, json_value_at_path, parse_indexed_review_inspection_request,
        render_changelog_preview_line as changelog_preview_line, render_execute_review_menu_lines,
        render_prepare_review_menu_lines, render_prepared_changelog_contents,
        render_updated_version_contents, replace_json_string_at_path_preserving_layout,
        resolve_version_field_path, review_label, suggested_bump, toml_value_at_path, BumpKind,
        ExecuteReviewState, PrepareReviewState, ReleaseConfig, ReleaseContext, ReleaseExecutePlan,
        ReleasePreparePlan, ResolvedVersionSource, SyncFileKind, VersionFileKind,
    };

    #[test]
    fn version_file_kind_detection_matches_supported_names() {
        assert_eq!(
            detect_version_file_kind(std::path::Path::new("Cargo.toml")),
            Some(VersionFileKind::CargoToml)
        );
        assert_eq!(
            detect_version_file_kind(std::path::Path::new("package.json")),
            Some(VersionFileKind::PackageJson)
        );
        assert_eq!(
            detect_version_file_kind(std::path::Path::new("pyproject.toml")),
            Some(VersionFileKind::PyProjectToml)
        );
        assert_eq!(
            detect_version_file_kind(std::path::Path::new("VERSION")),
            Some(VersionFileKind::PlainText)
        );
    }

    #[test]
    fn version_field_path_defaults_follow_known_formats() {
        assert_eq!(
            resolve_version_field_path(VersionFileKind::CargoToml, None).expect("default path"),
            Some("package.version".to_owned())
        );
        assert_eq!(
            resolve_version_field_path(VersionFileKind::PackageJson, None).expect("default path"),
            Some("version".to_owned())
        );
        assert_eq!(
            resolve_version_field_path(VersionFileKind::PyProjectToml, None).expect("default path"),
            None
        );
    }

    #[test]
    fn toml_and_json_path_helpers_follow_dot_segments() {
        let toml: toml::Value = "[package]\nversion = \"0.2.4\"\n".parse().expect("toml");
        let json: serde_json::Value =
            serde_json::from_str("{\"package\":{\"version\":\"0.2.4\"}}").expect("json");

        assert_eq!(
            toml_value_at_path(&toml, "package.version").and_then(toml::Value::as_str),
            Some("0.2.4")
        );
        assert_eq!(
            json_value_at_path(&json, "package.version").and_then(serde_json::Value::as_str),
            Some("0.2.4")
        );
    }

    #[test]
    fn detect_pyproject_path_prefers_project_version_when_present() {
        let parsed: toml::Value = "[project]\nversion = \"0.2.4\"\n".parse().expect("toml");
        assert_eq!(
            detect_pyproject_version_path(&parsed),
            Some("project.version")
        );
    }

    #[test]
    fn render_updated_version_contents_supports_json_and_plain_text() {
        let root = std::env::temp_dir().join(format!(
            "effigy-release-version-render-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("mkdir");

        let package_json = root.join("package.json");
        std::fs::write(&package_json, "{\n  \"version\": \"0.2.4\"\n}\n").expect("write json");
        let version_file = root.join("VERSION");
        std::fs::write(&version_file, "0.2.4\n").expect("write version");

        let updated_json = render_updated_version_contents(
            &ResolvedVersionSource {
                path: package_json,
                kind: VersionFileKind::PackageJson,
                field_path: Some("version".to_owned()),
            },
            &semver::Version::new(0, 2, 5),
        )
        .expect("render json");
        let updated_text = render_updated_version_contents(
            &ResolvedVersionSource {
                path: version_file,
                kind: VersionFileKind::PlainText,
                field_path: None,
            },
            &semver::Version::new(0, 2, 5),
        )
        .expect("render version");

        assert!(updated_json.contains("\"version\": \"0.2.5\""));
        assert_eq!(updated_text, "0.2.5\n");
    }

    #[test]
    fn render_updated_version_contents_preserves_toml_comments_and_order() {
        let root = std::env::temp_dir().join(format!(
            "effigy-release-version-render-toml-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("mkdir");

        let cargo_toml = root.join("Cargo.toml");
        std::fs::write(
            &cargo_toml,
            "# leading comment\n[package] # keep package heading comment\nname = \"fixture\"\nversion = \"0.2.4\" # inline version note\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .expect("write cargo");

        let updated = render_updated_version_contents(
            &ResolvedVersionSource {
                path: cargo_toml,
                kind: VersionFileKind::CargoToml,
                field_path: Some("package.version".to_owned()),
            },
            &semver::Version::new(0, 2, 5),
        )
        .expect("render cargo");

        assert!(updated.contains("# leading comment"));
        assert!(updated.contains("[package] # keep package heading comment"));
        assert!(updated.contains("version = \"0.2.5\" # inline version note"));
        assert!(updated.contains("\n\n[dependencies]\nserde = \"1\"\n"));
    }

    #[test]
    fn render_updated_version_contents_preserves_pyproject_comments() {
        let root = std::env::temp_dir().join(format!(
            "effigy-release-version-render-pyproject-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("mkdir");

        let pyproject = root.join("pyproject.toml");
        std::fs::write(
            &pyproject,
            "# pyproject comment\n[project]\nname = \"fixture\"\nversion = \"0.2.4\" # keep comment\n\n[tool.poetry]\nversion = \"9.9.9\"\n",
        )
        .expect("write pyproject");

        let updated = render_updated_version_contents(
            &ResolvedVersionSource {
                path: pyproject,
                kind: VersionFileKind::PyProjectToml,
                field_path: None,
            },
            &semver::Version::new(0, 2, 5),
        )
        .expect("render pyproject");

        assert!(updated.contains("# pyproject comment"));
        assert!(updated.contains("version = \"0.2.5\" # keep comment"));
        assert!(updated.contains("[tool.poetry]\nversion = \"9.9.9\""));
    }

    #[test]
    fn replace_json_string_at_path_preserves_layout_for_nested_version_keys() {
        let updated = replace_json_string_at_path_preserving_layout(
            "{\n  \"package\": {\n    \"name\": \"fixture\",\n    \"version\"  :  \"0.2.4\"\n  },\n  \"unchanged\": [1, {\"flag\": true}]\n}\n",
            "package.version",
            "0.2.5",
        )
        .expect("replace nested json value");

        assert!(updated.contains("\"version\"  :  \"0.2.5\""));
        assert!(updated.contains("\"unchanged\": [1, {\"flag\": true}]"));
    }

    #[test]
    fn render_prepared_changelog_moves_unreleased_entries_into_new_release() {
        let parsed = crate::changelog::parse(
            "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Fix release output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior fix\n",
        )
        .expect("parse changelog");
        let rendered = render_prepared_changelog_contents(
            &parsed,
            &semver::Version::new(0, 2, 5),
            "2026-03-11",
        )
        .expect("render changelog");

        assert!(rendered.contains("## [Unreleased]"));
        assert!(rendered.contains("## [0.2.5] - 2026-03-11"));
        assert_eq!(
            changelog_preview_line(&rendered, &semver::Version::new(0, 2, 5), "2026-03-11"),
            "## [0.2.5] - 2026-03-11"
        );
    }

    #[test]
    fn suggested_bump_respects_pre_1_0_breaking_policy() {
        let changelog = crate::changelog::parse(
            "# Changelog\n\n## [Unreleased]\n\n### Breaking\n- Break\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior fix\n",
        )
        .expect("parse changelog");

        assert_eq!(
            suggested_bump(&changelog, &semver::Version::new(0, 2, 4), true),
            BumpKind::Minor
        );
        assert_eq!(
            suggested_bump(&changelog, &semver::Version::new(0, 2, 4), false),
            BumpKind::Major
        );
    }

    #[test]
    fn validate_prepare_version_override_rejects_non_incrementing_versions() {
        let changelog = crate::changelog::parse(
            "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Fix release output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior fix\n",
        )
        .expect("parse changelog");
        let context = ReleaseContext {
            repo_root: std::env::temp_dir(),
            config: ReleaseConfig {
                version_source: ResolvedVersionSource {
                    path: std::env::temp_dir().join("Cargo.toml"),
                    kind: VersionFileKind::CargoToml,
                    field_path: Some("package.version".to_owned()),
                },
                changelog_path: std::env::temp_dir().join("CHANGELOG.md"),
                pre_1_0: false,
                sync_files: Vec::new(),
                gates: Vec::new(),
                tag_format: "v{version}".to_owned(),
            },
            current_version: semver::Version::new(0, 2, 4),
            parsed_changelog: changelog,
            changelog_diagnostics: Vec::new(),
            unreleased_counts: std::collections::BTreeMap::new(),
            unreleased_empty: false,
            suggested_bump: BumpKind::Patch,
            next_version: Some(semver::Version::new(0, 2, 5)),
            tag: Some(format_release_tag(
                "v{version}",
                &semver::Version::new(0, 2, 5),
            )),
            blockers: Vec::new(),
        };

        let err =
            validate_prepare_version_override(&context, &semver::Version::new(0, 2, 5), "0.2.4")
                .expect_err("current version should be rejected");
        assert!(err.contains("must be greater than current version"));
    }

    #[test]
    fn build_diff_preview_limits_to_concise_changed_lines() {
        let before = "alpha\nbeta\ncharlie\ndelta\necho\nfoxtrot\ngolf\n";
        let after =
            "alpha\nbeta changed\ncharlie\ndelta changed\necho\nfoxtrot changed\ngolf changed\n";

        let preview = build_diff_preview(before, after);

        assert_eq!(
            preview,
            vec![
                "- beta".to_owned(),
                "+ beta changed".to_owned(),
                "- delta".to_owned(),
                "+ delta changed".to_owned(),
                "- foxtrot".to_owned(),
                "+ foxtrot changed".to_owned(),
                "... 1 more changed line(s)".to_owned(),
            ]
        );
    }

    #[test]
    fn parse_prepare_mutation_inspection_request_accepts_keyword_and_bare_index() {
        assert_eq!(
            parse_prepare_mutation_inspection_request("inspect 2", 3),
            Some(1)
        );
        assert_eq!(parse_prepare_mutation_inspection_request("3", 3), Some(2));
        assert_eq!(
            parse_prepare_mutation_inspection_request("inspect 4", 3),
            None
        );
        assert_eq!(
            parse_prepare_mutation_inspection_request("inspect nope", 3),
            None
        );
    }

    #[test]
    fn parse_indexed_review_inspection_request_accepts_short_form() {
        assert_eq!(parse_indexed_review_inspection_request("i 1", 2), Some(0));
        assert_eq!(parse_indexed_review_inspection_request("2", 2), Some(1));
        assert_eq!(parse_indexed_review_inspection_request("0", 2), None);
    }

    #[test]
    fn review_label_marks_pending_reviewed_and_not_applicable() {
        assert_eq!(review_label(false, true), "pending");
        assert_eq!(review_label(true, true), "reviewed");
        assert_eq!(review_label(false, false), "n/a");
    }

    #[test]
    fn remediation_hints_cover_prepare_and_execute_blockers() {
        let prepare_hints = remediation_hints_for_blockers(
            &[
                "unreleased changelog section has no entries".to_owned(),
                "gate `smoke` failed".to_owned(),
            ],
            ReleaseBlockedStage::Prepare,
        );
        assert!(prepare_hints
            .iter()
            .any(|hint| hint.contains("CHANGELOG.md")));
        assert!(prepare_hints
            .iter()
            .any(|hint| hint.contains("effigy release gates")));

        let execute_hints = remediation_hints_for_blockers(
            &[
                "release state is stale; rerun `effigy release prepare` or pass `--allow-stale` to acknowledge and continue".to_owned(),
                "working tree contains 1 unexpected change(s)".to_owned(),
            ],
            ReleaseBlockedStage::Execute,
        );
        assert!(execute_hints
            .iter()
            .any(|hint| hint.contains("--allow-stale")));
        assert!(execute_hints
            .iter()
            .any(|hint| hint.contains("only prepared release files remain")));
    }

    #[test]
    fn normalize_verify_install_repo_url_rewrites_scp_style_ssh_remotes() {
        assert_eq!(
            normalize_verify_install_repo_url("git@github.com:betterthanclay/effigy.git"),
            "ssh://git@github.com/betterthanclay/effigy.git"
        );
        assert_eq!(
            normalize_verify_install_repo_url("github.com:betterthanclay/effigy.git"),
            "ssh://github.com/betterthanclay/effigy.git"
        );
    }

    #[test]
    fn normalize_verify_install_repo_url_keeps_supported_non_ssh_forms() {
        assert_eq!(
            normalize_verify_install_repo_url("https://github.com/betterthanclay/effigy.git"),
            "https://github.com/betterthanclay/effigy.git"
        );
        assert_eq!(
            normalize_verify_install_repo_url("file:///tmp/effigy.git"),
            "file:///tmp/effigy.git"
        );
        assert_eq!(normalize_verify_install_repo_url("../effigy"), "../effigy");
        assert_eq!(
            normalize_verify_install_repo_url("localhost:8080"),
            "localhost:8080"
        );
    }

    #[test]
    fn resolve_verify_install_repo_url_normalizes_origin_ssh_remote() {
        let root = std::env::temp_dir().join(format!(
            "effigy-release-verify-install-remote-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("mkdir");

        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "--quiet"])
            .status()
            .expect("git init");
        assert!(init.success(), "git init should succeed");

        let remote = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "remote",
                "add",
                "origin",
                "git@github.com:betterthanclay/effigy.git",
            ])
            .status()
            .expect("git remote add");
        assert!(remote.success(), "git remote add should succeed");

        let resolved = ResolvedTarget {
            resolved_root: root,
            resolution_mode: ResolutionMode::Explicit,
            evidence: vec!["test fixture".to_owned()],
            warnings: Vec::new(),
        };

        let repo_url =
            resolve_verify_install_repo_url(&resolved, None).expect("resolve repo remote");
        assert_eq!(repo_url, "ssh://git@github.com/betterthanclay/effigy.git");
    }

    #[test]
    fn review_menu_renderers_show_review_markers() {
        let prepare_lines = render_prepare_review_menu_lines(
            &ReleasePreparePlan {
                repo_root: std::env::temp_dir(),
                current_version: semver::Version::new(0, 2, 4),
                version_source: ResolvedVersionSource {
                    path: std::env::temp_dir().join("Cargo.toml"),
                    kind: VersionFileKind::CargoToml,
                    field_path: Some("package.version".to_owned()),
                },
                suggested_version: Some(semver::Version::new(0, 2, 5)),
                planned_version: Some(semver::Version::new(0, 2, 5)),
                suggested_tag: Some("v0.2.5".to_owned()),
                tag: Some("v0.2.5".to_owned()),
                version_override_used: false,
                release_date: "2026-03-11".to_owned(),
                gates_checked: true,
                configured_gate_count: 1,
                gate_results: Vec::new(),
                blockers: Vec::new(),
                mutations: Vec::new(),
                ready: true,
            },
            true,
            PrepareReviewState {
                version_reviewed: true,
                mutations_reviewed: false,
                gates_reviewed: true,
                final_reviewed: false,
            },
        )
        .join("\n");
        assert!(prepare_lines.contains("Reviewed sections: version=reviewed"));
        assert!(prepare_lines.contains("[2] Mutation Review [pending]"));
        assert!(prepare_lines.contains("[3] Gate Review [reviewed]"));

        let execute_lines = render_execute_review_menu_lines(
            &ReleaseExecutePlan {
                repo_root: std::env::temp_dir(),
                state_file: std::env::temp_dir().join(".release-prepared.json"),
                previous_version: Some(semver::Version::new(0, 2, 4)),
                suggested_version: Some(semver::Version::new(0, 2, 5)),
                prepared_version: Some(semver::Version::new(0, 2, 5)),
                suggested_tag: Some("v0.2.5".to_owned()),
                tag: Some("v0.2.5".to_owned()),
                version_override_used: false,
                release_date: Some("2026-03-11".to_owned()),
                prepared_at: Some("2026-03-11T14:00:00+00:00".to_owned()),
                state_loaded: true,
                gates_checked: true,
                gates_passed: true,
                stale: true,
                stale_threshold_seconds: 3600,
                stale_override_required: true,
                stale_override_used: false,
                prepared_branch: Some("main".to_owned()),
                prepared_head: Some("abc123".to_owned()),
                branch: Some("main".to_owned()),
                current_head: Some("abc123".to_owned()),
                remote: Some("origin".to_owned()),
                expected_files: vec!["Cargo.toml".to_owned()],
                modified_files: vec!["Cargo.toml".to_owned()],
                missing_expected_files: Vec::new(),
                unexpected_files: vec!["notes.txt".to_owned()],
                source_fingerprint_available: true,
                fingerprint_drift: Vec::new(),
                warnings: vec!["stale state".to_owned()],
                blockers: vec!["working tree contains 1 unexpected change(s)".to_owned()],
                ready: false,
            },
            false,
            ExecuteReviewState {
                stale_reviewed: true,
                state_reviewed: true,
                working_tree_reviewed: false,
                final_reviewed: false,
            },
        )
        .join("\n");
        assert!(execute_lines.contains("Reviewed sections: stale=reviewed"));
        assert!(execute_lines.contains("[1] Stale Warning Review [reviewed]"));
        assert!(execute_lines.contains("[3] Working Tree Review [pending]"));
    }

    #[test]
    fn current_repo_release_config_matches_self_hosting_release_surfaces() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let config = load_release_config(root).expect("load release config");

        assert_eq!(config.version_source.path, root.join("Cargo.toml"));
        assert_eq!(config.changelog_path, root.join("CHANGELOG.md"));
        assert_eq!(config.tag_format, "v{version}");
        assert_eq!(config.sync_files.len(), 1);
        assert_eq!(config.sync_files[0].path, root.join("Cargo.lock"));
        assert_eq!(config.sync_files[0].kind, SyncFileKind::CargoLock);

        let gate_pairs = config
            .gates
            .iter()
            .map(|gate| (gate.name.as_str(), gate.command.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            gate_pairs,
            vec![
                ("build", "cargo build --release --bin effigy"),
                ("format", "cargo fmt --all -- --check"),
                (
                    "metadata",
                    "cargo run --bin effigy -- distribution validate-metadata"
                ),
                ("qa", "cargo run --bin effigy -- qa:ci"),
                ("smoke", "cargo run --bin effigy -- smoke:release"),
                ("test", "cargo test"),
            ]
        );

        let manifest_source =
            std::fs::read_to_string(root.join("effigy.toml")).expect("read effigy manifest");
        assert!(manifest_source.contains("release/effigy.release.toml"));

        let release_manifest = std::fs::read_to_string(root.join("release/effigy.release.toml"))
            .expect("read release manifest");
        assert!(release_manifest.contains("sync-files = [\"Cargo.lock\"]"));
        assert!(!root.join("scripts/check-release-gates.sh").exists());
        assert!(!root
            .join("scripts/check-release-install-from-tag.sh")
            .exists());
        assert!(!root.join("scripts/check-release-smoke.sh").exists());
        assert!(!root.join("scripts/prepare-release.sh").exists());
    }
}
