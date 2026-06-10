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

use super::process::run_command_capture_allow_failure;
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
    match run_command_capture_allow_failure(repo_root, "colima", &args) {
        Ok(output) if output.status.success() => {
            classify_ssh_agent_probe_output(&String::from_utf8_lossy(&output.stdout))
        }
        _ => SshAgentSocketHealth::Unknown,
    }
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
    fn probe_script_targets_the_forwarded_socket_and_emits_one_token() {
        let script = ssh_agent_socket_probe_script();
        assert!(script.contains(FORWARDED_SSH_AGENT_SOCKET));
        assert!(script.contains("readlink -f"));
        assert!(script.contains("-S"));
        for token in ["healthy", "stale", "absent"] {
            assert!(script.contains(token), "probe must emit `{token}`");
        }
    }
}
