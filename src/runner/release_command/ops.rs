use std::path::Path;

use effigy_core::resolver::ResolvedTarget;
use effigy_release::{
    collect_release_gate_run, collect_release_status as collect_release_status_via_release,
    git_remote_url as git_remote_url_via_release,
    plan_release_pipeline as plan_release_pipeline_via_release,
    read_release_record as read_release_record_via_release,
    release_home as release_home_via_release,
    resolve_verify_install_tag as resolve_verify_install_tag_via_release,
    run_release as run_release_via_release, run_release_gates_with_progress,
    run_release_verify_install as run_release_verify_install_via_release, GateExecutionReport,
    GitEffects, ReleaseAction, ReleaseGateRun, ReleasePipelinePlan, ReleaseRunOutcome,
    ReleaseRunRequest, ReleaseStage, ReleaseStatus, ReleaseVerifyInstall, ResolvedGate,
};

use super::*;

pub(super) fn collect_release_status(
    resolved: &ResolvedTarget,
    check_gates: bool,
) -> Result<ReleaseStatus, RunnerError> {
    // Status resolves the pipeline instead of the shorthand context, so a
    // release with no `semver` identity (contract 054 §4.3) still reports.
    let plan = plan_release_pipeline_via_release(&resolved.resolved_root)?;
    if check_gates && !plan.gates.is_empty() {
        emit_release_progress_line("checking release gates for status");
    }
    let gate_report = if check_gates {
        run_release_gates(&resolved.resolved_root, &plan.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    Ok(collect_release_status_via_release(
        &plan,
        check_gates,
        gate_report,
    ))
}

pub(super) fn collect_release_plan(
    resolved: &ResolvedTarget,
    version_override: Option<semver::Version>,
) -> Result<ReleasePipelinePlan, RunnerError> {
    let mut plan = plan_release_pipeline_via_release(&resolved.resolved_root)?;
    if let Some(version) = version_override {
        apply_release_plan_version_override(&mut plan, &version)?;
    }
    Ok(plan)
}

/// Apply `release --plan --version` to the single `semver` identity.
///
/// The override is the successor of `release simulate --version`: it reports
/// what the pipeline would resolve to, and writes nothing. A release with more
/// than one `semver` identity refuses it, because one flag cannot say which
/// identity it means.
fn apply_release_plan_version_override(
    plan: &mut ReleasePipelinePlan,
    version: &semver::Version,
) -> Result<(), RunnerError> {
    let semver_identities = plan
        .identities
        .iter()
        .enumerate()
        .filter(|(_, identity)| identity.kind == effigy_release::IdentityKind::Semver)
        .map(|(index, identity)| (index, identity.name.clone()))
        .collect::<Vec<_>>();
    let [(index, name)] = semver_identities.as_slice() else {
        return Err(RunnerError::task_invocation(format!(
            "`release --plan --version` needs exactly one `semver` identity; this release declares {} ({})",
            semver_identities.len(),
            semver_identities
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let name = name.clone();
    let identity = &mut plan.identities[*index];
    let Some(current) = identity.current_version.clone() else {
        return Err(RunnerError::task_invocation(format!(
            "`release --plan --version` needs a changelog-derived next version for identity `{name}`"
        )));
    };
    // `== current` stays legal only when the plan already resolves to it: that
    // is the `initial-tag-current-version` release with no changelog history.
    let permits_current = identity.next_version.as_ref() == Some(&current);
    if version < &current || (version == &current && !permits_current) {
        return Err(RunnerError::task_invocation(format!(
            "invalid `release --plan --version`: {version} must be greater than current version {current}"
        )));
    }
    if plan
        .existing_changelog_versions
        .iter()
        .any(|existing| existing == &version.to_string())
    {
        return Err(RunnerError::task_invocation(format!(
            "invalid `release --plan --version`: the changelog already contains release version {version}"
        )));
    }
    identity.tag = identity
        .tag_format
        .as_deref()
        .map(|format| effigy_release::format_release_tag(format, version));
    identity.next_version = Some(version.clone());
    Ok(())
}

/// Run, resume or approve a release Run (contract 054 §3, §5).
pub(super) fn release_run_action(
    resolved: &ResolvedTarget,
    action: ReleaseAction,
    to: Option<ReleaseStage>,
) -> Result<ReleaseRunOutcome, RunnerError> {
    let home = release_home_via_release()?;
    let request = ReleaseRunRequest {
        repo_root: resolved.resolved_root.clone(),
        home,
        action,
        to,
    };
    let mut effects = GitEffects::new(&resolved.resolved_root);
    let outcome = run_release_via_release(&request, &mut effects)?;
    for message in &outcome.messages {
        emit_release_progress_line(message);
    }
    Ok(outcome)
}

/// The release receipt for a Run id, or the active Run's receipt.
pub(super) fn release_receipt(
    resolved: &ResolvedTarget,
    run: Option<&str>,
) -> Result<effigy_release::ReleaseReceipt, RunnerError> {
    let home = release_home_via_release()?;
    let record =
        read_release_record_via_release(&home, &resolved.resolved_root)?.ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "there is no release Run for {}; `effigy release run` starts one",
                resolved.resolved_root.display()
            ))
        })?;
    if let Some(run) = run {
        let run = run.trim();
        if run != record.release_id && run != record.run_id {
            return Err(RunnerError::task_invocation(format!(
                "`{run}` is not this checkout's release Run; the last release Run is {} (stage `{}`, status `{}`)",
                record.release_id,
                if record.stage.is_empty() {
                    "validate"
                } else {
                    &record.stage
                },
                record.status.label()
            )));
        }
    }
    Ok(effigy_release::ReleaseReceipt::from_record(&record))
}

pub(super) fn run_standalone_release_gates(
    resolved: &ResolvedTarget,
) -> Result<ReleaseGateRun, RunnerError> {
    let config = load_release_config(&resolved.resolved_root)?;
    let names = config
        .gates
        .iter()
        .map(|gate| gate.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    emit_release_progress_line(&format!(
        "configured gates ({}): {names}",
        config.gates.len()
    ));
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

pub(super) fn run_release_verify_install(
    resolved: &ResolvedTarget,
    tag: Option<String>,
    repo_url: Option<String>,
) -> Result<ReleaseVerifyInstall, RunnerError> {
    let tag = resolve_verify_install_tag_via_release(tag, std::env::var("GITHUB_REF_NAME").ok())?;
    let repo_url = resolve_verify_install_repo_url(resolved, repo_url)?;
    run_release_verify_install_via_release(resolved.resolved_root.clone(), tag, repo_url)
        .map_err(Into::into)
}

fn run_release_gates(root: &Path, gates: &[ResolvedGate], fail_fast: bool) -> GateExecutionReport {
    run_release_gates_with_progress(root, gates, fail_fast, emit_release_progress_line)
}

pub(super) fn resolve_verify_install_repo_url(
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

    let detected = git_remote_url_via_release(&resolved.resolved_root, "origin")?;
    Ok(effigy_release::normalize_verify_install_repo_url(&detected))
}

pub(super) fn emit_release_progress_line(message: &str) {
    eprintln!("[release] {message}");
}
