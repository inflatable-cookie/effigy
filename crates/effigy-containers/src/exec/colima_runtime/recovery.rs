//! Explicit recovery and reset of an owned Colima profile.

use super::*;

/// One phase of a Colima runtime recovery or reset, reported while the work
/// runs so the owning command plane names the command actually in flight
/// (Q-017). The recovery used to report nothing, so a plane could only guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColimaRecoveryPhase {
    /// `colima stop` for the profile.
    Stopping,
    /// Terminating the profile's leftover processes from its pidfiles.
    TerminatingProcesses,
    /// `colima status` proving the profile is stopped.
    VerifyingStopped,
    /// Creating the profile directories and reading, parsing and writing its
    /// managed configuration, before `colima start` can run.
    Configuring,
    /// `colima start` for the profile.
    Restarting,
    /// `colima status` proving the restarted profile is healthy.
    VerifyingHealth,
}

/// Reports each recovery phase as it starts.
pub type ColimaRecoveryProgress<'a> = &'a mut dyn FnMut(ColimaRecoveryPhase);

pub fn recover_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    recover_colima_runtime_with_progress(policy, repo_root, &mut |_| {})
}

/// Recover the profile, reporting the phase now in flight to `progress`.
pub fn recover_colima_runtime_with_progress(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    progress: ColimaRecoveryProgress<'_>,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    let mut steps = vec![format!(
        "[check] recovering Colima profile `{}`",
        policy.profile
    )];

    progress(ColimaRecoveryPhase::Stopping);
    let graceful_ok = match stop_colima_profile_for_repair(policy, repo_root) {
        Ok(_) => {
            steps.push(format!("[ok] stopped Colima profile `{}`", policy.profile));
            true
        }
        Err(error) => {
            steps.push(format!(
                "[warning] graceful stop failed for profile `{}`; inspecting profile pidfiles\n{}",
                policy.profile, error
            ));
            false
        }
    };

    progress(ColimaRecoveryPhase::TerminatingProcesses);
    force_terminate_colima_profile_processes(policy, graceful_ok, &mut steps)?;

    progress(ColimaRecoveryPhase::VerifyingStopped);
    verify_profile_stopped(policy, repo_root, &steps, true)?;

    if let Err(error) = restart_and_verify_colima_profile(policy, repo_root, &mut steps, progress) {
        let runtime_state_loss = match &error {
            ContainerExecError::Failure { stdout, stderr, .. } => {
                docker_failure_looks_like_colima_runtime_state_loss(stdout, stderr)
            }
            ContainerExecError::Launch { .. } | ContainerExecError::Refusal { .. } => false,
        };
        if !runtime_state_loss {
            return Err(error);
        }

        steps.push(format!(
            "[warning] profile `{}` still reported an empty runtime after restart; retrying one deeper repair pass",
            policy.profile
        ));
        progress(ColimaRecoveryPhase::Stopping);
        let stopped = match stop_colima_profile_for_repair(policy, repo_root) {
            Ok(()) => true,
            Err(error) => {
                steps.push(format!("[warning] retry graceful stop failed: {error}"));
                false
            }
        };
        progress(ColimaRecoveryPhase::TerminatingProcesses);
        force_terminate_colima_profile_processes(policy, stopped, &mut steps)?;
        progress(ColimaRecoveryPhase::VerifyingStopped);
        verify_profile_stopped(policy, repo_root, &steps, true)?;
        restart_and_verify_colima_profile(policy, repo_root, &mut steps, progress)?;
    }

    Ok(ColimaRecoveryReport {
        profile: policy.profile.clone(),
        steps,
    })
}

pub fn reset_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    reset_colima_runtime_with_progress(policy, repo_root, &mut |_| {})
}

/// Reset the profile to a fully stopped state, reporting each phase in flight.
pub fn reset_colima_runtime_with_progress(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    progress: ColimaRecoveryProgress<'_>,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    let mut steps = vec![format!(
        "[check] resetting Colima profile `{}` to a fully stopped state",
        policy.profile
    )];

    progress(ColimaRecoveryPhase::Stopping);
    let graceful_ok = match stop_colima_profile_for_repair(policy, repo_root) {
        Ok(_) => {
            steps.push(format!("[ok] stopped Colima profile `{}`", policy.profile));
            true
        }
        Err(error) => {
            steps.push(format!(
                "[warning] graceful stop failed for profile `{}`; inspecting profile pidfiles\n{}",
                policy.profile, error
            ));
            false
        }
    };

    progress(ColimaRecoveryPhase::TerminatingProcesses);
    force_terminate_colima_profile_processes(policy, graceful_ok, &mut steps)?;
    progress(ColimaRecoveryPhase::VerifyingStopped);
    verify_profile_stopped(policy, repo_root, &steps, false)?;
    steps.push(format!(
        "[ok] verified Colima profile `{}` is fully stopped",
        policy.profile
    ));

    Ok(ColimaRecoveryReport {
        profile: policy.profile.clone(),
        steps,
    })
}

fn restart_and_verify_colima_profile(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    steps: &mut Vec<String>,
    progress: ColimaRecoveryProgress<'_>,
) -> Result<(), ContainerExecError> {
    // Preparation reads, parses and writes the profile's managed configuration;
    // `colima start` has not begun while it runs.
    progress(ColimaRecoveryPhase::Configuring);
    prepare_managed_colima_profile(policy).map_err(|error| ContainerExecError::Failure {
        command: "colima profile config".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error,
    })?;
    progress(ColimaRecoveryPhase::Restarting);
    let start = colima_start_command(policy);
    let start_args: Vec<&str> = start.args.iter().map(|value| value.as_str()).collect();
    run_command_capture_with_timeout(
        repo_root,
        &start.program,
        &start_args,
        &start.label,
        COLIMA_START_TIMEOUT,
    )?;
    steps.push(format!("[ok] started Colima profile `{}`", policy.profile));

    progress(ColimaRecoveryPhase::VerifyingHealth);
    let status = colima_status_command(policy);
    let status_args: Vec<&str> = status.args.iter().map(|value| value.as_str()).collect();
    let output = run_command_capture_with_timeout(
        repo_root,
        &status.program,
        &status_args,
        &status.label,
        COLIMA_STATUS_TIMEOUT,
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !parse_colima_running(&stdout, &stderr) {
        return Err(ContainerExecError::Failure {
            command: status.label,
            code: output.status.code(),
            stdout: stdout.into_owned(),
            stderr: stderr.into_owned(),
        });
    }
    steps.push(format!(
        "[ok] verified Colima profile `{}` is healthy",
        policy.profile
    ));
    Ok(())
}

fn stop_colima_profile_for_repair(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(), ContainerExecError> {
    let stop = colima_stop_command(policy);
    let stop_args: Vec<&str> = stop.args.iter().map(|value| value.as_str()).collect();
    match run_command_capture_with_timeout(
        repo_root,
        &stop.program,
        &stop_args,
        &stop.label,
        COLIMA_STOP_TIMEOUT,
    ) {
        Ok(output) => Ok(output).map(|_| ()),
        Err(error) if stop_timeout_can_be_accepted(policy, repo_root, &error)? => Ok(()),
        Err(error) => Err(error),
    }
}

fn stop_timeout_can_be_accepted(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    error: &ContainerExecError,
) -> Result<bool, ContainerExecError> {
    if !error_is_timeout(error) {
        return Ok(false);
    }
    let deadline = Instant::now() + COLIMA_STOP_SETTLE_TIMEOUT;
    loop {
        match colima_running_probe_after_stop(policy, repo_root)? {
            Some(true) if Instant::now() < deadline => thread::sleep(Duration::from_millis(250)),
            Some(true) => return Ok(false),
            Some(false) | None => return Ok(true),
        }
    }
}

fn colima_running_probe_after_stop(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<Option<bool>, ContainerExecError> {
    let status = colima_status_command(policy);
    let args: Vec<&str> = status.args.iter().map(|value| value.as_str()).collect();
    let output = run_command_capture_allow_failure_with_deadline(
        repo_root,
        &status.program,
        &args,
        &status.label,
        Some(Instant::now() + COLIMA_STATUS_TIMEOUT),
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if docker_failure_looks_like_colima_runtime_state_loss(&stdout, &stderr) {
        return Ok(None);
    }
    if output.status.success() && parse_colima_running(&stdout, &stderr) {
        return Ok(Some(true));
    }
    if format!("{stdout}\n{stderr}")
        .to_ascii_lowercase()
        .contains("not running")
    {
        return Ok(Some(false));
    }
    Err(ContainerExecError::Failure {
        command: status.label,
        code: output.status.code(),
        stdout: stdout.into_owned(),
        stderr: format!("Colima stop status is unproven: {stderr}"),
    })
}

fn verify_profile_stopped(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    steps: &[String],
    allow_empty_runtime: bool,
) -> Result<(), ContainerExecError> {
    let reason = match colima_running_probe_after_stop(policy, repo_root) {
        Ok(Some(false)) => return Ok(()),
        Ok(None) if allow_empty_runtime => return Ok(()),
        Ok(Some(true)) => "profile is still running".to_owned(),
        Ok(None) => "profile status is unavailable".to_owned(),
        Err(error) => error.to_string(),
    };
    Err(ContainerExecError::Failure {
        command: "colima status".to_owned(),
        code: None,
        stdout: steps.join("\n"),
        stderr: format!("{reason}; refusing profile recovery. Verify pidfile ownership before manually running `colima stop -p {} --force`; Effigy did not run it.", policy.profile),
    })
}

fn force_terminate_colima_profile_processes(
    policy: &EffectiveContainerPolicy,
    graceful_ok: bool,
    steps: &mut Vec<String>,
) -> Result<(), ContainerExecError> {
    let stopped =
        crate::exec::colima_cleanup::terminate(&policy.profile, graceful_ok).map_err(|error| {
            ContainerExecError::Failure {
                command: "Colima profile pidfile cleanup".to_owned(),
                code: None,
                stdout: steps.join("\n"),
                stderr: error,
            }
        })?;
    steps.extend(stopped);
    Ok(())
}

