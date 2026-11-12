use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_rhai::RhaiSecretPolicy;
use effigy_rhai::RhaiSecretTarget;
use serde_json::Value;

use super::report::{validate_provider_phase_report, DeployProviderPhaseReport};
use super::DeployProviderPackage;
use crate::runner::error::RunnerError;
use crate::runner::script_command::execute_repo_rhai_script_with_secret_policy;

pub(in crate::runner::deploy_command) fn run_provider_preflight(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    context: Value,
) -> Result<Option<DeployProviderPhaseReport>, RunnerError> {
    let Some(script) = &package.descriptor.capabilities.preflight else {
        return Ok(None);
    };
    run_provider_phase(
        repo_root,
        provider_name,
        package,
        "preflight",
        script,
        context,
        true,
    )
}

pub(in crate::runner::deploy_command) fn run_provider_status(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    context: Value,
) -> Result<Option<DeployProviderPhaseReport>, RunnerError> {
    let Some(script) = &package.descriptor.capabilities.status else {
        return Ok(None);
    };
    run_provider_phase(
        repo_root,
        provider_name,
        package,
        "status",
        script,
        context,
        false,
    )
}

pub(in crate::runner::deploy_command) fn run_provider_export(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    context: Value,
    plan: bool,
) -> Result<DeployProviderPhaseReport, RunnerError> {
    let Some(script) = &package.descriptor.capabilities.export else {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` package does not declare an export capability"
        )));
    };
    run_provider_phase(
        repo_root,
        provider_name,
        package,
        "export",
        script,
        context,
        plan,
    )?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` export capability did not produce a report"
        ))
    })
}

/// `plan` phases are previews: they run with no secret access at all, so a
/// provider script that asks for a secret fails instead of reaching the vault.
fn run_provider_phase(
    repo_root: &Path,
    provider_name: &str,
    package: &DeployProviderPackage,
    phase: &str,
    script: &str,
    context: Value,
    plan: bool,
) -> Result<Option<DeployProviderPhaseReport>, RunnerError> {
    let _guard = provider_script_env_lock().lock().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to lock deploy provider script environment: {error}"
        ))
    })?;
    let workspace = provider_phase_workspace(repo_root, provider_name, phase)?;
    let refusal = plan_mode_secret_refusal(provider_name, phase);
    let secret_policy = if plan {
        RhaiSecretPolicy::Refuse(&refusal)
    } else {
        RhaiSecretPolicy::Targets(&[RhaiSecretTarget::Deploy])
    };
    let context_path = workspace.path.join("context.json");
    let report_path = workspace.path.join("report.json");
    write_provider_json(&context_path, &context)?;
    let _env_guard = ScopedProviderEnvOverride::set(&[
        (
            "EFFIGY_DEPLOY_PROVIDER_CONTEXT",
            context_path.display().to_string(),
        ),
        (
            "EFFIGY_DEPLOY_PROVIDER_REPORT",
            report_path.display().to_string(),
        ),
    ]);
    execute_repo_rhai_script_with_secret_policy(
        repo_root,
        &format!("deploy-provider:{provider_name}:{phase}"),
        &package.root.join(script),
        &[],
        secret_policy,
    )?;
    if !report_path.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` {phase} script did not write a provider report"
        )));
    }
    let raw = std::fs::read_to_string(&report_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read deploy provider `{provider_name}` {phase} report {}: {error}",
            report_path.display()
        ))
    })?;
    let report = serde_json::from_str::<DeployProviderPhaseReport>(&raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse deploy provider `{provider_name}` {phase} report: {error}"
        ))
    })?;
    validate_provider_phase_report(provider_name, phase, &report)?;
    Ok(Some(report))
}

fn provider_script_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// A provider phase's context/report directory. It lives under the checkout's
/// v0.15 state root, not the retired `.effigy/runtime/` tree, and the guard
/// removes it when the phase ends, whatever the outcome.
struct ProviderPhaseWorkspace {
    path: PathBuf,
}

impl Drop for ProviderPhaseWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Leftovers exist only when a previous process died mid-phase. The sweep is
/// bounded like other per-checkout cleanup and only removes Effigy's own
/// `<millis>-<phase>` directories, never anything else under the provider root.
const PROVIDER_PHASE_RETENTION: std::time::Duration = std::time::Duration::from_secs(3600);
const PROVIDER_PHASE_SWEEP_LIMIT: usize = 32;

fn provider_phase_workspace(
    repo_root: &Path,
    provider_name: &str,
    phase: &str,
) -> Result<ProviderPhaseWorkspace, RunnerError> {
    let provider_dir = repo_root
        .join(".effigy/state/deploy/provider")
        .join(provider_name);
    sweep_stale_provider_phases(&provider_dir);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let path = provider_dir.join(format!("{millis}-{phase}"));
    std::fs::create_dir_all(&path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create deploy provider `{provider_name}` {phase} workspace {}: {error}",
            path.display()
        ))
    })?;
    Ok(ProviderPhaseWorkspace { path })
}

fn sweep_stale_provider_phases(provider_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(provider_dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        if removed >= PROVIDER_PHASE_SWEEP_LIMIT {
            return;
        }
        if !is_provider_phase_workspace_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let stale = now
            .duration_since(modified)
            .map(|age| age >= PROVIDER_PHASE_RETENTION)
            .unwrap_or(false);
        if stale && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
}

fn is_provider_phase_workspace_name(name: &str) -> bool {
    match name.split_once('-') {
        Some((millis, phase)) => {
            !millis.is_empty()
                && millis.bytes().all(|byte| byte.is_ascii_digit())
                && matches!(
                    phase,
                    "preflight" | "export" | "validate" | "status" | "apply"
                )
        }
        None => false,
    }
}

/// Plan-mode phases receive no secrets. The refusal names the phase and the
/// requested secret; the vault and manifest are never read.
fn plan_mode_secret_refusal(provider_name: &str, phase: &str) -> String {
    format!(
        "deploy provider `{provider_name}` {phase} phase runs in plan mode and receives no secret values"
    )
}

fn write_provider_json(path: &Path, value: &Value) -> Result<(), RunnerError> {
    let raw = serde_json::to_string_pretty(value).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode deploy provider context: {error}"))
    })?;
    std::fs::write(path, raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write deploy provider context {}: {error}",
            path.display()
        ))
    })
}

struct ScopedProviderEnvOverride {
    previous: Vec<(&'static str, Option<String>)>,
}

impl ScopedProviderEnvOverride {
    fn set(values: &[(&'static str, String)]) -> Self {
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in values {
            std::env::set_var(key, value);
        }
        Self { previous }
    }
}

impl Drop for ScopedProviderEnvOverride {
    fn drop(&mut self) {
        for (key, value) in self.previous.iter().rev() {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_provider_phase_workspace_name, PROVIDER_PHASE_SWEEP_LIMIT};

    #[test]
    fn the_sweep_only_removes_effigys_own_phase_workspaces() {
        assert!(is_provider_phase_workspace_name("1700000000000-export"));
        assert!(is_provider_phase_workspace_name("1-preflight"));
        assert!(is_provider_phase_workspace_name("1700000000000-status"));
        // An operator's own directory, a phase name Effigy never writes, and a
        // non-numeric prefix all stay untouched.
        assert!(!is_provider_phase_workspace_name("notes"));
        assert!(!is_provider_phase_workspace_name("1700000000000-anything"));
        assert!(!is_provider_phase_workspace_name("millis-export"));
        assert!(!is_provider_phase_workspace_name("1700000000000"));
        assert_eq!(PROVIDER_PHASE_SWEEP_LIMIT, 32);
    }
}
