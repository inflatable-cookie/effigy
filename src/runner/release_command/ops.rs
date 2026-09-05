use std::path::Path;

use effigy_core::resolver::ResolvedTarget;
use effigy_release::{
    build_release_prepare_plan as build_release_prepare_plan_via_release,
    collect_release_execute_plan as collect_release_execute_plan_via_release,
    collect_release_gate_run, collect_release_simulation as collect_release_simulation_via_release,
    collect_release_status as collect_release_status_via_release,
    execute_release as execute_release_via_release,
    execute_release_prepare as execute_release_prepare_via_release,
    git_remote_url as git_remote_url_via_release,
    load_release_context as load_release_context_via_release,
    resolve_verify_install_tag as resolve_verify_install_tag_via_release,
    run_release_gates_with_progress,
    run_release_verify_install as run_release_verify_install_via_release,
    validate_planned_release_version, GateExecutionReport, ReleaseContext, ReleaseExecutePlan,
    ReleaseExecuted, ReleaseGateRun, ReleasePreparePlan, ReleasePrepared, ReleaseSimulation,
    ReleaseStatus, ReleaseVerifyInstall, ResolvedGate,
};

use super::*;

pub(super) fn validate_prepare_version_override(
    context: &ReleaseContext,
    raw_version: &str,
) -> Result<semver::Version, String> {
    let version = semver::Version::parse(raw_version.trim())
        .map_err(|error| format!("`{}` is not valid semver: {error}", raw_version.trim()))?;
    validate_planned_release_version(context, &version)?;
    Ok(version)
}

pub(super) fn parse_release_version_override(
    repo_root: &Path,
    raw_version: Option<&str>,
    subcommand: &str,
) -> Result<Option<semver::Version>, RunnerError> {
    let Some(raw_version) = raw_version else {
        return Ok(None);
    };
    let context = load_release_context_via_release(repo_root)?;
    context.next_version.as_ref().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "release {subcommand} `--version` requires a changelog-derived suggested version"
        ))
    })?;
    let version = validate_prepare_version_override(&context, raw_version).map_err(|message| {
        RunnerError::task_invocation(format!(
            "invalid `release {subcommand} --version`: {message}"
        ))
    })?;
    Ok(Some(version))
}

pub(super) fn collect_release_status(
    resolved: &ResolvedTarget,
    check_gates: bool,
) -> Result<ReleaseStatus, RunnerError> {
    let context = load_release_context_via_release(&resolved.resolved_root)?;
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

pub(super) fn collect_release_prepare_plan(
    resolved: &ResolvedTarget,
    check_gates: bool,
    version_override: Option<semver::Version>,
) -> Result<ReleasePreparePlan, RunnerError> {
    let context = load_release_context_via_release(&resolved.resolved_root)?;
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
        .map_err(Into::into)
}

pub(super) fn collect_release_simulation(
    resolved: &ResolvedTarget,
    version_override: Option<semver::Version>,
) -> Result<ReleaseSimulation, RunnerError> {
    let context = load_release_context_via_release(&resolved.resolved_root)?;
    if !context.config.gates.is_empty() {
        emit_release_progress_line("checking release gates for simulation");
    }
    let gate_report = run_release_gates(&resolved.resolved_root, &context.config.gates, true);
    let prepare_plan = build_release_prepare_plan_via_release(
        &context,
        true,
        gate_report.clone(),
        version_override,
    )?;
    Ok(collect_release_simulation_via_release(
        &resolved.resolved_root,
        RELEASE_PREPARED_STATE_FILE,
        prepare_plan,
        &gate_report,
    ))
}

pub(super) fn execute_release_prepare(
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
    .map_err(Into::into)
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

pub(super) fn collect_release_execute_plan(
    resolved: &ResolvedTarget,
    allow_stale: bool,
) -> Result<ReleaseExecutePlan, RunnerError> {
    collect_release_execute_plan_via_release(
        resolved.resolved_root.clone(),
        RELEASE_PREPARED_STATE_FILE,
        RELEASE_STATE_STALE_THRESHOLD_SECS,
        allow_stale,
    )
    .map_err(Into::into)
}

pub(super) fn execute_release(
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
