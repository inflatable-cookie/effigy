use std::path::{Path, PathBuf};

use effigy_cli::{ReleaseArgs, ReleaseEvidenceSubcommand, ReleaseSubcommand};
use effigy_distribution::{effective_brew_formula, effective_repo_url, load_distribution_policy};
use effigy_release::{
    load_release_config, render_release_gate_run_json as render_release_gate_run_json_payload,
    render_release_gate_run_text, render_release_pipeline_plan_text,
    render_release_plan_json as render_release_plan_json_payload, render_release_receipt_json,
    render_release_receipt_text, render_release_run_json,
    render_release_status_json as render_release_status_json_payload, render_release_status_text,
    render_release_verify_install_json as render_release_verify_install_json_payload,
    render_release_verify_install_text, ReleaseAction, ReleaseRunOutcome, ReleaseStage,
};

use super::command_context::resolve_active_repo_root;
use super::distribution_command::ops as distribution_ops;
use super::RunnerError;
#[cfg(test)]
use ops::resolve_verify_install_repo_url;
use ops::{
    collect_release_plan, collect_release_status, release_receipt, release_run_action,
    run_release_verify_install, run_standalone_release_gates,
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
                let rendered = render_release_status_json_payload(&status);
                if succeeded {
                    return Ok(rendered);
                }
                return Err(RunnerError::CommandJsonFailure { rendered });
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
                    },
                ),
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
                        artifacts_dir: &resolve_repo_input(&resolved.resolved_root, artifacts_dir),
                        crate_version: crate_version.as_deref(),
                        repo_url: &effective_repo_url(&distribution_policy, &repo_url),
                        brew_formula: &effective_brew_formula(&distribution_policy, &brew_formula),
                        homebrew_executed,
                        log_files: &log_files,
                        output_json: args.output_json,
                    },
                ),
            }
        }
        ReleaseSubcommand::Plan { version_override } => {
            let requested_version_override = parse_release_plan_version_override(
                &resolved.resolved_root,
                version_override.as_deref(),
            )?;
            let plan = collect_release_plan(&resolved, requested_version_override)?;
            if args.output_json {
                let rendered = render_release_plan_json_payload(&plan);
                if plan.ready {
                    return Ok(rendered);
                }
                return Err(RunnerError::CommandJsonFailure { rendered });
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
        ReleaseSubcommand::Approve { run } => render_release_run(
            args.output_json,
            release_run_action(&resolved, ReleaseAction::Approve { run }, None)?,
        ),
        ReleaseSubcommand::Receipt { run } => {
            let receipt = release_receipt(&resolved, run.as_deref())?;
            if args.output_json {
                return Ok(render_release_receipt_json(&receipt));
            }
            Ok(render_release_receipt_text(&receipt))
        }
    }
}

/// A release Run reports its receipt plus the lines the Run printed.
fn render_release_run(
    output_json: bool,
    outcome: ReleaseRunOutcome,
) -> Result<String, RunnerError> {
    let receipt = outcome.receipt();
    let succeeded = outcome.succeeded();
    if output_json {
        let rendered = render_release_run_json(
            &receipt,
            &outcome.messages,
            &outcome.receipt_path.display().to_string(),
        );
        if succeeded {
            return Ok(rendered);
        }
        return Err(RunnerError::CommandJsonFailure { rendered });
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

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

#[cfg(test)]
mod tests;
