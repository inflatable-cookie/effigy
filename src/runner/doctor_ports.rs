//! Concrete [`DoctorRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-doctor`. This module threads the
//! doctor orchestration layer's two reach-backs (health task
//! execution via `execute::run_manifest_task_with_cwd`; deferral
//! analysis via `deferral::select_deferral`) into the runner,
//! converting `RunnerError` to `DoctorError` at the port boundary so
//! the doctor layer only speaks its own error type.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_cli::TaskInvocation;
use effigy_containers::{
    colima::parse_colima_running,
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    exec::{inspect_colima_ssh_agent_socket_for_profile, SshAgentSocketHealth},
    load_all_container_policies, user_global_backend_preference, user_global_colima_profile,
};
use effigy_doctor::{
    check_id, DoctorError, DoctorFinding, DoctorRuntimeDiagnostics, DoctorRuntimePorts,
    DoctorSeverity,
};
use effigy_execution::ExecutionSurface;
use effigy_manifest::{DeferredCommand, LoadedCatalog, ManifestContainerDriver};
use effigy_tasks::TaskSelector;

use crate::runner::deferral;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::run_compose_exec;
use crate::runner::execute::api;
use crate::runner::system_command::is_primary_service_running;

#[derive(Debug, Default)]
pub(in crate::runner) struct RunnerDoctorPorts;

impl RunnerDoctorPorts {
    pub(in crate::runner) fn new() -> Self {
        Self
    }
}

impl DoctorRuntimePorts for RunnerDoctorPorts {
    fn run_manifest_task(
        &self,
        invocation: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, DoctorError> {
        api::run_manifest_task_with_surface(invocation, cwd, ExecutionSurface::DirectCli)
            .map_err(runner_to_doctor)
    }

    fn select_deferral(
        &self,
        selector: &TaskSelector,
        catalogs: &[LoadedCatalog],
        cwd: &Path,
        workspace_root: &Path,
    ) -> Option<DeferredCommand> {
        deferral::select_deferral(selector, catalogs, cwd, workspace_root)
    }

    fn runtime_diagnostics(
        &self,
        resolved_root: &Path,
    ) -> Result<DoctorRuntimeDiagnostics, DoctorError> {
        collect_runtime_diagnostics(resolved_root)
    }
}

fn runner_to_doctor(error: RunnerError) -> DoctorError {
    match error {
        RunnerError::CommandJsonFailure { rendered } => {
            DoctorError::CommandJsonFailure { rendered }
        }
        RunnerError::TaskInvocation(message) => DoctorError::TaskInvocation(message),
        RunnerError::Ui(message) => DoctorError::Ui(message),
        other => DoctorError::TaskInvocation(other.to_string()),
    }
}

fn collect_runtime_diagnostics(
    resolved_root: &Path,
) -> Result<DoctorRuntimeDiagnostics, DoctorError> {
    let mut diagnostics = DoctorRuntimeDiagnostics::default();

    // Gateway route-table trust is machine-global and independent of container
    // policies, so surface it before any container-policy early return.
    append_route_table_trust_diagnostics(&mut diagnostics);

    // Catalog-pack health is machine-global too, and matters on repos with no
    // container policy at all, so it also precedes the early returns below.
    append_catalog_pack_diagnostics(&mut diagnostics);

    let policies = match load_all_container_policies(resolved_root) {
        Ok(value) => value,
        Err(_) => return Ok(diagnostics),
    };
    if policies.is_empty() {
        return Ok(diagnostics);
    }

    let mut profiles = policies
        .iter()
        .filter(|policy| policy.driver == ManifestContainerDriver::Colima)
        .map(|policy| policy.profile.clone())
        .collect::<Vec<_>>();
    profiles.sort();
    profiles.dedup();

    if !profiles.is_empty() {
        let selected_backend = match resolve_compose_backend_for_repo(resolved_root, &policies[0]) {
            ComposeBackend::Docker => "docker-compose",
            ComposeBackend::ColimaNerdctl => "colima-nerdctl",
        };
        diagnostics.evidence.push(format!(
            "container-backend-selection: {selected_backend} (manifest driver=colima, profiles={})",
            profiles.join(", ")
        ));
        for profile in &profiles {
            match colima_profile_running(profile) {
                Ok(running) => {
                    diagnostics.evidence.push(format!(
                        "colima-profile `{profile}`: {}",
                        if running { "running" } else { "stopped" }
                    ));
                    if running {
                        append_ssh_agent_socket_warning(profile, resolved_root, &mut diagnostics);
                    }
                }
                Err(error) => diagnostics
                    .warnings
                    .push(format!("colima profile `{profile}` probe failed: {error}")),
            }
        }
    }

    let user_backend = user_global_backend_preference();
    if let Some(backend) = user_backend.clone() {
        diagnostics.evidence.push(format!(
            "user-global container backend preference: {backend}"
        ));
    }
    if let Some(profile) = user_global_colima_profile() {
        diagnostics
            .evidence
            .push(format!("user-global Colima profile preference: {profile}"));
    }

    match docker_context_name() {
        Ok(Some(context)) => {
            diagnostics
                .evidence
                .push(format!("docker-context: {context}"));
            if let Some(warning) =
                docker_context_mismatch_warning(&context, !profiles.is_empty(), user_backend)
            {
                diagnostics.warnings.push(warning);
            }
        }
        Ok(None) => {}
        Err(error) => diagnostics
            .warnings
            .push(format!("docker context probe failed: {error}")),
    }

    append_workspace_ownership_diagnostics(resolved_root, &policies, &mut diagnostics);

    Ok(diagnostics)
}

fn append_workspace_ownership_diagnostics(
    repo_root: &Path,
    policies: &[effigy_containers::EffectiveContainerPolicy],
    diagnostics: &mut DoctorRuntimeDiagnostics,
) {
    for policy in policies {
        let Some(workspace_user) = policy.workspace_user.as_deref() else {
            continue;
        };
        match is_primary_service_running(repo_root, policy) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                diagnostics.warnings.push(format!(
                    "container `{}` workspace ownership probe skipped: {error}",
                    policy.name
                ));
                continue;
            }
        }

        let script = workspace_ownership_scan_script(policy);
        let args = vec![
            OsString::from("exec"),
            OsString::from("-T"),
            OsString::from("-u"),
            OsString::from("0"),
            OsString::from(policy.primary_service.as_str()),
            OsString::from("sh"),
            OsString::from("-lc"),
            OsString::from(script),
        ];
        match run_compose_exec(repo_root, policy, &args, true, "workspace ownership probe") {
            Ok(output) if output.status.success() => {
                let root_owned = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if root_owned.is_empty() {
                    diagnostics.evidence.push(format!(
                        "container `{}` workspace ownership: clean for user `{workspace_user}`",
                        policy.name
                    ));
                } else {
                    diagnostics.findings.push(workspace_ownership_finding(
                        &policy.name,
                        workspace_user,
                        &root_owned,
                    ));
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                diagnostics.warnings.push(format!(
                    "container `{}` workspace ownership probe failed (exit={}){}",
                    policy.name,
                    output
                        .status
                        .code()
                        .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
                    if stderr.is_empty() {
                        String::new()
                    } else {
                        format!(": {stderr}")
                    }
                ));
            }
            Err(error) => diagnostics.warnings.push(format!(
                "container `{}` workspace ownership probe failed: {error}",
                policy.name
            )),
        }
    }
}

fn workspace_ownership_scan_script(policy: &effigy_containers::EffectiveContainerPolicy) -> String {
    let mut targets = policy
        .managed_volumes
        .iter()
        .filter(|volume| volume.service == policy.primary_service)
        .filter_map(|volume| volume.mount_target.as_deref())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    let quoted_targets = targets
        .iter()
        .map(|target| effigy_core::shell::shell_quote(target))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "set -- {quoted_targets}; \
         if [ -n \"${{BUN_INSTALL:-}}\" ]; then set -- \"$@\" \"$BUN_INSTALL/install\"; fi; \
         for ownership_root in \"$@\"; do \
           [ ! -e \"$ownership_root\" ] && continue; \
           root_owned=$(find \"$ownership_root\" -xdev -user root -print -quit 2>/dev/null); \
           [ -z \"$root_owned\" ] || printf '%s\\t%s\\n' \"$ownership_root\" \"$root_owned\"; \
         done"
    )
}

fn workspace_ownership_finding(
    container_name: &str,
    workspace_user: &str,
    root_owned: &[String],
) -> DoctorFinding {
    DoctorFinding {
        check_id: check_id::CONTAINER_WORKSPACE_OWNERSHIP.to_owned(),
        severity: DoctorSeverity::Warning,
        evidence: format!(
            "container `{container_name}` has root-owned paths in workspace volumes or the Bun cache, while workspace commands run as `{workspace_user}`: {}",
            root_owned.join(", ")
        ),
        remediation: format!(
            "Repair these paths to user `{workspace_user}`, then rerun `effigy doctor`. Host-routed tasks and `effigy exec` targeting the primary service must use the declared workspace user."
        ),
        fixable: false,
    }
}

/// Surface installed catalog-pack health. A pack that has become unreadable or
/// incompatible resolves to the compiled baseline silently as far as compose
/// output is concerned, so doctor is where an operator finds out — with one
/// direct repair command.
fn append_catalog_pack_diagnostics(diagnostics: &mut DoctorRuntimeDiagnostics) {
    let selection =
        effigy_catalog::pack::select_pack(crate::runner::service_command::effigy_version());
    if let Some(finding) = crate::runner::service_command::pack_health_finding(&selection) {
        diagnostics.findings.push(finding);
        return;
    }
    if let Some(record) = selection.active.as_ref() {
        diagnostics.evidence.push(format!(
            "catalog-pack: active {} {} ({})",
            record.pack_id, record.pack_version, record.install_id
        ));
    }
}

/// Surface gateway route-table trust state (contract 033) as a doctor runtime
/// diagnostic: an evidence line when trusted, a remediation warning when not.
fn append_route_table_trust_diagnostics(diagnostics: &mut DoctorRuntimeDiagnostics) {
    use effigy_gateway::server::GatewayConfig;
    use effigy_gateway::trust::{inspect_route_table_trust, RouteTableTrust};

    let Ok(gateway_dir) = crate::runner::gateway_command::gateway_dir() else {
        return;
    };
    let route_table_path = GatewayConfig::standard(gateway_dir).route_table_path;

    match inspect_route_table_trust(&route_table_path) {
        // No table yet — nothing to report.
        RouteTableTrust::Absent => {}
        RouteTableTrust::Trusted => diagnostics
            .evidence
            .push("gateway-route-table-trust: trusted".to_string()),
        RouteTableTrust::Untrusted { reason } => diagnostics.warnings.push(format!(
            "gateway route table is untrusted ({reason}); the gateway keeps its last-known-good routes. Restore owner-only permissions (no group/other write) or re-register routes with `effigy container up` to re-stamp it."
        )),
    }
}

/// Flag a stale colima SSH-agent forwarding socket for a running profile. A
/// dangling `/run/host-services/ssh-auth.sock` (host agent socket rotated on a
/// long-running VM) makes `effigy container up` fail with `mkdir ... file
/// exists`; surface it here with the `colima restart` remediation (g08.017).
fn append_ssh_agent_socket_warning(
    profile: &str,
    repo_root: &Path,
    diagnostics: &mut DoctorRuntimeDiagnostics,
) {
    let detail = match inspect_colima_ssh_agent_socket_for_profile(profile, repo_root) {
        SshAgentSocketHealth::Stale => "is stale (host SSH-agent socket rotated)",
        SshAgentSocketHealth::Absent => "is not set up",
        SshAgentSocketHealth::Healthy | SshAgentSocketHealth::Unknown => return,
    };
    diagnostics.warnings.push(format!(
        "colima profile `{profile}`: workspace SSH-agent forwarding {detail}; `effigy container \
         up` can fail with `mkdir /run/host-services/ssh-auth.sock: file exists`. \
         Fix: `colima restart {profile}`."
    ));
}

fn docker_context_mismatch_warning(
    context: &str,
    has_colima_profiles: bool,
    user_backend: Option<effigy_containers::BackendId>,
) -> Option<String> {
    if !has_colima_profiles || user_backend == Some(effigy_containers::BackendId::colima_nerdctl())
    {
        return None;
    }
    Some(format!(
        "docker CLI context is `{context}`, but Effigy will prefer Colima for declared `driver = \"colima\"` containers. If Colima should stay your machine-wide default for unscoped runtime commands too, set `[containers] backend = \"containerd\"` in `~/.effigy/config.toml`."
    ))
}

fn colima_profile_running(profile: &str) -> Result<bool, DoctorError> {
    let output = Command::new("colima")
        .args(["status", "--profile", profile])
        .output()
        .map_err(|error| {
            DoctorError::task_invocation(format!(
                "failed to launch `colima status --profile {profile}`: {error}"
            ))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_colima_running(&stdout, &stderr))
}

fn docker_context_name() -> Result<Option<String>, DoctorError> {
    let output = match Command::new("docker").args(["context", "show"]).output() {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(DoctorError::task_invocation(format!(
                "failed to launch `docker context show`: {error}"
            )));
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            return Ok(None);
        }
        return Err(DoctorError::task_invocation(format!(
            "`docker context show` failed: {stderr}"
        )));
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        docker_context_mismatch_warning, workspace_ownership_finding,
        workspace_ownership_scan_script,
    };
    use effigy_containers::BackendId;
    use effigy_doctor::{check_id, DoctorSeverity};

    use crate::runner::test_support::effective_container_policy;

    #[test]
    fn docker_context_warning_shows_when_colima_repo_has_no_pinned_containerd_preference() {
        let warning =
            docker_context_mismatch_warning("default", true, Some(BackendId::docker_compose()))
                .expect("warning");
        assert!(warning.contains("docker CLI context is `default`"));
        assert!(warning.contains("[containers] backend = \"containerd\""));
    }

    #[test]
    fn docker_context_warning_stays_hidden_when_containerd_is_already_pinned() {
        assert_eq!(
            docker_context_mismatch_warning("default", true, Some(BackendId::colima_nerdctl())),
            None
        );
    }

    #[test]
    fn docker_context_warning_stays_hidden_without_colima_profiles() {
        assert_eq!(
            docker_context_mismatch_warning("default", false, None),
            None
        );
    }

    #[test]
    fn workspace_ownership_scan_covers_primary_volumes_and_bun_cache() {
        let mut policy = effective_container_policy("web", "demo-web", "workspace", "compose.yml");
        policy.managed_volumes = vec![
            effigy_catalog::volumes::ManagedVolume {
                name: "root-node-modules".to_owned(),
                service: "workspace".to_owned(),
                persist: false,
                size_bytes: None,
                mount_point: None,
                mount_target: Some("/workspace/root/node_modules".to_owned()),
            },
            effigy_catalog::volumes::ManagedVolume {
                name: "db-data".to_owned(),
                service: "postgres".to_owned(),
                persist: true,
                size_bytes: None,
                mount_point: None,
                mount_target: Some("/var/lib/postgresql/data".to_owned()),
            },
        ];

        let script = workspace_ownership_scan_script(&policy);

        assert!(script.contains("'/workspace/root/node_modules'"));
        assert!(!script.contains("/var/lib/postgresql/data"));
        assert!(script.contains("$BUN_INSTALL/install"));
        assert!(script.contains("-user root"));
        assert!(script.contains("-print -quit"));
        assert!(script.contains("$ownership_root"));
    }

    #[test]
    fn workspace_ownership_finding_is_named_and_actionable() {
        let finding = workspace_ownership_finding(
            "workspace",
            "dev",
            &["/workspace/root/node_modules/pkg".to_owned()],
        );

        assert_eq!(finding.check_id, check_id::CONTAINER_WORKSPACE_OWNERSHIP);
        assert_eq!(finding.severity, DoctorSeverity::Warning);
        assert!(finding.evidence.contains("node_modules/pkg"));
        assert!(finding.remediation.contains("user `dev`"));
        assert!(finding.remediation.contains("effigy doctor"));
    }
}
