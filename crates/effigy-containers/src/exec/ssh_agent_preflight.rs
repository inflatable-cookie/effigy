//! Preflight for the colima-forwarded host SSH-agent socket.
//!
//! Colima's `--ssh-agent` flag forwards the host SSH agent to
//! `/run/host-services/ssh-auth.sock` inside the VM (as a symlink to the
//! forwarded socket). On a long-running VM the host agent socket can rotate,
//! leaving that symlink dangling. nerdctl then crashes the workspace container
//! at mount setup (`mkdir "/run/host-services/ssh-auth.sock": file exists`),
//! because it cannot bind-mount a dangling-symlink source.
//!
//! The socket lives inside the VM, not on the host where Effigy runs, so the
//! check is performed VM-side via `colima ssh`. Callers use the verdict to warn
//! pre-emptively at `container up` and to surface a `doctor` finding, both
//! naming the `colima restart <profile>` remediation (g08.017).

use std::path::Path;
use std::time::Instant;

use super::process::run_command_capture_allow_failure_with_deadline;
use crate::EffectiveContainerPolicy;

/// Path inside the colima VM where colima forwards the host SSH-agent socket.
pub(crate) const FORWARDED_SSH_AGENT_SOCKET: &str = "/run/host-services/ssh-auth.sock";

/// Health of the forwarded SSH-agent socket inside the colima VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshAgentSocketHealth {
    /// Source resolves to a live socket — the bind mount will work.
    Healthy,
    /// Source exists but its target is missing or not a socket: stale
    /// forwarding. Mounting it crashes the container runtime.
    Stale,
    /// Source path is absent entirely (forwarding not set up).
    Absent,
    /// Could not determine (probe failed, non-colima backend, or unsupported);
    /// callers must not block on this verdict.
    Unknown,
}

impl SshAgentSocketHealth {
    /// True when the socket is safe to bind-mount (healthy, or undetermined so
    /// we fall back to current behavior rather than blocking).
    pub fn is_mountable(self) -> bool {
        matches!(self, Self::Healthy | Self::Unknown)
    }

    /// True when the socket is known-bad (stale or absent forwarding).
    pub fn is_broken(self) -> bool {
        matches!(self, Self::Stale | Self::Absent)
    }
}

/// Shell probe executed inside the VM. Prints exactly one of
/// `healthy` / `stale` / `absent` so the host side can classify the result
/// without depending on exit codes.
pub(crate) fn ssh_agent_socket_probe_script() -> String {
    format!(
        "src={FORWARDED_SSH_AGENT_SOCKET}; \
         if [ ! -e \"$src\" ] && [ ! -L \"$src\" ]; then echo absent; exit 0; fi; \
         tgt=$(readlink -f \"$src\" 2>/dev/null || true); \
         if [ -n \"$tgt\" ] && [ -S \"$tgt\" ]; then echo healthy; else echo stale; fi"
    )
}

/// Map the probe's stdout to a health verdict.
pub(crate) fn classify_ssh_agent_probe_output(stdout: &str) -> SshAgentSocketHealth {
    match stdout.trim() {
        "healthy" => SshAgentSocketHealth::Healthy,
        "stale" => SshAgentSocketHealth::Stale,
        "absent" => SshAgentSocketHealth::Absent,
        _ => SshAgentSocketHealth::Unknown,
    }
}

/// Probe the forwarded SSH-agent socket inside the colima VM for the policy's
/// profile. Returns [`SshAgentSocketHealth::Unknown`] on any probe failure so a
/// flaky check never blocks bring-up.
pub fn inspect_colima_ssh_agent_socket(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> SshAgentSocketHealth {
    inspect_colima_ssh_agent_socket_for_profile(&policy.profile, repo_root)
}

/// Probe the forwarded SSH-agent socket for a named colima profile. The caller
/// is responsible for only probing a profile that is running (`colima ssh`
/// against a stopped profile fails and yields [`SshAgentSocketHealth::Unknown`]).
pub fn inspect_colima_ssh_agent_socket_for_profile(
    profile: &str,
    repo_root: &Path,
) -> SshAgentSocketHealth {
    inspect_colima_ssh_agent_socket_for_profile_with_deadline(profile, repo_root, None)
}

/// Deadline-aware probe of the forwarded SSH-agent socket. A bounded (doctor)
/// caller shares one monotonic deadline; an expired deadline never spawns and a
/// timeout resolves to [`SshAgentSocketHealth::Unknown`], which never blocks and
/// is never mistaken for a healthy socket.
pub fn inspect_colima_ssh_agent_socket_for_profile_with_deadline(
    profile: &str,
    repo_root: &Path,
    deadline: Option<Instant>,
) -> SshAgentSocketHealth {
    let probe = ssh_agent_socket_probe_script();
    let args = [
        "ssh",
        "--profile",
        profile,
        "--",
        "sh",
        "-c",
        probe.as_str(),
    ];
    match run_command_capture_allow_failure_with_deadline(
        repo_root,
        "colima",
        &args,
        "colima SSH-agent socket probe",
        deadline,
    ) {
        Ok(output) if output.status.success() => {
            classify_ssh_agent_probe_output(&String::from_utf8_lossy(&output.stdout))
        }
        _ => SshAgentSocketHealth::Unknown,
    }
}

/// Where Colima's forwarder places the host agent socket inside the VM. A repair
/// links the forwarded path to one live socket matching this glob.
const GUEST_FORWARDED_AGENT_GLOB: &str = "/tmp/ssh-*/agent.*";

/// Outcome of repairing the forwarded SSH-agent symlink inside a running VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshAgentSymlinkRepair {
    /// The forwarded path was absent or a dangling symlink and now points at a
    /// live guest agent socket.
    Repaired { source: String },
    /// The forwarded path already holds a live socket or a non-symlink, which
    /// is never overwritten.
    Untouched,
    /// No live forwarded agent socket exists in the guest to link to.
    NoLiveAgent,
    /// The link was made but does not resolve to a socket, or the output was
    /// not a recognised verdict.
    Failed,
}

/// Repair script run inside the VM. It prints exactly one verdict line and only
/// replaces an absent path or a dangling symlink. `privilege` prefixes the
/// root-only commands: `sudo ` inside the VM, empty for fixture runs.
fn ssh_agent_symlink_repair_script(forwarded: &str, candidates: &str, privilege: &str) -> String {
    format!(
        "src={forwarded}; \
         if [ -S \"$src\" ]; then echo untouched; exit 0; fi; \
         if [ -e \"$src\" ] && [ ! -L \"$src\" ]; then echo untouched; exit 0; fi; \
         live=; for s in {candidates}; do if [ -S \"$s\" ]; then live=$s; break; fi; done; \
         if [ -z \"$live\" ]; then echo no-live-agent; exit 0; fi; \
         {privilege}mkdir -p \"$(dirname \"$src\")\" && {privilege}ln -sfn \"$live\" \"$src\" && [ -S \"$src\" ] && echo \"repaired $live\" || echo failed"
    )
}

/// Map the repair script's stdout to its verdict. Anything unrecognised is a
/// failed repair, never a success.
pub(crate) fn classify_ssh_agent_repair_output(stdout: &str) -> SshAgentSymlinkRepair {
    let line = stdout.trim();
    if let Some(source) = line.strip_prefix("repaired ") {
        return SshAgentSymlinkRepair::Repaired {
            source: source.to_owned(),
        };
    }
    match line {
        "untouched" => SshAgentSymlinkRepair::Untouched,
        "no-live-agent" => SshAgentSymlinkRepair::NoLiveAgent,
        _ => SshAgentSymlinkRepair::Failed,
    }
}

/// Repair the forwarded SSH-agent symlink inside a running colima VM without
/// stopping or restarting it. Runs through `colima ssh`, so it is one bounded
/// guest command. An `Err` means the command itself could not be run.
pub fn repair_colima_ssh_agent_symlink(
    profile: &str,
    repo_root: &Path,
) -> Result<SshAgentSymlinkRepair, String> {
    repair_colima_ssh_agent_symlink_with(
        "colima",
        profile,
        repo_root,
        FORWARDED_SSH_AGENT_SOCKET,
        GUEST_FORWARDED_AGENT_GLOB,
        "sudo ",
    )
}

fn repair_colima_ssh_agent_symlink_with(
    program: &str,
    profile: &str,
    repo_root: &Path,
    forwarded: &str,
    candidates: &str,
    privilege: &str,
) -> Result<SshAgentSymlinkRepair, String> {
    let script = ssh_agent_symlink_repair_script(forwarded, candidates, privilege);
    let args = [
        "ssh",
        "--profile",
        profile,
        "--",
        "sh",
        "-c",
        script.as_str(),
    ];
    let output = run_command_capture_allow_failure_with_deadline(
        repo_root,
        program,
        &args,
        "colima SSH-agent symlink repair",
        None,
    )
    .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "colima ssh exited unsuccessfully: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(classify_ssh_agent_repair_output(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_probe_output() {
        assert_eq!(
            classify_ssh_agent_probe_output("healthy\n"),
            SshAgentSocketHealth::Healthy
        );
        assert_eq!(
            classify_ssh_agent_probe_output("  stale "),
            SshAgentSocketHealth::Stale
        );
        assert_eq!(
            classify_ssh_agent_probe_output("absent\n"),
            SshAgentSocketHealth::Absent
        );
        assert_eq!(
            classify_ssh_agent_probe_output(""),
            SshAgentSocketHealth::Unknown
        );
        assert_eq!(
            classify_ssh_agent_probe_output("garbage"),
            SshAgentSocketHealth::Unknown
        );
    }

    #[test]
    fn mountability_and_broken_partition_the_verdicts() {
        assert!(SshAgentSocketHealth::Healthy.is_mountable());
        assert!(SshAgentSocketHealth::Unknown.is_mountable());
        assert!(!SshAgentSocketHealth::Stale.is_mountable());
        assert!(!SshAgentSocketHealth::Absent.is_mountable());

        assert!(SshAgentSocketHealth::Stale.is_broken());
        assert!(SshAgentSocketHealth::Absent.is_broken());
        assert!(!SshAgentSocketHealth::Healthy.is_broken());
        assert!(!SshAgentSocketHealth::Unknown.is_broken());
    }

    #[test]
    fn probe_script_reports_live_stale_and_absent_sockets() {
        use std::os::unix::net::UnixListener;
        let root = tempfile::tempdir().expect("probe fixture");
        // Run the VM probe with its socket path redirected at the fixture;
        // a script that stopped targeting the forwarded socket probes the
        // real (absent) path and fails the healthy case below.
        let run_probe = |source: &std::path::Path| {
            let script = ssh_agent_socket_probe_script()
                .replace(FORWARDED_SSH_AGENT_SOCKET, &source.to_string_lossy());
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .output()
                .expect("probe script runs");
            assert!(output.status.success(), "probe exits zero: {output:?}");
            classify_ssh_agent_probe_output(&String::from_utf8_lossy(&output.stdout))
        };
        // Healthy: the source resolves to a live socket.
        let healthy = root.path().join("agent.sock");
        let _listener = UnixListener::bind(&healthy).expect("live socket");
        assert_eq!(run_probe(&healthy), SshAgentSocketHealth::Healthy);
        // Stale: a dangling symlink where the forwarded socket was.
        let stale = root.path().join("stale.sock");
        std::os::unix::fs::symlink(root.path().join("gone.sock"), &stale).expect("dangling link");
        assert_eq!(run_probe(&stale), SshAgentSocketHealth::Stale);
        // Absent: nothing at the forwarded path.
        assert_eq!(
            run_probe(&root.path().join("missing.sock")),
            SshAgentSocketHealth::Absent
        );
    }

    /// A fake `colima` that records each invocation and runs only the guest
    /// command it is given, so the repair runs against a fixture, not a VM.
    fn fake_colima(root: &Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join("colima");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n[ \"$1\" = ssh ] || exit 1\nshift 4\nexec \"$@\"\n",
            root.join("calls").display()
        );
        std::fs::write(&path, script).expect("fake colima writes");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("fake colima is executable");
        path
    }

    fn run_repair(root: &Path, colima: &Path, forwarded: &Path) -> SshAgentSymlinkRepair {
        repair_colima_ssh_agent_symlink_with(
            &colima.to_string_lossy(),
            "effigy",
            root,
            &forwarded.to_string_lossy(),
            &root.join("ssh-*/agent.*").to_string_lossy(),
            "",
        )
        .expect("repair command runs")
    }

    #[test]
    fn repair_links_a_live_guest_agent_in_place_without_restarting_the_vm() {
        use std::os::unix::fs::FileTypeExt;
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        let agent_dir = root.path().join("ssh-abc");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        let live = agent_dir.join("agent.1");
        let _agent = std::os::unix::net::UnixListener::bind(&live).expect("live agent socket");
        let colima = fake_colima(root.path());

        // Absent path: the missing parent is created and the link points at the live agent.
        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Repaired {
                source: live.to_string_lossy().into_owned()
            }
        );
        assert!(std::fs::metadata(&forwarded)
            .expect("forwarded path resolves")
            .file_type()
            .is_socket());

        // Dangling symlink left by a rotated agent: relinked in place.
        std::fs::remove_file(&forwarded).expect("drop repaired link");
        std::os::unix::fs::symlink(root.path().join("gone.sock"), &forwarded)
            .expect("dangling link");
        assert!(matches!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Repaired { .. }
        ));
        assert!(std::fs::metadata(&forwarded)
            .expect("relinked path resolves")
            .file_type()
            .is_socket());

        // Only the guest `ssh` ran; the fake binary would have failed any other verb.
        let calls = std::fs::read_to_string(root.path().join("calls")).expect("call log");
        assert_eq!(calls.lines().count(), 2);
        assert!(calls
            .lines()
            .all(|line| line.starts_with("ssh --profile effigy -- sh -c")));
    }

    #[test]
    fn repair_never_replaces_a_live_socket_or_a_regular_file() {
        use std::os::unix::fs::FileTypeExt;
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        let agent_dir = root.path().join("ssh-abc");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        let _agent = std::os::unix::net::UnixListener::bind(agent_dir.join("agent.1"))
            .expect("live guest agent");
        let colima = fake_colima(root.path());

        // A live socket already at the forwarded path is left exactly as it is.
        let live_forwarded =
            std::os::unix::net::UnixListener::bind(&forwarded).expect("live forwarded socket");
        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Untouched
        );
        assert!(std::fs::symlink_metadata(&forwarded)
            .expect("forwarded path")
            .file_type()
            .is_socket());

        // A regular file at the forwarded path is not a broken link, so it stays too.
        drop(live_forwarded);
        std::fs::remove_file(&forwarded).expect("drop forwarded socket");
        std::fs::write(&forwarded, "not a socket").expect("regular file");
        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Untouched
        );
        assert!(std::fs::symlink_metadata(&forwarded)
            .expect("forwarded path")
            .file_type()
            .is_file());
    }

    #[test]
    fn repair_refuses_cleanly_without_a_live_guest_agent() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        // A stale candidate that is not a socket must never be linked to.
        std::fs::create_dir_all(root.path().join("ssh-old")).expect("stale dir");
        std::fs::write(root.path().join("ssh-old/agent.9"), "not a socket").expect("stale file");
        let colima = fake_colima(root.path());

        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::NoLiveAgent
        );
        assert!(std::fs::symlink_metadata(&forwarded).is_err());
    }

    #[test]
    fn classifies_repair_output() {
        assert_eq!(
            classify_ssh_agent_repair_output("repaired /tmp/ssh-x/agent.1\n"),
            SshAgentSymlinkRepair::Repaired {
                source: "/tmp/ssh-x/agent.1".to_owned()
            }
        );
        assert_eq!(
            classify_ssh_agent_repair_output("untouched\n"),
            SshAgentSymlinkRepair::Untouched
        );
        assert_eq!(
            classify_ssh_agent_repair_output("no-live-agent"),
            SshAgentSymlinkRepair::NoLiveAgent
        );
        assert_eq!(
            classify_ssh_agent_repair_output("failed"),
            SshAgentSymlinkRepair::Failed
        );
        assert_eq!(
            classify_ssh_agent_repair_output(""),
            SshAgentSymlinkRepair::Failed
        );
        assert_eq!(
            classify_ssh_agent_repair_output("garbage"),
            SshAgentSymlinkRepair::Failed
        );
    }
}
