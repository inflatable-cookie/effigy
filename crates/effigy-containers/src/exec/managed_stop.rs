//! End managed exec groups using current label ownership and a full runtime ID.
//! Teardown must not require exec readiness, published ports, or an inspect
//! snapshot of a container that may already have disappeared.

use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use super::colima_runtime::run_runtime_command_capture_for_policy_with_absolute_deadline;
use super::implementation::{owned_service_listing_args, ContainerExecError, OwnershipContext};
use crate::EffectiveContainerPolicy;

/// Teardown authority comes from current labels for every project container,
/// including stopped services. An empty listing is an already-gone stack.
pub fn verify_owned_stack_for_shutdown(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerExecError> {
    let ownership = OwnershipContext::resolve(repo_root, policy)?;
    let args = ["ps".to_owned(), "--all".to_owned(), "--no-trunc".to_owned(),
        "--filter".to_owned(), format!("label=com.docker.compose.project={}", policy.project_name),
        "--format".to_owned(),
        "{{.ID}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.effigy.checkout\"}}\t{{.Label \"com.effigy.scope\"}}".to_owned(),
    ].into_iter().map(OsString::from).collect::<Vec<_>>();
    let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &args,
        "list managed shutdown containers",
        Instant::now() + Duration::from_secs(15),
        Duration::from_secs(15),
    )?;
    ownership.verify_shutdown_listing(policy, &output.stdout)
}

pub fn end_owned_container_process_group(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    pgid: u32,
) -> Result<(), ContainerExecError> {
    if pgid <= 1 {
        return Err(ContainerExecError::Failure {
            command: "end managed exec group".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: format!("invalid managed exec process group {pgid}"),
        });
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let ownership = OwnershipContext::resolve(repo_root, policy)?;
    let current_id = || -> Result<Option<String>, ContainerExecError> {
        let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
            repo_root,
            policy,
            &owned_service_listing_args(policy, &policy.primary_service),
            "list managed exec container",
            deadline,
            Duration::from_secs(15),
        )?;
        if output.stdout.iter().all(u8::is_ascii_whitespace) {
            return Ok(None);
        }
        ownership
            .verify_listed_service(&policy.primary_service, &output.stdout)
            .map(Some)
    };
    let Some(id) = current_id()? else {
        // No running primary means its exec processes have already ended.
        return Ok(());
    };
    let args = [
        "exec".to_owned(),
        "--user".to_owned(),
        "0".to_owned(),
        id.clone(),
        "sh".to_owned(),
        "-lc".to_owned(),
        group_stop_command(pgid),
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    match run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &args,
        "end managed exec group",
        deadline,
        Duration::from_secs(15),
    ) {
        Ok(_) => Ok(()),
        Err(error) => {
            // A concurrent down may remove the proven container between ps
            // and exec. Never redirect the old PGID into its replacement.
            if current_id()?.as_deref() != Some(id.as_str()) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

fn group_stop_command(pgid: u32) -> String {
    format!(
        "alive() {{ kill -0 -- -{pgid} 2>/dev/null || kill -0 {pgid} 2>/dev/null; }}; kill -TERM -- -{pgid} 2>/dev/null || kill -TERM {pgid} 2>/dev/null || true; i=0; while [ \"$i\" -lt 12 ]; do alive || exit 0; sleep 0.25; i=$((i+1)); done; kill -KILL -- -{pgid} 2>/dev/null || kill -KILL {pgid} 2>/dev/null || true; i=0; while [ \"$i\" -lt 12 ]; do alive || exit 0; sleep 0.25; i=$((i+1)); done; printf '%s\\n' 'managed exec group {pgid} survived termination' >&2; exit 1"
    )
}
