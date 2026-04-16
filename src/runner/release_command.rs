use std::collections::{BTreeMap, BTreeSet};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::changelog::{self, BumpKind, CategoryKind};
use crate::resolver::ResolvedTarget;
use crate::{ReleaseArgs, ReleaseSubcommand};
use chrono::{Duration, Utc};
use effigy_release::{
    apply_release_mutations as apply_release_mutations_via_release,
    build_changelog_mutation_detail_lines as build_changelog_mutation_detail_lines_via_release,
    build_diff_preview as build_diff_preview_via_release,
    build_version_mutation_detail_lines as build_version_mutation_detail_lines_via_release,
    collect_changed_mutation_paths as collect_changed_mutation_paths_via_release,
    compare_release_state_fingerprints as compare_release_state_fingerprints_via_release,
    format_release_tag, gate_blockers, gate_blockers_if_checked,
    git_add_release_files as git_add_release_files_via_release,
    git_commit_release as git_commit_release_via_release,
    git_create_tag as git_create_tag_via_release,
    git_current_branch as git_current_branch_via_release, git_head_sha as git_head_sha_via_release,
    git_modified_files as git_modified_files_via_release,
    git_push_release as git_push_release_via_release, git_remote_url as git_remote_url_via_release,
    git_tag_exists as git_tag_exists_via_release, load_release_config,
    load_release_prepared_state as load_release_prepared_state_via_release,
    normalized_expected_files as normalized_expected_files_via_release,
    normalized_repo_files as normalized_repo_files_via_release,
    read_current_version as read_current_version_via_release,
    render_changelog_preview_line as render_changelog_preview_line_via_release,
    render_release_execute_plan_json as render_release_execute_plan_json_payload,
    render_release_executed_json as render_release_executed_json_payload,
    render_release_gate_run_json as render_release_gate_run_json_payload,
    render_release_prepare_plan_json as render_release_prepare_plan_json_payload,
    render_release_prepared_json as render_release_prepared_json_payload,
    render_release_resume_json as render_release_resume_json_payload,
    render_release_simulation_json as render_release_simulation_json_payload,
    render_release_status_json as render_release_status_json_payload,
    render_release_verify_install_json as render_release_verify_install_json_payload,
    render_updated_version_contents as render_updated_version_contents_via_release,
    render_version_preview_line as render_version_preview_line_via_release,
    resolve_verify_install_tag as resolve_verify_install_tag_via_release,
    run_release_gate as execute_release_gate,
    run_release_verify_install as run_release_verify_install_via_release,
    snapshot_mutation_paths as snapshot_mutation_paths_via_release, write_release_prepared_state,
    FileMutationApply, FileMutationPlan, GateExecutionReport, GateResult, ReleaseConfig,
    ReleaseError, ReleaseExecutePlan, ReleaseExecuted, ReleaseGateRun, ReleasePreparePlan,
    ReleasePrepared, ReleaseSimulation, ReleaseStatus, ReleaseVerifyInstall, ResolvedGate,
    ResolvedSyncFile, SyncFileKind,
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
    let context = load_release_context(&resolved.resolved_root)?;
    let check_gates = requested_check_gates || !context.config.gates.is_empty();
    let gate_report = if check_gates {
        run_release_gates(&resolved.resolved_root, &context.config.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    let mut plan = build_release_prepare_plan_with_gate_report(
        &context,
        check_gates,
        gate_report.clone(),
        None,
    )?;
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
                    plan = build_release_prepare_plan_with_gate_report(
                        &context,
                        check_gates,
                        gate_report.clone(),
                        Some(selected_version),
                    )?;
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

fn render_release_resume_menu_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let prepared_version = plan
        .prepared_version
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "unavailable".to_owned());
    let drift_count = plan.missing_expected_files.len() + plan.unexpected_files.len();
    vec![
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
        format!("  Prepared version: {prepared_version}"),
        format!("  Tag: {}", plan.tag.as_deref().unwrap_or("unavailable")),
        format!(
            "  Prepared at: {}",
            plan.prepared_at.as_deref().unwrap_or("unavailable")
        ),
        format!("  Stale state: {}", if plan.stale { "yes" } else { "no" }),
        format!(
            "  Drift summary: {} missing / {} unexpected ({drift_count} total)",
            plan.missing_expected_files.len(),
            plan.unexpected_files.len()
        ),
        format!(
            "  Ready to execute immediately: {}",
            if plan.ready { "yes" } else { "no" }
        ),
        format!(
            "  Review handoff available: {}",
            if plan.state_loaded { "yes" } else { "no" }
        ),
        format!("  Source drift items: {}", plan.fingerprint_drift.len()),
        "  Commands: 1=state 2=drift 3=gates 4=reprepare 5=discard review cancel".to_owned(),
        "  Shortcuts: state drift gates reprepare discard r c".to_owned(),
        "  [1] Prepared State Summary".to_owned(),
        "  [2] Drift Since Prepare".to_owned(),
        "  [3] Run release gates now".to_owned(),
        "  [4] Discard current state and re-enter prepare".to_owned(),
        "  [5] Discard prepared state and stop recovery".to_owned(),
        "  [review] Re-enter execute review".to_owned(),
        "  [cancel] Stop without entering execute review".to_owned(),
    ]
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

fn render_prepare_review_menu_lines(
    plan: &ReleasePreparePlan,
    check_gates: bool,
    review_state: PrepareReviewState,
) -> Vec<String> {
    let selected_version = match &plan.planned_version {
        Some(version) if plan.version_override_used => {
            format!("{version} (custom override)")
        }
        Some(version) => version.to_string(),
        None => "unavailable".to_owned(),
    };
    let gate_status = if plan.configured_gate_count == 0 {
        "not configured".to_owned()
    } else if check_gates {
        format!(
            "{} reviewed / {} configured",
            plan.gate_results.len(),
            plan.configured_gate_count
        )
    } else {
        format!("not checked ({} configured)", plan.configured_gate_count)
    };
    let lines = vec![
        format!("  Repository: {}", plan.repo_root.display()),
        "  Current selection:".to_owned(),
        format!(
            "    Suggested version: {}",
            plan.suggested_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!("    Selected version: {selected_version}"),
        format!(
            "    Planned tag: {}",
            plan.tag.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "    Custom override active: {}",
            if plan.version_override_used {
                "yes"
            } else {
                "no"
            }
        ),
        format!("  Mutation count: {}", plan.mutations.len()),
        format!("  Gate review status: {gate_status}"),
        format!(
            "  Reviewed sections: version={} mutations={} gates={} final={}",
            review_label(review_state.version_reviewed, true),
            review_label(review_state.mutations_reviewed, true),
            review_label(
                review_state.gates_reviewed,
                check_gates && plan.configured_gate_count > 0
            ),
            review_label(review_state.final_reviewed, true)
        ),
        "  Commands: 1=version 2=mutations 3=gates 4=final apply cancel".to_owned(),
        "  Shortcuts: v m g f a c".to_owned(),
        format!(
            "  [1] Version Review [{}]",
            review_label(review_state.version_reviewed, true)
        ),
        format!(
            "  [2] Mutation Review [{}]",
            review_label(review_state.mutations_reviewed, true)
        ),
        format!(
            "  [3] Gate Review [{}]",
            review_label(
                review_state.gates_reviewed,
                check_gates && plan.configured_gate_count > 0
            )
        ),
        format!(
            "  [4] Final Approval Preview [{}]",
            review_label(review_state.final_reviewed, true)
        ),
        "  [apply] Apply release preparation".to_owned(),
        "  [cancel] Cancel without applying".to_owned(),
    ];
    lines
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
            &render_prepare_version_review_lines(context, plan),
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

fn render_execute_review_menu_lines(
    plan: &ReleaseExecutePlan,
    stale_acknowledged: bool,
    review_state: ExecuteReviewState,
) -> Vec<String> {
    let prepared_version = match &plan.prepared_version {
        Some(version) if plan.version_override_used => {
            format!("{version} (custom override)")
        }
        Some(version) => version.to_string(),
        None => "unavailable".to_owned(),
    };
    let stale_ack_status = if plan.stale_override_used || stale_acknowledged {
        "recorded"
    } else if plan.stale_override_required {
        "pending"
    } else {
        "not required"
    };
    vec![
        format!("  Repository: {}", plan.repo_root.display()),
        "  Current execute state:".to_owned(),
        format!("    Prepared version: {prepared_version}"),
        format!("    Tag: {}", plan.tag.as_deref().unwrap_or("unavailable")),
        format!("    Stale acknowledgement: {stale_ack_status}"),
        format!(
            "    Ready to execute: {}",
            if plan.ready { "yes" } else { "no" }
        ),
        format!(
            "    Working tree blockers: {} missing / {} unexpected",
            plan.missing_expected_files.len(),
            plan.unexpected_files.len()
        ),
        format!("  Stale warning: {}", if plan.stale { "yes" } else { "no" }),
        format!(
            "  Reviewed sections: stale={} state={} working-tree={} final={}",
            review_label(
                review_state.stale_reviewed,
                plan.stale_override_required || plan.stale
            ),
            review_label(review_state.state_reviewed, true),
            review_label(review_state.working_tree_reviewed, true),
            review_label(review_state.final_reviewed, true)
        ),
        format!("  Source drift items: {}", plan.fingerprint_drift.len()),
        "  Commands: 1=stale 2=state 3=working-tree 4=final 5=gates 6=reprepare 7=discard execute cancel".to_owned(),
        "  Shortcuts: warning state tree final gates reprepare discard x c".to_owned(),
        format!(
            "  [1] Stale Warning Review [{}]",
            review_label(
                review_state.stale_reviewed,
                plan.stale_override_required || plan.stale
            )
        ),
        format!(
            "  [2] Prepared State Review [{}]",
            review_label(review_state.state_reviewed, true)
        ),
        format!(
            "  [3] Working Tree Review [{}]",
            review_label(review_state.working_tree_reviewed, true)
        ),
        format!(
            "  [4] Final Approval Preview [{}]",
            review_label(review_state.final_reviewed, true)
        ),
        "  [5] Run release gates now".to_owned(),
        "  [6] Discard current state and re-enter prepare".to_owned(),
        "  [7] Discard prepared state and stop recovery".to_owned(),
        "  [execute] Execute commit, tag, and push".to_owned(),
        "  [cancel] Cancel without executing".to_owned(),
    ]
}

fn review_label(reviewed: bool, applicable: bool) -> &'static str {
    if !applicable {
        "n/a"
    } else if reviewed {
        "reviewed"
    } else {
        "pending"
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

fn parse_indexed_review_inspection_request(input: &str, item_count: usize) -> Option<usize> {
    let token = input
        .strip_prefix("inspect ")
        .or_else(|| input.strip_prefix("i "))
        .unwrap_or(input)
        .trim();
    let index = token.parse::<usize>().ok()?;
    if (1..=item_count).contains(&index) {
        Some(index - 1)
    } else {
        None
    }
}

fn render_release_gate_run_lines(run: &ReleaseGateRun) -> Vec<String> {
    render_release_gate_run_text(run)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn prompt_release_reprepare_handoff(plan: &ReleaseExecutePlan) -> Result<bool, RunnerError> {
    prompt_release_confirmation(
        "Release Recovery: Reprepare",
        &[
            format!("  Repository: {}", plan.repo_root.display()),
            format!("  Current state file: {}", plan.state_file.display()),
            format!(
                "  Prepared version: {}",
                plan.prepared_version
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "unavailable".to_owned())
            ),
            format!("  Current blockers: {}", plan.blockers.len()),
            "  This will remove the current `.release-prepared.json` and open `effigy release prepare` review.".to_owned(),
        ],
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
        &[
            format!("  Repository: {}", repo_root.display()),
            format!("  State file: {}", state_file.display()),
            format!(
                "  State file present: {}",
                if state_file.exists() { "yes" } else { "no" }
            ),
            "  This discards prepared release recovery state only; it does not revert working-tree changes.".to_owned(),
        ],
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

fn render_release_state_discarded_text(repo_root: &Path, state_file: &Path) -> String {
    [
        "Release Prepared State Discarded".to_owned(),
        format!("  Repository: {}", repo_root.display()),
        format!("  State file: {} (removed)", state_file.display()),
        "  Next step: rerun `effigy release prepare` when you are ready to regenerate release state."
            .to_owned(),
    ]
    .join("\n")
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
    let context = load_release_context(repo_root)?;
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

fn render_prepare_version_review_lines(
    context: &ReleaseContext,
    plan: &ReleasePreparePlan,
) -> Vec<String> {
    let mut lines = vec![
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  Current version: {}", context.current_version),
        format!("  Suggested bump: {}", context.suggested_bump),
    ];
    match &plan.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &plan.planned_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Selected version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Selected version: {version}")),
        None => lines.push("  Selected version: unavailable".to_owned()),
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Planned tag: {tag}"));
    }
    lines.push(format!(
        "  Unreleased entries: {}",
        format_counts(&context.unreleased_counts)
    ));
    lines
}

fn render_prepare_mutation_review_lines(plan: &ReleasePreparePlan) -> Vec<String> {
    let mut lines = vec![format!(
        "  Planned mutation count: {}",
        plan.mutations.len()
    )];
    append_mutation_preview_lines(&mut lines, &plan.mutations);
    lines
}

fn render_prepare_mutation_detail_lines(plan: &ReleasePreparePlan, index: usize) -> Vec<String> {
    let mutation = &plan.mutations[index];
    let mut lines = vec![
        format!("  Mutation: {} of {}", index + 1, plan.mutations.len()),
        format!("  Path: {}", mutation.path.display()),
        format!("  Kind: {}", mutation.kind),
        format!("  Summary: {}", mutation.summary),
        format!("  Before: {}", mutation.before_preview),
        format!("  After:  {}", mutation.after_preview),
    ];
    if !mutation.detail_lines.is_empty() {
        lines.push("  Details:".to_owned());
        for detail in &mutation.detail_lines {
            lines.push(format!("    - {detail}"));
        }
    }
    if !mutation.diff_preview.is_empty() {
        lines.push("  Diff Preview:".to_owned());
        for line in &mutation.diff_preview {
            lines.push(format!("    {line}"));
        }
    }
    lines
}

fn append_mutation_preview_lines(lines: &mut Vec<String>, mutations: &[FileMutationPlan]) {
    for mutation in mutations {
        lines.push(format!(
            "  - {} ({})",
            mutation.path.display(),
            mutation.kind
        ));
        lines.push(format!("    summary: {}", mutation.summary));
        lines.push(format!("    before: {}", mutation.before_preview));
        lines.push(format!("    after:  {}", mutation.after_preview));
        for detail in &mutation.detail_lines {
            lines.push(format!("    detail: {detail}"));
        }
        if !mutation.diff_preview.is_empty() {
            lines.push("    diff:".to_owned());
            for line in &mutation.diff_preview {
                lines.push(format!("      {line}"));
            }
        }
    }
}

fn render_prepare_gate_review_lines(plan: &ReleasePreparePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Configured gates: {}", plan.configured_gate_count),
        format!("  Executed gates: {}", plan.gate_results.len()),
    ];
    if plan.gate_results.is_empty() {
        lines.push("  No gate results were recorded.".to_owned());
        return lines;
    }

    for (index, gate) in plan.gate_results.iter().enumerate() {
        let outcome = if gate.passed { "pass" } else { "fail" };
        let detail = gate
            .exit_code
            .map(|code| format!("exit {code}"))
            .or_else(|| gate.launch_error.clone())
            .unwrap_or_else(|| "ok".to_owned());
        lines.push(format!(
            "  [{}] {}: {} ({}; {}ms)",
            index + 1,
            gate.name,
            outcome,
            detail,
            gate.duration_ms
        ));
    }
    lines
}

fn render_prepare_final_review_lines(plan: &ReleasePreparePlan) -> Vec<String> {
    let mut lines = vec![format!("  Repository: {}", plan.repo_root.display())];
    if let Some(version) = &plan.suggested_version {
        lines.push(format!("  Suggested version: {version}"));
    }
    match &plan.planned_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used: {}",
        if plan.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!("  Release date: {}", plan.release_date));
    lines.push(format!(
        "  State file: {}",
        plan.repo_root.join(RELEASE_PREPARED_STATE_FILE).display()
    ));
    lines.push(format!("  Files to modify: {}", plan.mutations.len()));
    if plan.gates_checked {
        lines.push(format!("  Reviewed gates: {}", plan.gate_results.len()));
    }
    lines
}

fn render_execute_stale_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
    ];
    if let Some(prepared_at) = &plan.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    for warning in &plan.warnings {
        lines.push(format!("  Warning: {warning}"));
    }
    lines.push(
        "  Action required: rerun `effigy release prepare` or acknowledge this stale state now."
            .to_owned(),
    );
    lines
}

fn render_execute_state_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
    ];
    match &plan.previous_version {
        Some(version) => lines.push(format!("  Previous version: {version}")),
        None => lines.push("  Previous version: unavailable".to_owned()),
    }
    if let Some(version) = &plan.suggested_version {
        lines.push(format!("  Suggested version at prepare time: {version}"));
    }
    match &plan.prepared_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag at prepare time: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used during prepare: {}",
        if plan.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(prepared_at) = &plan.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    lines.push(format!(
        "  Stale state warning: {}",
        if plan.stale { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Stale override used: {}",
        if plan.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Gates passed in state: {}",
        if plan.gates_passed { "yes" } else { "no" }
    ));
    lines
}

fn render_execute_working_tree_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  Expected prepared files: {}", plan.expected_files.len()),
        format!("  Detected modified files: {}", plan.modified_files.len()),
    ];
    if !plan.expected_files.is_empty() {
        lines.push("  Expected files:".to_owned());
        for path in &plan.expected_files {
            lines.push(format!("    - {path}"));
        }
    }
    if !plan.modified_files.is_empty() {
        lines.push("  Modified files:".to_owned());
        for path in &plan.modified_files {
            lines.push(format!("    - {path}"));
        }
    }
    lines
}

fn render_release_resume_drift_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![
        format!("  State file: {}", plan.state_file.display()),
        format!(
            "  Prepared at: {}",
            plan.prepared_at.as_deref().unwrap_or("unavailable")
        ),
        format!("  Stale state: {}", if plan.stale { "yes" } else { "no" }),
        format!("  Warning count: {}", plan.warnings.len()),
        format!("  Expected prepared files: {}", plan.expected_files.len()),
        format!("  Detected modified files: {}", plan.modified_files.len()),
        format!(
            "  Drift: {} missing expected / {} unexpected",
            plan.missing_expected_files.len(),
            plan.unexpected_files.len()
        ),
    ];
    if !plan.warnings.is_empty() {
        lines.push("  Warnings:".to_owned());
        for warning in &plan.warnings {
            lines.push(format!("    - {warning}"));
        }
    }
    if !plan.missing_expected_files.is_empty() {
        lines.push("  Missing expected changes:".to_owned());
        for path in &plan.missing_expected_files {
            lines.push(format!("    - {path}"));
        }
    }
    if !plan.unexpected_files.is_empty() {
        lines.push("  Unexpected changes:".to_owned());
        for path in &plan.unexpected_files {
            lines.push(format!("    - {path}"));
        }
    }
    lines
}

fn build_execute_stale_review_items(plan: &ReleaseExecutePlan) -> Vec<ExecuteReviewItem> {
    plan.warnings
        .iter()
        .enumerate()
        .map(|(index, warning)| {
            let mut detail_lines = vec![format!("  Warning: {warning}")];
            if let Some(prepared_at) = &plan.prepared_at {
                detail_lines.push(format!("  Prepared at: {prepared_at}"));
            }
            detail_lines.push(format!(
                "  Stale threshold: {} seconds",
                plan.stale_threshold_seconds
            ));
            detail_lines.push(
                "  Recommended action: rerun `effigy release prepare` unless you intentionally want to continue with stale state."
                    .to_owned(),
            );
            ExecuteReviewItem {
                summary: format!("stale warning {}: {}", index + 1, warning),
                detail_lines,
            }
        })
        .collect()
}

fn build_execute_working_tree_review_items(plan: &ReleaseExecutePlan) -> Vec<ExecuteReviewItem> {
    let mut items = Vec::new();

    for path in &plan.missing_expected_files {
        items.push(ExecuteReviewItem {
            summary: format!("missing expected prepared file: {path}"),
            detail_lines: vec![
                format!("  Category: missing expected prepared file"),
                format!("  Path: {path}"),
                "  Meaning: `.release-prepared.json` expects this file to still be modified, but it is no longer present in the working tree.".to_owned(),
                "  Recommended action: rerun `effigy release prepare` or restore the expected prepared change before executing.".to_owned(),
            ],
        });
    }

    for path in &plan.unexpected_files {
        items.push(ExecuteReviewItem {
            summary: format!("unexpected working tree change: {path}"),
            detail_lines: vec![
                "  Category: unexpected working tree change".to_owned(),
                format!("  Path: {path}"),
                "  Meaning: this file is modified now but was not part of the prepared release state.".to_owned(),
                "  Recommended action: clean or commit the extra change before executing the release.".to_owned(),
            ],
        });
    }

    for path in &plan.expected_files {
        items.push(ExecuteReviewItem {
            summary: format!("expected prepared file: {path}"),
            detail_lines: vec![
                "  Category: expected prepared file".to_owned(),
                format!("  Path: {path}"),
                "  Meaning: this file is part of the prepared release state and should remain modified until execute succeeds.".to_owned(),
            ],
        });
    }

    for path in &plan.modified_files {
        items.push(ExecuteReviewItem {
            summary: format!("detected modified file: {path}"),
            detail_lines: vec![
                "  Category: detected modified file".to_owned(),
                format!("  Path: {path}"),
                "  Meaning: git currently reports this file as modified in the working tree."
                    .to_owned(),
            ],
        });
    }

    for warning in &plan.warnings {
        items.push(ExecuteReviewItem {
            summary: format!("working tree warning: {warning}"),
            detail_lines: vec![
                "  Category: warning".to_owned(),
                format!("  Message: {warning}"),
            ],
        });
    }

    items
}

fn append_indexed_review_hint(lines: &mut Vec<String>, items: &[ExecuteReviewItem], noun: &str) {
    if items.is_empty() {
        return;
    }
    lines.push(format!(
        "  Inspect a specific {noun} with `inspect <n>` or a bare number."
    ));
    for (index, item) in items.iter().enumerate() {
        lines.push(format!("  [{}] {}", index + 1, item.summary));
    }
}

fn render_execute_review_item_detail_lines(
    items: &[ExecuteReviewItem],
    index: usize,
) -> Vec<String> {
    let item = &items[index];
    let mut lines = vec![
        format!("  Item: {} of {}", index + 1, items.len()),
        format!("  Summary: {}", item.summary),
    ];
    lines.extend(item.detail_lines.clone());
    lines
}

fn render_execute_final_review_lines(plan: &ReleaseExecutePlan) -> Vec<String> {
    let mut lines = vec![format!("  Repository: {}", plan.repo_root.display())];
    if let Some(branch) = &plan.branch {
        lines.push(format!("  Branch: {branch}"));
    }
    if let Some(remote) = &plan.remote {
        lines.push(format!("  Remote: {remote}"));
    }
    match &plan.prepared_version {
        Some(version) => lines.push(format!("  Commit message: release: v{version}")),
        None => lines.push("  Commit message: release: vunknown".to_owned()),
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Tag to create: {tag}"));
    }
    lines.push(format!(
        "  Stale override accepted: {}",
        if plan.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  State file removed on success: {}",
        plan.state_file.display()
    ));
    lines
}

#[derive(Debug, Clone)]
struct ReleaseContext {
    repo_root: PathBuf,
    config: ReleaseConfig,
    current_version: semver::Version,
    parsed_changelog: changelog::Changelog,
    changelog_diagnostics: Vec<String>,
    unreleased_counts: BTreeMap<String, usize>,
    unreleased_empty: bool,
    suggested_bump: BumpKind,
    next_version: Option<semver::Version>,
    tag: Option<String>,
    blockers: Vec<String>,
}

#[derive(Debug, Clone)]
struct ExecuteReviewItem {
    summary: String,
    detail_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrepareMenuAction {
    Version,
    Mutations,
    Gates,
    Final,
    Apply,
    Cancel,
}

#[derive(Debug, Clone, Copy, Default)]
struct PrepareReviewState {
    version_reviewed: bool,
    mutations_reviewed: bool,
    gates_reviewed: bool,
    final_reviewed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecuteMenuAction {
    Stale,
    State,
    WorkingTree,
    Final,
    Gates,
    Reprepare,
    Discard,
    Execute,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResumeMenuAction {
    State,
    Drift,
    Gates,
    Reprepare,
    Discard,
    Review,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockedPreflightAction {
    Stop,
    Gates,
    Reprepare,
    Discard,
}

#[derive(Debug, Clone, Copy, Default)]
struct ExecuteReviewState {
    stale_reviewed: bool,
    state_reviewed: bool,
    working_tree_reviewed: bool,
    final_reviewed: bool,
}

fn collect_release_status(
    resolved: &ResolvedTarget,
    check_gates: bool,
) -> Result<ReleaseStatus, RunnerError> {
    let context = load_release_context(&resolved.resolved_root)?;
    if check_gates && !context.config.gates.is_empty() {
        emit_release_progress_line("checking release gates for status");
    }
    let gate_report = if check_gates {
        run_release_gates(&resolved.resolved_root, &context.config.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    let mut blockers = context.blockers.clone();
    if check_gates {
        blockers.extend(gate_blockers(&gate_report.results));
    }

    Ok(ReleaseStatus {
        repo_root: context.repo_root,
        current_version: context.current_version,
        version_source: context.config.version_source,
        changelog_path: context.config.changelog_path,
        changelog_valid: context.changelog_diagnostics.is_empty(),
        changelog_diagnostics: context.changelog_diagnostics,
        unreleased_counts: context.unreleased_counts,
        unreleased_empty: context.unreleased_empty,
        suggested_bump: context.suggested_bump.to_string(),
        next_version: context.next_version,
        tag: context.tag,
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    })
}

fn collect_release_prepare_plan(
    resolved: &ResolvedTarget,
    check_gates: bool,
    version_override: Option<semver::Version>,
) -> Result<ReleasePreparePlan, RunnerError> {
    let context = load_release_context(&resolved.resolved_root)?;
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
    build_release_prepare_plan_with_gate_report(context, check_gates, gate_report, version_override)
}

fn build_release_prepare_plan_with_gate_report(
    context: &ReleaseContext,
    check_gates: bool,
    gate_report: GateExecutionReport,
    version_override: Option<semver::Version>,
) -> Result<ReleasePreparePlan, RunnerError> {
    let release_date = Utc::now().date_naive().to_string();
    let mut blockers = context.blockers.clone();
    let mut mutations = Vec::new();
    let suggested_version = context.next_version.clone();
    let suggested_tag = suggested_version
        .as_ref()
        .map(|version| format_release_tag(&context.config.tag_format, version));
    let version_override_used = version_override.is_some();

    let Some(next_version) = version_override.or_else(|| suggested_version.clone()) else {
        blockers.push("no next version could be derived from changelog content".to_owned());
        blockers.extend(gate_blockers_if_checked(check_gates, &gate_report.results));
        return Ok(ReleasePreparePlan {
            repo_root: context.repo_root.clone(),
            current_version: context.current_version.clone(),
            version_source: context.config.version_source.clone(),
            suggested_version,
            planned_version: None,
            suggested_tag,
            tag: None,
            version_override_used,
            release_date,
            gates_checked: check_gates,
            configured_gate_count: context.config.gates.len(),
            gate_results: gate_report.results,
            blockers: blockers.clone(),
            mutations,
            ready: false,
        });
    };
    let selected_tag = format_release_tag(&context.config.tag_format, &next_version);

    if context
        .parsed_changelog
        .find_version(&next_version.to_string())
        .is_some()
    {
        blockers.push(format!(
            "changelog already contains release version {}",
            next_version
        ));
    }

    if blockers.is_empty() {
        let version_before =
            std::fs::read_to_string(&context.config.version_source.path).map_err(|error| {
                RunnerError::TaskManifestRead {
                    path: context.config.version_source.path.clone(),
                    error,
                }
            })?;
        let changelog_before =
            std::fs::read_to_string(&context.config.changelog_path).map_err(|error| {
                RunnerError::TaskManifestRead {
                    path: context.config.changelog_path.clone(),
                    error,
                }
            })?;
        let version_after = render_updated_version_contents_via_release(
            &context.config.version_source,
            &next_version,
        )
        .map_err(map_release_error)?;
        let changelog_after = render_prepared_changelog_contents(
            &context.parsed_changelog,
            &next_version,
            &release_date,
        )?;

        mutations.push(FileMutationPlan {
            path: context.config.version_source.path.clone(),
            kind: "version-file",
            summary: format!(
                "update version from {} to {}",
                context.current_version, next_version
            ),
            before_preview: render_version_preview_line_via_release(
                &context.config.version_source,
                &version_before,
                &context.current_version.to_string(),
            ),
            after_preview: render_version_preview_line_via_release(
                &context.config.version_source,
                &version_after,
                &next_version.to_string(),
            ),
            detail_lines: build_version_mutation_detail_lines_via_release(
                &context.config.version_source,
                &next_version,
            ),
            diff_preview: build_diff_preview_via_release(&version_before, &version_after),
            apply: FileMutationApply::Write {
                after_contents: version_after.clone(),
            },
        });
        mutations.push(FileMutationPlan {
            path: context.config.changelog_path.clone(),
            kind: "changelog",
            summary: format!(
                "promote [Unreleased] into [{}] - {} and reset [Unreleased]",
                next_version, release_date
            ),
            before_preview: format!(
                "[Unreleased] currently contains {}",
                format_counts(&context.unreleased_counts)
            ),
            after_preview: render_changelog_preview_line_via_release(
                &changelog_after,
                &next_version,
                &release_date,
            ),
            detail_lines: build_changelog_mutation_detail_lines_via_release(
                &context.unreleased_counts,
                &next_version,
                &release_date,
            ),
            diff_preview: build_diff_preview_via_release(&changelog_before, &changelog_after),
            apply: FileMutationApply::Write {
                after_contents: changelog_after.clone(),
            },
        });
        mutations.extend(build_sync_mutations(&context.config.sync_files));
    }

    blockers.extend(gate_blockers_if_checked(check_gates, &gate_report.results));

    Ok(ReleasePreparePlan {
        repo_root: context.repo_root.clone(),
        current_version: context.current_version.clone(),
        version_source: context.config.version_source.clone(),
        suggested_version,
        planned_version: Some(next_version),
        suggested_tag,
        tag: Some(selected_tag),
        version_override_used,
        release_date,
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        blockers: blockers.clone(),
        mutations,
        ready: blockers.is_empty(),
    })
}

fn collect_release_simulation(
    resolved: &ResolvedTarget,
    version_override: Option<semver::Version>,
) -> Result<ReleaseSimulation, RunnerError> {
    let context = load_release_context(&resolved.resolved_root)?;
    if !context.config.gates.is_empty() {
        emit_release_progress_line("checking release gates for simulation");
    }
    let gate_report = run_release_gates(&resolved.resolved_root, &context.config.gates, true);
    let prepare_plan = build_release_prepare_plan_with_gate_report(
        &context,
        true,
        gate_report.clone(),
        version_override,
    )?;
    let state_file = resolved.resolved_root.join(RELEASE_PREPARED_STATE_FILE);
    let state_file_exists = state_file.exists();
    let mut blockers = prepare_plan.blockers.clone();
    if state_file_exists {
        blockers.push(format!(
            "release state file already exists and would block prepare: {}",
            state_file.display()
        ));
    }

    Ok(ReleaseSimulation {
        repo_root: prepare_plan.repo_root,
        current_version: prepare_plan.current_version,
        version_source: prepare_plan.version_source,
        suggested_version: prepare_plan.suggested_version.clone(),
        planned_version: prepare_plan.planned_version.clone(),
        suggested_tag: prepare_plan.suggested_tag.clone(),
        tag: prepare_plan.tag.clone(),
        version_override_used: prepare_plan.version_override_used,
        release_date: prepare_plan.release_date,
        state_file,
        state_file_exists,
        state_file_written: false,
        commit_message: prepare_plan
            .planned_version
            .as_ref()
            .map(|version| format!("release: v{version}")),
        configured_gate_count: prepare_plan.configured_gate_count,
        executed_gate_count: gate_report.results.len(),
        stopped_early: gate_report.stopped_early,
        total_duration_ms: gate_report.total_duration_ms,
        gate_results: gate_report.results,
        mutations: prepare_plan.mutations,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    })
}

fn execute_release_prepare(
    resolved: &ResolvedTarget,
    check_gates: bool,
    version_override: Option<semver::Version>,
) -> Result<ReleasePrepared, RunnerError> {
    let context = load_release_context(&resolved.resolved_root)?;
    let plan = build_release_prepare_plan(&context, false, version_override)?;
    let state_file = resolved.resolved_root.join(RELEASE_PREPARED_STATE_FILE);
    let mut blockers = plan.blockers.clone();

    if state_file.exists() {
        blockers.push(format!(
            "release state file already exists: {}",
            state_file.display()
        ));
    }
    if !check_gates && !context.config.gates.is_empty() {
        blockers.push(
            "release prepare requires `--check-gates` when `[release.gates]` is configured"
                .to_owned(),
        );
    }

    let prepared_version = plan.planned_version.clone();
    let planned_files = plan
        .mutations
        .iter()
        .map(|mutation| mutation.path.clone())
        .collect::<Vec<_>>();
    if !blockers.is_empty() {
        return Ok(ReleasePrepared {
            repo_root: resolved.resolved_root.clone(),
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: false,
            configured_gate_count: context.config.gates.len(),
            gate_results: Vec::new(),
            files_modified: planned_files,
            blockers,
            prepared: false,
            state_file_written: false,
        });
    }

    let snapshots =
        snapshot_mutation_paths_via_release(&plan.mutations).map_err(map_release_error)?;
    if let Err(error) =
        apply_release_mutations_via_release(&resolved.resolved_root, &plan.mutations)
    {
        let files_modified =
            collect_changed_mutation_paths_via_release(&plan.mutations, &snapshots)
                .unwrap_or_else(|_| planned_files.clone());
        return Ok(ReleasePrepared {
            repo_root: resolved.resolved_root.clone(),
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: false,
            configured_gate_count: context.config.gates.len(),
            gate_results: Vec::new(),
            files_modified,
            blockers: vec![error.to_string()],
            prepared: false,
            state_file_written: false,
        });
    }

    if check_gates && !context.config.gates.is_empty() {
        emit_release_progress_line("re-running release gates against prepared files");
    }
    let gate_report = if check_gates {
        run_release_gates(&resolved.resolved_root, &context.config.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    let gate_blockers = gate_blockers_if_checked(check_gates, &gate_report.results);
    let files_modified = collect_changed_mutation_paths_via_release(&plan.mutations, &snapshots)
        .unwrap_or_else(|_| planned_files.clone());
    if !gate_blockers.is_empty() {
        return Ok(ReleasePrepared {
            repo_root: resolved.resolved_root.clone(),
            previous_version: context.current_version,
            suggested_version: plan.suggested_version.clone(),
            prepared_version,
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            state_file,
            gates_checked: check_gates,
            configured_gate_count: context.config.gates.len(),
            gate_results: gate_report.results,
            files_modified,
            blockers: gate_blockers,
            prepared: false,
            state_file_written: false,
        });
    }

    let prepared_branch = git_current_branch_via_release(&resolved.resolved_root).ok();
    let prepared_head = git_head_sha_via_release(&resolved.resolved_root).ok();
    write_release_prepared_state(
        &state_file,
        &resolved.resolved_root,
        &context.current_version,
        plan.suggested_version.as_ref(),
        prepared_version.as_ref(),
        plan.suggested_tag.as_deref(),
        plan.tag.as_deref(),
        plan.version_override_used,
        &plan.release_date,
        check_gates,
        &files_modified,
        prepared_branch.as_deref(),
        prepared_head.as_deref(),
    )
    .map_err(map_release_error)?;

    Ok(ReleasePrepared {
        repo_root: resolved.resolved_root.clone(),
        previous_version: context.current_version,
        suggested_version: plan.suggested_version,
        prepared_version,
        suggested_tag: plan.suggested_tag,
        tag: plan.tag,
        version_override_used: plan.version_override_used,
        release_date: plan.release_date,
        state_file,
        gates_checked: check_gates,
        configured_gate_count: context.config.gates.len(),
        gate_results: gate_report.results,
        files_modified,
        blockers: Vec::new(),
        prepared: true,
        state_file_written: true,
    })
}

fn run_standalone_release_gates(resolved: &ResolvedTarget) -> Result<ReleaseGateRun, RunnerError> {
    let config = load_release_config(&resolved.resolved_root).map_err(map_release_error)?;
    if !config.gates.is_empty() {
        emit_release_progress_line("running standalone release gates");
    }
    let report = run_release_gates(&resolved.resolved_root, &config.gates, true);
    let blockers = gate_blockers(&report.results);

    Ok(ReleaseGateRun {
        repo_root: resolved.resolved_root.clone(),
        configured_gate_count: config.gates.len(),
        executed_gate_count: report.results.len(),
        stopped_early: report.stopped_early,
        total_duration_ms: report.total_duration_ms,
        gate_results: report.results,
        blockers: blockers.clone(),
        passed: blockers.is_empty(),
    })
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
    let repo_root = resolved.resolved_root.clone();
    let state_file = repo_root.join(RELEASE_PREPARED_STATE_FILE);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut previous_version = None;
    let mut suggested_version = None;
    let mut prepared_version = None;
    let mut suggested_tag = None;
    let mut tag = None;
    let mut version_override_used = false;
    let mut release_date = None;
    let mut prepared_at = None;
    let mut state_loaded = false;
    let mut stale = false;
    let mut stale_override_required = false;
    let mut stale_override_used = false;
    let mut gates_checked = false;
    let mut gates_passed = false;
    let mut prepared_branch = None;
    let mut prepared_head = None;
    let mut branch = None;
    let mut current_head = None;
    let mut remote = None;
    let mut expected_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut missing_expected_files = Vec::new();
    let mut unexpected_files = Vec::new();
    let mut source_fingerprint_available = false;
    let mut fingerprint_drift = Vec::new();

    if !state_file.exists() {
        blockers.push(format!(
            "release state file does not exist: {}",
            state_file.display()
        ));
    } else {
        match load_release_prepared_state_via_release(&state_file) {
            Ok(state) => {
                state_loaded = true;
                previous_version = Some(state.previous_version.clone());
                suggested_version = state.suggested_version.clone();
                prepared_version = Some(state.prepared_version.clone());
                suggested_tag = state.suggested_tag.clone();
                tag = state.tag.clone();
                version_override_used = state.version_override_used;
                release_date = state.release_date.clone();
                prepared_at = Some(state.prepared_at_raw.clone());
                gates_checked = state.gates_checked;
                gates_passed = state.gates_passed;
                prepared_branch = state
                    .source_fingerprints
                    .as_ref()
                    .and_then(|fingerprints| fingerprints.prepared_branch.clone());
                prepared_head = state
                    .source_fingerprints
                    .as_ref()
                    .and_then(|fingerprints| fingerprints.prepared_head.clone());
                source_fingerprint_available = state.source_fingerprints.is_some();

                let age = Utc::now().signed_duration_since(state.prepared_at);
                if age > Duration::seconds(RELEASE_STATE_STALE_THRESHOLD_SECS) {
                    stale = true;
                    stale_override_required = !allow_stale;
                    stale_override_used = allow_stale;
                    warnings.push(format!(
                        "release state is stale: prepared {} seconds ago (threshold: {} seconds)",
                        age.num_seconds(),
                        RELEASE_STATE_STALE_THRESHOLD_SECS
                    ));
                    if !allow_stale {
                        blockers.push(
                            "release state is stale; rerun `effigy release prepare` or pass `--allow-stale` to acknowledge and continue"
                                .to_owned(),
                        );
                    }
                }
                if !state.gates_passed {
                    blockers
                        .push("prepared release state reports failed or skipped gates".to_owned());
                }
                match git_current_branch_via_release(&repo_root) {
                    Ok(current_branch) => branch = Some(current_branch),
                    Err(error) => blockers.push(error.to_string()),
                }
                match git_head_sha_via_release(&repo_root) {
                    Ok(head) => current_head = Some(head),
                    Err(error) => blockers.push(error.to_string()),
                }
                match git_remote_url_via_release(&repo_root, "origin") {
                    Ok(url) => remote = Some(url),
                    Err(error) => blockers.push(error.to_string()),
                }
                if let Some(prepared_tag) = state.tag.as_deref() {
                    match git_tag_exists_via_release(&repo_root, prepared_tag) {
                        Ok(true) => blockers.push(format!(
                            "release tag already exists locally: {prepared_tag}"
                        )),
                        Ok(false) => {}
                        Err(error) => blockers.push(error.to_string()),
                    }
                }

                expected_files = normalized_expected_files_via_release(
                    RELEASE_PREPARED_STATE_FILE,
                    &repo_root,
                    &state.files_modified,
                );
                match git_modified_files_via_release(&repo_root) {
                    Ok(paths) => {
                        modified_files = paths;
                        let expected_set = expected_files.iter().cloned().collect::<BTreeSet<_>>();
                        let modified_set = modified_files.iter().cloned().collect::<BTreeSet<_>>();
                        missing_expected_files = expected_set
                            .difference(&modified_set)
                            .cloned()
                            .collect::<Vec<_>>();
                        unexpected_files = modified_set
                            .difference(&expected_set)
                            .cloned()
                            .collect::<Vec<_>>();
                        if !missing_expected_files.is_empty() {
                            blockers.push(format!(
                                "working tree is missing {} expected prepared file change(s)",
                                missing_expected_files.len()
                            ));
                        }
                        if !unexpected_files.is_empty() {
                            blockers.push(format!(
                                "working tree contains {} unexpected change(s)",
                                unexpected_files.len()
                            ));
                        }
                    }
                    Err(error) => blockers.push(error.to_string()),
                }

                if let Some(fingerprints) = &state.source_fingerprints {
                    fingerprint_drift = compare_release_state_fingerprints_via_release(
                        &repo_root,
                        fingerprints,
                        branch.as_deref(),
                        current_head.as_deref(),
                    );
                    if !fingerprint_drift.is_empty() {
                        blockers.push(format!(
                            "prepared release source drift detected in {} place(s)",
                            fingerprint_drift.len()
                        ));
                    }
                } else {
                    warnings.push(
                        "release state does not record source fingerprints; branch, HEAD, and content drift checks are limited"
                            .to_owned(),
                    );
                }
            }
            Err(error) => blockers.push(error.to_string()),
        }
    }

    Ok(ReleaseExecutePlan {
        repo_root,
        state_file,
        previous_version,
        suggested_version,
        prepared_version,
        suggested_tag,
        tag,
        version_override_used,
        release_date,
        prepared_at,
        state_loaded,
        stale,
        stale_threshold_seconds: RELEASE_STATE_STALE_THRESHOLD_SECS,
        stale_override_required,
        stale_override_used,
        gates_checked,
        gates_passed,
        prepared_branch,
        prepared_head,
        branch,
        current_head,
        remote,
        expected_files,
        modified_files,
        missing_expected_files,
        unexpected_files,
        source_fingerprint_available,
        fingerprint_drift,
        warnings,
        blockers: blockers.clone(),
        ready: blockers.is_empty(),
    })
}

fn execute_release(
    resolved: &ResolvedTarget,
    allow_stale: bool,
) -> Result<ReleaseExecuted, RunnerError> {
    let plan = collect_release_execute_plan(resolved, allow_stale)?;
    let state = load_release_prepared_state_via_release(&plan.state_file).ok();
    let files_committed = state
        .as_ref()
        .map(|loaded| {
            normalized_repo_files_via_release(&resolved.resolved_root, &loaded.files_modified)
        })
        .unwrap_or_default();
    let commit_message = plan
        .prepared_version
        .as_ref()
        .map(|version| format!("release: v{version}"));

    if !plan.ready {
        return Ok(ReleaseExecuted {
            repo_root: plan.repo_root,
            state_file: plan.state_file,
            previous_version: plan.previous_version,
            suggested_version: plan.suggested_version,
            prepared_version: plan.prepared_version,
            suggested_tag: plan.suggested_tag,
            tag: plan.tag,
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            prepared_at: plan.prepared_at,
            prepared_branch: plan.prepared_branch,
            prepared_head: plan.prepared_head,
            branch: plan.branch,
            current_head: plan.current_head,
            remote: plan.remote,
            commit_message,
            commit_sha: None,
            stale: plan.stale,
            stale_override_used: plan.stale_override_used,
            fingerprint_drift: plan.fingerprint_drift,
            warnings: plan.warnings,
            blockers: plan.blockers,
            files_committed,
            state_file_removed: false,
            committed: false,
            tag_created: false,
            pushed: false,
            executed: false,
            post_release_instructions: Vec::new(),
        });
    }

    let Some(state) = state else {
        return Ok(ReleaseExecuted {
            repo_root: plan.repo_root,
            state_file: plan.state_file,
            previous_version: plan.previous_version,
            suggested_version: plan.suggested_version,
            prepared_version: plan.prepared_version,
            suggested_tag: plan.suggested_tag,
            tag: plan.tag,
            version_override_used: plan.version_override_used,
            release_date: plan.release_date,
            prepared_at: plan.prepared_at,
            prepared_branch: plan.prepared_branch,
            prepared_head: plan.prepared_head,
            branch: plan.branch,
            current_head: plan.current_head,
            remote: plan.remote,
            commit_message,
            commit_sha: None,
            stale: plan.stale,
            stale_override_used: plan.stale_override_used,
            fingerprint_drift: plan.fingerprint_drift,
            warnings: plan.warnings,
            blockers: vec!["release state became unreadable during execute".to_owned()],
            files_committed,
            state_file_removed: false,
            committed: false,
            tag_created: false,
            pushed: false,
            executed: false,
            post_release_instructions: Vec::new(),
        });
    };

    let branch = plan.branch.clone();
    let remote = plan.remote.clone();
    let tag = plan.tag.clone();
    let mut blockers = Vec::new();
    let mut commit_sha = None;
    let mut committed = false;
    let mut tag_created = false;
    let mut pushed = false;
    let mut state_file_removed = false;

    if let Err(error) =
        git_add_release_files_via_release(&resolved.resolved_root, &state.files_modified)
    {
        blockers.push(error.to_string());
    } else {
        emit_release_progress_line("creating release commit");
        match git_commit_release_via_release(
            &resolved.resolved_root,
            commit_message.as_deref().unwrap_or("release: vunknown"),
        ) {
            Ok(sha) => {
                commit_sha = Some(sha);
                committed = true;
            }
            Err(error) => blockers.push(error.to_string()),
        }
    }

    if blockers.is_empty() {
        if let Some(prepared_tag) = tag.as_deref() {
            emit_release_progress_line(&format!("creating tag `{prepared_tag}`"));
            match git_create_tag_via_release(&resolved.resolved_root, prepared_tag) {
                Ok(()) => tag_created = true,
                Err(error) => blockers.push(error.to_string()),
            }
        }
    }

    if blockers.is_empty() {
        emit_release_progress_line("pushing release commit and tag");
        match git_push_release_via_release(
            &resolved.resolved_root,
            branch.as_deref().unwrap_or("HEAD"),
            "origin",
            tag.as_deref(),
        ) {
            Ok(()) => pushed = true,
            Err(error) => blockers.push(error.to_string()),
        }
    }

    if blockers.is_empty() {
        emit_release_progress_line(&format!("removing {}", plan.state_file.display()));
        std::fs::remove_file(&plan.state_file)
            .map_err(|error| RunnerError::task_invocation_failed_write(&plan.state_file, error))?;
        state_file_removed = true;
    }

    let executed = blockers.is_empty() && committed && pushed;
    let post_release_instructions = if executed {
        build_post_release_instructions(tag.as_deref())
    } else {
        Vec::new()
    };

    Ok(ReleaseExecuted {
        repo_root: plan.repo_root,
        state_file: plan.state_file,
        previous_version: plan.previous_version,
        suggested_version: plan.suggested_version,
        prepared_version: plan.prepared_version,
        suggested_tag: plan.suggested_tag,
        tag,
        version_override_used: plan.version_override_used,
        release_date: plan.release_date,
        prepared_at: plan.prepared_at,
        prepared_branch: plan.prepared_branch,
        prepared_head: plan.prepared_head,
        branch,
        current_head: plan.current_head,
        remote,
        commit_message,
        commit_sha,
        stale: plan.stale,
        stale_override_used: plan.stale_override_used,
        fingerprint_drift: plan.fingerprint_drift,
        warnings: plan.warnings,
        blockers,
        files_committed,
        state_file_removed,
        committed,
        tag_created,
        pushed,
        executed,
        post_release_instructions,
    })
}

fn load_release_context(root: &Path) -> Result<ReleaseContext, RunnerError> {
    let config = load_release_config(root).map_err(map_release_error)?;
    let current_version =
        read_current_version_via_release(&config.version_source).map_err(map_release_error)?;
    let raw_changelog = std::fs::read_to_string(&config.changelog_path).map_err(|error| {
        RunnerError::TaskManifestRead {
            path: config.changelog_path.clone(),
            error,
        }
    })?;
    let parsed_changelog = changelog::parse(&raw_changelog).map_err(|error| {
        RunnerError::task_invocation_failed_parse(&config.changelog_path, error)
    })?;
    let diagnostics = changelog::validate(&parsed_changelog, &raw_changelog);
    let changelog_diagnostics = diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let unreleased_counts = unreleased_counts(&parsed_changelog);
    let unreleased_empty = unreleased_counts.values().copied().sum::<usize>() == 0;
    let suggested_bump = suggested_bump(&parsed_changelog, &current_version, config.pre_1_0);
    let next_version = apply_bump(&current_version, suggested_bump);
    let tag = next_version
        .as_ref()
        .map(|version| format_release_tag(&config.tag_format, version));

    let mut blockers = Vec::new();
    if !changelog_diagnostics.is_empty() {
        blockers.push(format!(
            "changelog validation failed with {} issue(s)",
            changelog_diagnostics.len()
        ));
    }
    if let Some(version) = parsed_changelog
        .latest_version()
        .and_then(|release| release.version.clone())
    {
        if version != current_version {
            blockers.push(format!(
                "version file reports {current_version} but latest changelog release is {version}"
            ));
        }
    }
    if unreleased_empty {
        blockers.push("unreleased changelog section has no entries".to_owned());
    }

    Ok(ReleaseContext {
        repo_root: root.to_path_buf(),
        config,
        current_version,
        parsed_changelog,
        changelog_diagnostics,
        unreleased_counts,
        unreleased_empty,
        suggested_bump,
        next_version,
        tag,
        blockers,
    })
}

fn render_prepared_changelog_contents(
    parsed: &changelog::Changelog,
    next_version: &semver::Version,
    release_date: &str,
) -> Result<String, RunnerError> {
    let Some(unreleased_index) = parsed
        .releases
        .iter()
        .position(|release| release.is_unreleased())
    else {
        return Err(RunnerError::task_invocation(
            "changelog is missing `## [Unreleased]`".to_owned(),
        ));
    };
    let mut updated = parsed.clone();
    let unreleased_categories = updated.releases[unreleased_index].categories.clone();
    updated.releases[unreleased_index].categories.clear();
    updated.releases.insert(
        unreleased_index + 1,
        crate::changelog::Release {
            version: Some(next_version.clone()),
            date: Some(release_date.to_owned()),
            categories: unreleased_categories,
            line: 0,
        },
    );
    Ok(changelog::format(&updated))
}

fn unreleased_counts(changelog: &changelog::Changelog) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    if let Some(unreleased) = changelog.unreleased() {
        for category in &unreleased.categories {
            let count = category.entries.len();
            if count > 0 {
                counts.insert(category.kind.to_string(), count);
            }
        }
    }
    counts
}

fn suggested_bump(
    changelog: &changelog::Changelog,
    current_version: &semver::Version,
    pre_1_0: bool,
) -> BumpKind {
    let Some(unreleased) = changelog.unreleased() else {
        return BumpKind::None;
    };

    let mut has_breaking = false;
    let mut has_minor = false;
    let mut has_patch = false;

    for category in &unreleased.categories {
        if category.entries.is_empty() {
            continue;
        }
        match category.kind {
            CategoryKind::Breaking => has_breaking = true,
            CategoryKind::Added
            | CategoryKind::Changed
            | CategoryKind::Deprecated
            | CategoryKind::Removed => has_minor = true,
            CategoryKind::Fixed | CategoryKind::Security => has_patch = true,
        }
    }

    if !has_breaking && !has_minor && !has_patch {
        return BumpKind::None;
    }
    if has_breaking {
        if current_version.major == 0 && pre_1_0 {
            return BumpKind::Minor;
        }
        return BumpKind::Major;
    }
    if has_minor {
        return if current_version.major == 0 {
            BumpKind::Patch
        } else {
            BumpKind::Minor
        };
    }
    BumpKind::Patch
}

fn apply_bump(version: &semver::Version, bump: BumpKind) -> Option<semver::Version> {
    match bump {
        BumpKind::Major => Some(semver::Version::new(version.major + 1, 0, 0)),
        BumpKind::Minor => Some(semver::Version::new(version.major, version.minor + 1, 0)),
        BumpKind::Patch => Some(semver::Version::new(
            version.major,
            version.minor,
            version.patch + 1,
        )),
        BumpKind::None => None,
    }
}

fn map_release_error(error: ReleaseError) -> RunnerError {
    match error {
        ReleaseError::Manifest(error) => RunnerError::task_invocation(error.to_string()),
        ReleaseError::TaskInvocation(message) => RunnerError::task_invocation(message),
    }
}

fn run_release_gates(root: &Path, gates: &[ResolvedGate], fail_fast: bool) -> GateExecutionReport {
    let started = Instant::now();
    let mut results = Vec::with_capacity(gates.len());
    let mut stopped_early = false;

    for gate in gates {
        emit_release_progress_line(&format!("running gate `{}`", gate.name));
        let result = execute_release_gate(root, gate);
        emit_release_progress_line(&format!(
            "gate `{}` {} ({})",
            result.name,
            if result.passed { "passed" } else { "failed" },
            format_release_progress_duration(result.duration_ms),
        ));
        let passed = result.passed;
        results.push(result);
        if fail_fast && !passed {
            stopped_early = results.len() < gates.len();
            break;
        }
    }
    GateExecutionReport {
        results,
        stopped_early,
        total_duration_ms: started.elapsed().as_millis(),
    }
}

#[derive(Debug, Clone, Copy)]
enum ReleaseBlockedStage {
    Prepare,
    Execute,
}

fn remediation_hints_for_blockers(blockers: &[String], stage: ReleaseBlockedStage) -> Vec<String> {
    let mut hints = BTreeSet::new();
    for blocker in blockers {
        if blocker.starts_with("changelog validation failed")
            || blocker.contains("unreleased changelog section has no entries")
            || blocker.contains("version file reports")
            || blocker.contains("changelog already contains release version")
            || blocker.contains("no next version could be derived")
        {
            hints.insert(
                "Update `CHANGELOG.md` so the unreleased section is valid, non-empty, and aligned with the current version, then rerun `effigy release status`.".to_owned(),
            );
        }
        if blocker.starts_with("gate `") {
            hints.insert(
                "Run `effigy release gates` to inspect the failing gate output, then fix the gate before retrying prepare or execute.".to_owned(),
            );
        }
        if blocker.contains("requires `--check-gates`") {
            hints.insert(
                "Rerun prepare with `--check-gates` so configured release gates are validated before state is written.".to_owned(),
            );
        }
        if blocker.starts_with("release state file does not exist") {
            hints.insert(
                "Run `effigy release prepare` or `effigy release prepare --yes --check-gates` before attempting execute.".to_owned(),
            );
        }
        if blocker.contains("release state is stale") {
            hints.insert(
                "Prefer rerunning `effigy release prepare` to refresh the prepared state; use `--allow-stale` only when the stale state has been deliberately reviewed.".to_owned(),
            );
        }
        if blocker.contains("prepared release source drift detected") {
            hints.insert(
                "Review the reported branch, HEAD, and content drift, then regenerate `.release-prepared.json` with `effigy release prepare` before executing the release. In interactive `release resume` or `release execute`, use `reprepare` to regenerate state or `discard` to clear the old prepared state.".to_owned(),
            );
        }
        if blocker.contains("prepared release state reports failed or skipped gates") {
            hints.insert(
                "Regenerate the prepared state with passing gates so execute is working from a fully validated release preparation.".to_owned(),
            );
        }
        if blocker.contains("requires a checked-out branch") {
            hints.insert(
                "Check out the release branch locally before running `effigy release execute`."
                    .to_owned(),
            );
        }
        if blocker.contains("requires a configured `origin` remote") {
            hints.insert(
                "Configure an `origin` remote for the release repository before execute needs to push branch and tag.".to_owned(),
            );
        }
        if blocker.contains("release tag already exists locally") {
            hints.insert(
                "Do not reuse the prepared tag. Investigate the existing tag and move forward with a new release version if needed.".to_owned(),
            );
        }
        if blocker.contains("working tree is missing") {
            hints.insert(
                "Restore or rerun `effigy release prepare` so every expected prepared file change is present before execute.".to_owned(),
            );
        }
        if blocker.contains("working tree contains") {
            hints.insert(
                "Clean, stash, or commit unrelated working tree changes so only prepared release files remain before execute.".to_owned(),
            );
        }
        if blocker.contains("failed to inspect git repository")
            || blocker.contains("requires a git work tree")
            || blocker.contains("failed to inspect git working tree")
        {
            hints.insert(
                "Run the release command from a valid git work tree and verify `git status` succeeds before retrying.".to_owned(),
            );
        }
        if blocker.contains("failed to stage release files")
            || blocker.contains("failed to create release commit")
            || blocker.contains("failed to create release tag")
            || blocker.contains("failed to push release")
        {
            hints.insert(
                "Resolve the git failure manually, verify the repository state, and retry the release from a clean working tree without reusing a partial tag.".to_owned(),
            );
        }
    }

    if hints.is_empty() {
        hints.insert(match stage {
            ReleaseBlockedStage::Prepare => {
                "Review the blockers above, correct the release inputs, and rerun `effigy release prepare --plan` before applying changes."
                    .to_owned()
            }
            ReleaseBlockedStage::Execute => {
                "Review the blockers above, correct the repository state, and rerun `effigy release execute --plan` before attempting the irreversible execute step."
                    .to_owned()
            }
        });
    }

    hints.into_iter().collect()
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

fn build_sync_mutations(sync_files: &[ResolvedSyncFile]) -> Vec<FileMutationPlan> {
    sync_files
        .iter()
        .map(|sync| match sync.kind {
            SyncFileKind::CargoLock => FileMutationPlan {
                path: sync.path.clone(),
                kind: "sync-file",
                summary: "sync Cargo.lock via `cargo generate-lockfile --quiet`".to_owned(),
                before_preview: if sync.path.exists() {
                    "Cargo.lock exists and will be regenerated".to_owned()
                } else {
                    "Cargo.lock is missing and will be created".to_owned()
                },
                after_preview: "Cargo.lock synced via `cargo generate-lockfile --quiet`".to_owned(),
                detail_lines: vec![
                    "sync command: cargo generate-lockfile --quiet".to_owned(),
                    "preview fidelity: lockfile contents are generated at apply time".to_owned(),
                ],
                diff_preview: Vec::new(),
                apply: FileMutationApply::SyncCargoLock,
            },
        })
        .collect()
}

fn release_progress_enabled() -> bool {
    std::io::stderr().is_terminal()
}

fn emit_release_progress_line(message: &str) {
    if release_progress_enabled() {
        eprintln!("[release] {message}");
    }
}

fn format_release_progress_duration(duration_ms: u128) -> String {
    if duration_ms >= 1_000 {
        format!("{:.1}s", duration_ms as f64 / 1_000.0)
    } else {
        format!("{duration_ms}ms")
    }
}

fn build_post_release_instructions(tag: Option<&str>) -> Vec<String> {
    let mut instructions = vec![
        "Confirm the release CI pipeline starts for the pushed branch and tag.".to_owned(),
        "Monitor the published release artifacts before announcing availability.".to_owned(),
    ];
    if let Some(tag) = tag {
        instructions.push(format!(
            "Verify the remote tag `{tag}` points at the release commit."
        ));
    }
    instructions
}

fn render_release_status_text(status: &ReleaseStatus) -> String {
    let mut lines = vec![
        "Release Status".to_owned(),
        format!("  Repository: {}", status.repo_root.display()),
        format!("  Current version: {}", status.current_version),
        format!(
            "  Version source: {} ({})",
            status.version_source.path.display(),
            status
                .version_source
                .field_path
                .as_deref()
                .unwrap_or("direct")
        ),
        format!(
            "  Changelog: {} ({})",
            if status.changelog_valid {
                "valid"
            } else {
                "invalid"
            },
            status.changelog_path.display()
        ),
    ];

    if status.unreleased_empty {
        lines.push("  Unreleased changes: empty".to_owned());
    } else {
        lines.push(format!(
            "  Unreleased changes: {}",
            format_counts(&status.unreleased_counts)
        ));
    }

    match &status.next_version {
        Some(next_version) => lines.push(format!(
            "  Suggested bump: {} -> {}",
            status.suggested_bump, next_version
        )),
        None => lines.push(format!("  Suggested bump: {}", status.suggested_bump)),
    }

    if let Some(tag) = &status.tag {
        lines.push(format!("  Tag: {tag}"));
    }

    append_gate_status_lines(
        &mut lines,
        status.gates_checked,
        status.configured_gate_count,
        &status.gate_results,
    );
    lines.push(if status.ready {
        if status.gates_checked || status.configured_gate_count == 0 {
            "  Ready: yes".to_owned()
        } else {
            "  Ready: yes (pending gate validation)".to_owned()
        }
    } else {
        "  Ready: no".to_owned()
    });

    append_blockers_and_diagnostics(&mut lines, &status.blockers, &status.changelog_diagnostics);
    lines.join("\n")
}

fn render_release_prepare_plan_text(plan: &ReleasePreparePlan) -> String {
    let mut lines = vec![
        "Release Prepare Plan".to_owned(),
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  Mode: plan-only (non-destructive)"),
        format!("  Current version: {}", plan.current_version),
        format!(
            "  Version source: {} ({})",
            plan.version_source.path.display(),
            plan.version_source
                .field_path
                .as_deref()
                .unwrap_or("direct")
        ),
    ];

    match &plan.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &plan.planned_version {
        Some(version) if plan.version_override_used => {
            lines.push(format!("  Planned version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Planned version: {version}")),
        None => lines.push("  Planned version: unavailable".to_owned()),
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Planned tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used: {}",
        if plan.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!("  Release date: {}", plan.release_date));

    append_gate_status_lines(
        &mut lines,
        plan.gates_checked,
        plan.configured_gate_count,
        &plan.gate_results,
    );
    lines.push(if plan.ready {
        "  Ready to prepare: yes".to_owned()
    } else {
        "  Ready to prepare: no".to_owned()
    });

    if !plan.mutations.is_empty() {
        lines.push(String::new());
        lines.push("Planned Mutations".to_owned());
        append_mutation_preview_lines(&mut lines, &plan.mutations);
    }

    if !plan.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &plan.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&plan.blockers, ReleaseBlockedStage::Prepare) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

fn render_release_simulation_text(simulation: &ReleaseSimulation) -> String {
    let mut lines = vec![
        if simulation.ready {
            "Release Simulation".to_owned()
        } else {
            "Release Simulation Blocked".to_owned()
        },
        format!("  Repository: {}", simulation.repo_root.display()),
        "  Mode: full dry-run (no files written, no git mutations)".to_owned(),
        format!("  Current version: {}", simulation.current_version),
        format!(
            "  Version source: {} ({})",
            simulation.version_source.path.display(),
            simulation
                .version_source
                .field_path
                .as_deref()
                .unwrap_or("direct")
        ),
    ];

    match &simulation.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &simulation.planned_version {
        Some(version) if simulation.version_override_used => {
            lines.push(format!("  Planned version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Planned version: {version}")),
        None => lines.push("  Planned version: unavailable".to_owned()),
    }
    if let Some(tag) = &simulation.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &simulation.tag {
        lines.push(format!("  Planned tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used: {}",
        if simulation.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(commit_message) = &simulation.commit_message {
        lines.push(format!("  Planned commit: {commit_message}"));
    }
    lines.push(format!("  Release date: {}", simulation.release_date));
    lines.push(format!(
        "  State file: {} ({})",
        simulation.state_file.display(),
        if simulation.state_file_exists {
            "already exists"
        } else {
            "not present"
        }
    ));
    lines.push(format!(
        "  State file written: {}",
        if simulation.state_file_written {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Gates executed: {}/{}",
        simulation.executed_gate_count, simulation.configured_gate_count
    ));
    lines.push(format!(
        "  Gate fail-fast stopped early: {}",
        if simulation.stopped_early {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Gate duration: {}ms",
        simulation.total_duration_ms
    ));
    lines.push(format!(
        "  Ready to prepare and execute: {}",
        if simulation.ready { "yes" } else { "no" }
    ));

    append_gate_status_lines(
        &mut lines,
        true,
        simulation.configured_gate_count,
        &simulation.gate_results,
    );

    if !simulation.mutations.is_empty() {
        lines.push(String::new());
        lines.push("Planned Mutations".to_owned());
        append_mutation_preview_lines(&mut lines, &simulation.mutations);
    }

    if !simulation.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &simulation.blockers {
            lines.push(format!("  - {blocker}"));
        }
    }

    lines.join("\n")
}

fn render_release_prepared_text(result: &ReleasePrepared) -> String {
    let mut lines = vec![
        if result.prepared {
            "Release Prepared".to_owned()
        } else {
            "Release Prepare Failed".to_owned()
        },
        format!("  Repository: {}", result.repo_root.display()),
        format!("  Previous version: {}", result.previous_version),
    ];

    match &result.suggested_version {
        Some(version) => lines.push(format!("  Suggested version: {version}")),
        None => lines.push("  Suggested version: unavailable".to_owned()),
    }
    match &result.prepared_version {
        Some(version) if result.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(tag) = &result.suggested_tag {
        lines.push(format!("  Suggested tag: {tag}"));
    }
    if let Some(tag) = &result.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used: {}",
        if result.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!("  Release date: {}", result.release_date));
    lines.push(format!(
        "  State file: {}{}",
        result.state_file.display(),
        if result.state_file_written {
            ""
        } else {
            " (not written)"
        }
    ));
    append_gate_status_lines(
        &mut lines,
        result.gates_checked,
        result.configured_gate_count,
        &result.gate_results,
    );
    lines.push(format!(
        "  Prepared: {}",
        if result.prepared { "yes" } else { "no" }
    ));

    if !result.files_modified.is_empty() {
        lines.push(String::new());
        lines.push("Files Modified".to_owned());
        for path in &result.files_modified {
            lines.push(format!("  - {}", path.display()));
        }
    }
    if !result.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &result.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&result.blockers, ReleaseBlockedStage::Prepare) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

fn render_release_resume_text(plan: &ReleaseExecutePlan) -> String {
    let mut lines = vec![
        if plan.state_loaded {
            "Release Resume".to_owned()
        } else {
            "Release Resume Unavailable".to_owned()
        },
        format!("  Repository: {}", plan.repo_root.display()),
        format!("  State file: {}", plan.state_file.display()),
        format!(
            "  State loaded: {}",
            if plan.state_loaded { "yes" } else { "no" }
        ),
        format!(
            "  Prepared version: {}",
            plan.prepared_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unavailable".to_owned())
        ),
        format!("  Tag: {}", plan.tag.as_deref().unwrap_or("unavailable")),
        format!(
            "  Prepared at: {}",
            plan.prepared_at.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Prepared branch: {}",
            plan.prepared_branch.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Prepared HEAD: {}",
            plan.prepared_head.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Current branch: {}",
            plan.branch.as_deref().unwrap_or("unavailable")
        ),
        format!(
            "  Current HEAD: {}",
            plan.current_head.as_deref().unwrap_or("unavailable")
        ),
        format!("  Stale state: {}", if plan.stale { "yes" } else { "no" }),
        format!(
            "  Drift since prepare: {} missing expected / {} unexpected",
            plan.missing_expected_files.len(),
            plan.unexpected_files.len()
        ),
        format!(
            "  Ready to execute immediately: {}",
            if plan.ready { "yes" } else { "no" }
        ),
        format!(
            "  Can re-enter execute review: {}",
            if plan.state_loaded { "yes" } else { "no" }
        ),
    ];

    if !plan.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings".to_owned());
        for warning in &plan.warnings {
            lines.push(format!("  - {warning}"));
        }
    }
    if !plan.fingerprint_drift.is_empty() {
        lines.push(String::new());
        lines.push("Source Drift".to_owned());
        for drift in &plan.fingerprint_drift {
            lines.push(format!("  - {drift}"));
        }
    }
    if !plan.missing_expected_files.is_empty() {
        lines.push(String::new());
        lines.push("Missing Expected Changes".to_owned());
        for path in &plan.missing_expected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.unexpected_files.is_empty() {
        lines.push(String::new());
        lines.push("Unexpected Changes".to_owned());
        for path in &plan.unexpected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &plan.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&plan.blockers, ReleaseBlockedStage::Execute) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

fn render_release_execute_plan_text(plan: &ReleaseExecutePlan) -> String {
    let mut lines = vec![
        "Release Execute Plan".to_owned(),
        format!("  Repository: {}", plan.repo_root.display()),
        "  Mode: plan-only (preflight)".to_owned(),
        format!("  State file: {}", plan.state_file.display()),
    ];

    match &plan.previous_version {
        Some(version) => lines.push(format!("  Previous version: {version}")),
        None => lines.push("  Previous version: unavailable".to_owned()),
    }
    match &plan.prepared_version {
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(version) = &plan.suggested_version {
        lines.push(format!("  Suggested version at prepare time: {version}"));
    }
    if let Some(tag) = &plan.suggested_tag {
        lines.push(format!("  Suggested tag at prepare time: {tag}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used during prepare: {}",
        if plan.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(release_date) = &plan.release_date {
        lines.push(format!("  Release date: {release_date}"));
    }
    if let Some(prepared_at) = &plan.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    if let Some(branch) = &plan.branch {
        lines.push(format!("  Branch: {branch}"));
    }
    if let Some(remote) = &plan.remote {
        lines.push(format!("  Remote: {remote}"));
    }
    lines.push(format!(
        "  State loaded: {}",
        if plan.state_loaded { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Gates passed in state: {}{}",
        if plan.gates_passed { "yes" } else { "no" },
        if plan.gates_checked {
            ""
        } else {
            " (not checked during prepare)"
        }
    ));
    if let Some(prepared_branch) = &plan.prepared_branch {
        lines.push(format!("  Prepared branch: {prepared_branch}"));
    }
    if let Some(prepared_head) = &plan.prepared_head {
        lines.push(format!("  Prepared HEAD: {prepared_head}"));
    }
    if let Some(current_head) = &plan.current_head {
        lines.push(format!("  Current HEAD: {current_head}"));
    }
    lines.push(format!(
        "  Source fingerprints available: {}",
        if plan.source_fingerprint_available {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Stale: {} (threshold: {}s)",
        if plan.stale { "yes" } else { "no" },
        plan.stale_threshold_seconds
    ));
    lines.push(format!(
        "  Stale override required: {}",
        if plan.stale_override_required {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Stale override accepted: {}",
        if plan.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Ready to execute: {}",
        if plan.ready { "yes" } else { "no" }
    ));

    if !plan.expected_files.is_empty() {
        lines.push(String::new());
        lines.push("Expected Files".to_owned());
        for path in &plan.expected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.modified_files.is_empty() {
        lines.push(String::new());
        lines.push("Working Tree Changes".to_owned());
        for path in &plan.modified_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.missing_expected_files.is_empty() {
        lines.push(String::new());
        lines.push("Missing Expected Changes".to_owned());
        for path in &plan.missing_expected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.unexpected_files.is_empty() {
        lines.push(String::new());
        lines.push("Unexpected Changes".to_owned());
        for path in &plan.unexpected_files {
            lines.push(format!("  - {path}"));
        }
    }
    if !plan.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings".to_owned());
        for warning in &plan.warnings {
            lines.push(format!("  - {warning}"));
        }
    }
    if !plan.fingerprint_drift.is_empty() {
        lines.push(String::new());
        lines.push("Source Drift".to_owned());
        for drift in &plan.fingerprint_drift {
            lines.push(format!("  - {drift}"));
        }
    }
    if !plan.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &plan.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&plan.blockers, ReleaseBlockedStage::Execute) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

fn render_release_gate_run_text(run: &ReleaseGateRun) -> String {
    let mut lines = vec![
        if run.passed {
            "Release Gates".to_owned()
        } else {
            "Release Gates Failed".to_owned()
        },
        format!("  Repository: {}", run.repo_root.display()),
        format!("  Configured gates: {}", run.configured_gate_count),
        format!("  Executed gates: {}", run.executed_gate_count),
        format!(
            "  Fail-fast stopped early: {}",
            if run.stopped_early { "yes" } else { "no" }
        ),
        format!("  Total duration: {}ms", run.total_duration_ms),
        format!("  Passed: {}", if run.passed { "yes" } else { "no" }),
    ];

    if !run.gate_results.is_empty() {
        lines.push(String::new());
        lines.push("Gate Results".to_owned());
        for (index, gate) in run.gate_results.iter().enumerate() {
            let outcome = if gate.passed { "pass" } else { "fail" };
            let detail = gate
                .exit_code
                .map(|code| format!("exit {code}"))
                .or_else(|| gate.launch_error.clone())
                .unwrap_or_else(|| "ok".to_owned());
            lines.push(format!(
                "  [{}] {}: {} ({}; {}ms)",
                index + 1,
                gate.name,
                outcome,
                detail,
                gate.duration_ms
            ));
            if !gate.passed {
                if !gate.stdout.is_empty() {
                    lines.push(format!("    stdout: {}", gate.stdout));
                }
                if !gate.stderr.is_empty() {
                    lines.push(format!("    stderr: {}", gate.stderr));
                }
            }
        }
    }

    if !run.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &run.blockers {
            lines.push(format!("  - {blocker}"));
        }
    }

    lines.join("\n")
}

fn render_release_verify_install_text(result: &ReleaseVerifyInstall) -> String {
    let mut lines = vec![
        if result.verified {
            "Release Install Verification".to_owned()
        } else {
            "Release Install Verification Failed".to_owned()
        },
        format!("  Repository: {}", result.repo_root.display()),
        format!("  Tag: {}", result.tag),
        format!("  Repo URL: {}", result.repo_url),
        format!("  Configured checks: {}", result.configured_check_count),
        format!("  Executed checks: {}", result.executed_check_count),
        format!(
            "  Fail-fast stopped early: {}",
            if result.stopped_early { "yes" } else { "no" }
        ),
        format!("  Verified: {}", if result.verified { "yes" } else { "no" }),
    ];

    if let Some(installed_bin) = &result.installed_bin {
        lines.push(format!("  Installed binary: {}", installed_bin.display()));
    }

    if !result.results.is_empty() {
        lines.push(String::new());
        lines.push("Verification Steps".to_owned());
        for (index, step) in result.results.iter().enumerate() {
            let outcome = if step.passed { "pass" } else { "fail" };
            let detail = step
                .exit_code
                .map(|code| format!("exit {code}"))
                .or_else(|| step.launch_error.clone())
                .unwrap_or_else(|| "ok".to_owned());
            lines.push(format!(
                "  [{}] {}: {} ({}; {}ms)",
                index + 1,
                step.name,
                outcome,
                detail,
                step.duration_ms
            ));
            if !step.passed {
                if !step.stdout.is_empty() {
                    lines.push(format!("    stdout: {}", step.stdout));
                }
                if !step.stderr.is_empty() {
                    lines.push(format!("    stderr: {}", step.stderr));
                }
            }
        }
    }

    if !result.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &result.blockers {
            lines.push(format!("  - {blocker}"));
        }
        lines.push(String::new());
        lines.push("Suggested Actions".to_owned());
        for hint in remediation_hints_for_blockers(&result.blockers, ReleaseBlockedStage::Execute) {
            lines.push(format!("  - {hint}"));
        }
    }

    lines.join("\n")
}

fn render_release_executed_text(result: &ReleaseExecuted) -> String {
    let mut lines = vec![
        if result.executed {
            "Release Executed".to_owned()
        } else {
            "Release Execute Failed".to_owned()
        },
        format!("  Repository: {}", result.repo_root.display()),
        format!("  State file: {}", result.state_file.display()),
    ];

    match &result.previous_version {
        Some(version) => lines.push(format!("  Previous version: {version}")),
        None => lines.push("  Previous version: unavailable".to_owned()),
    }
    match &result.prepared_version {
        Some(version) if result.version_override_used => {
            lines.push(format!("  Prepared version: {version} (custom override)"))
        }
        Some(version) => lines.push(format!("  Prepared version: {version}")),
        None => lines.push("  Prepared version: unavailable".to_owned()),
    }
    if let Some(version) = &result.suggested_version {
        lines.push(format!("  Suggested version at prepare time: {version}"));
    }
    if let Some(tag) = &result.suggested_tag {
        lines.push(format!("  Suggested tag at prepare time: {tag}"));
    }
    if let Some(tag) = &result.tag {
        lines.push(format!("  Tag: {tag}"));
    }
    lines.push(format!(
        "  Version override used during prepare: {}",
        if result.version_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(branch) = &result.branch {
        lines.push(format!("  Branch: {branch}"));
    }
    if let Some(remote) = &result.remote {
        lines.push(format!("  Remote: {remote}"));
    }
    if let Some(release_date) = &result.release_date {
        lines.push(format!("  Release date: {release_date}"));
    }
    if let Some(prepared_at) = &result.prepared_at {
        lines.push(format!("  Prepared at: {prepared_at}"));
    }
    if let Some(prepared_branch) = &result.prepared_branch {
        lines.push(format!("  Prepared branch: {prepared_branch}"));
    }
    if let Some(prepared_head) = &result.prepared_head {
        lines.push(format!("  Prepared HEAD: {prepared_head}"));
    }
    if let Some(current_head) = &result.current_head {
        lines.push(format!("  Current HEAD: {current_head}"));
    }
    if let Some(commit_message) = &result.commit_message {
        lines.push(format!("  Commit message: {commit_message}"));
    }
    if let Some(commit_sha) = &result.commit_sha {
        lines.push(format!("  Commit sha: {commit_sha}"));
    }
    lines.push(format!(
        "  Stale state warning: {}",
        if result.stale { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Stale override used: {}",
        if result.stale_override_used {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Committed: {}",
        if result.committed { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Tag created: {}",
        if result.tag_created { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  Pushed: {}",
        if result.pushed { "yes" } else { "no" }
    ));
    lines.push(format!(
        "  State file removed: {}",
        if result.state_file_removed {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "  Executed: {}",
        if result.executed { "yes" } else { "no" }
    ));

    if !result.files_committed.is_empty() {
        lines.push(String::new());
        lines.push("Files Committed".to_owned());
        for path in &result.files_committed {
            lines.push(format!("  - {path}"));
        }
    }
    if !result.warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings".to_owned());
        for warning in &result.warnings {
            lines.push(format!("  - {warning}"));
        }
    }
    if !result.fingerprint_drift.is_empty() {
        lines.push(String::new());
        lines.push("Source Drift".to_owned());
        for drift in &result.fingerprint_drift {
            lines.push(format!("  - {drift}"));
        }
    }
    if !result.blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in &result.blockers {
            lines.push(format!("  - {blocker}"));
        }
    }
    if !result.post_release_instructions.is_empty() {
        lines.push(String::new());
        lines.push("Post-release Checklist".to_owned());
        for instruction in &result.post_release_instructions {
            lines.push(format!("  - {instruction}"));
        }
    }

    lines.join("\n")
}

fn append_gate_status_lines(
    lines: &mut Vec<String>,
    gates_checked: bool,
    configured_gate_count: usize,
    gate_results: &[GateResult],
) {
    if gates_checked {
        if gate_results.is_empty() {
            lines.push("  Gates: none configured".to_owned());
        } else {
            lines.push("  Gates:".to_owned());
            for gate in gate_results {
                let outcome = if gate.passed { "pass" } else { "fail" };
                let detail = gate
                    .exit_code
                    .map(|code| format!("exit {code}"))
                    .or_else(|| gate.launch_error.clone())
                    .unwrap_or_else(|| "ok".to_owned());
                lines.push(format!(
                    "    {}: {} ({}; {}ms)",
                    gate.name, outcome, detail, gate.duration_ms
                ));
            }
        }
    } else if configured_gate_count == 0 {
        lines.push("  Gates: none configured".to_owned());
    } else {
        lines.push(format!(
            "  Gates: not checked ({} configured)",
            configured_gate_count
        ));
    }
}

fn append_blockers_and_diagnostics(
    lines: &mut Vec<String>,
    blockers: &[String],
    changelog_diagnostics: &[String],
) {
    if !blockers.is_empty() {
        lines.push(String::new());
        lines.push("Blockers".to_owned());
        for blocker in blockers {
            lines.push(format!("  - {blocker}"));
        }
    }

    if !changelog_diagnostics.is_empty() {
        lines.push(String::new());
        lines.push("Changelog Diagnostics".to_owned());
        for diagnostic in changelog_diagnostics {
            lines.push(format!("  - {diagnostic}"));
        }
    }
}

fn format_counts(counts: &BTreeMap<String, usize>) -> String {
    let total = counts.values().copied().sum::<usize>();
    let details = counts
        .iter()
        .map(|(name, count)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{total} ({details})")
}

#[cfg(test)]
mod tests {
    use super::{
        format_release_progress_duration, format_release_tag, load_release_config,
        parse_indexed_review_inspection_request, parse_prepare_mutation_inspection_request,
        remediation_hints_for_blockers, render_execute_review_menu_lines,
        render_prepare_review_menu_lines, render_prepared_changelog_contents,
        resolve_verify_install_repo_url, review_label, suggested_bump,
        validate_prepare_version_override, ExecuteReviewState, PrepareReviewState,
        ReleaseBlockedStage, ReleaseConfig, ReleaseContext, ReleaseExecutePlan, ReleasePreparePlan,
        SyncFileKind,
    };
    use crate::changelog::BumpKind;
    use crate::resolver::ResolvedTarget;
    use crate::tasks::ResolutionMode;
    use effigy_release::normalize_verify_install_repo_url;
    use effigy_release::{
        build_diff_preview, detect_pyproject_version_path, detect_version_file_kind,
        json_value_at_path, render_changelog_preview_line as changelog_preview_line,
        render_updated_version_contents, replace_json_string_at_path_preserving_layout,
        resolve_version_field_path, toml_value_at_path, ResolvedVersionSource, VersionFileKind,
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
    fn release_progress_duration_formats_subsecond_and_seconds() {
        assert_eq!(format_release_progress_duration(842), "842ms");
        assert_eq!(format_release_progress_duration(1_500), "1.5s");
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
