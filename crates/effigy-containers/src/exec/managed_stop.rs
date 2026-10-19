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

/// Each signal tries the group with `--` (bash), then without it: dash and
/// busybox read `--` after `-SIG` as an illegal pid, which would otherwise
/// fall through to the leader alone and leave the group running.
fn group_stop_command(pgid: u32) -> String {
    let signal = |name: &str| {
        format!(
            "kill -{name} -- -{pgid} 2>/dev/null || kill -{name} -{pgid} 2>/dev/null || kill -{name} {pgid} 2>/dev/null"
        )
    };
    let alive = signal("0");
    let term = signal("TERM");
    let kill = signal("KILL");
    format!(
        "alive() {{ {alive}; }}; {term} || true; i=0; while [ \"$i\" -lt 12 ]; do alive || exit 0; sleep 0.25; i=$((i+1)); done; {kill} || true; i=0; while [ \"$i\" -lt 12 ]; do alive || exit 0; sleep 0.25; i=$((i+1)); done; printf '%s\\n' 'managed exec group {pgid} survived termination' >&2; exit 1"
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::group_stop_command;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn alive(pid: i32) -> bool {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
    }

    /// The recorded leader has exited and its group lives on: the stop must
    /// reach the group under every shell a container may run, not just bash.
    #[test]
    fn group_stop_ends_a_leaderless_group_under_dash_and_sh() {
        for shell in ["/bin/sh", "/bin/dash"] {
            if !std::path::Path::new(shell).exists() {
                continue;
            }
            let output = Command::new("/bin/sh")
                .args(["-c", "sleep 60 >/dev/null 2>&1 & echo $!; sleep 60 >/dev/null 2>&1 & echo $!; echo $$"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .process_group(0)
                .output()
                .expect("start group");
            let pids: Vec<i32> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.trim().parse().ok())
                .collect();
            let (members, leader) = (&pids[..2], pids[2] as u32);
            assert!(
                members.iter().all(|pid| alive(*pid)),
                "{shell}: group started"
            );
            let stop = Command::new(shell)
                .args(["-c", &group_stop_command(leader)])
                .output()
                .expect("run stop script");
            let deadline = Instant::now() + Duration::from_secs(5);
            while members.iter().any(|pid| alive(*pid)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            for pid in members {
                if alive(*pid) {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(*pid),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                }
            }
            assert!(
                stop.status.success(),
                "{shell}: {}",
                String::from_utf8_lossy(&stop.stderr)
            );
            assert!(
                members.iter().all(|pid| !alive(*pid)),
                "{shell}: the group survived its stop"
            );
        }
    }
}
