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
use std::time::{Duration, Instant};

use super::colima_runtime::{colima_profile_recovery_advice, ColimaAdviceMode};
use super::process::run_command_capture_allow_failure_with_deadline;
use crate::EffectiveContainerPolicy;

/// Path inside the colima VM where colima forwards the host SSH-agent socket.
pub(crate) const FORWARDED_SSH_AGENT_SOCKET: &str = "/run/host-services/ssh-auth.sock";

/// Bounds one guest SSH-agent probe, so a hung `colima ssh` cannot stall bring-up.
const SSH_AGENT_PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Bounds the guest repair, a few root-only filesystem commands.
const SSH_AGENT_REPAIR_TIMEOUT: Duration = Duration::from_secs(30);

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
/// The probe is bounded by [`SSH_AGENT_PROBE_TIMEOUT`].
pub fn inspect_colima_ssh_agent_socket_for_profile(
    profile: &str,
    repo_root: &Path,
) -> SshAgentSocketHealth {
    inspect_colima_ssh_agent_socket_for_profile_with_deadline(
        profile,
        repo_root,
        Some(Instant::now() + SSH_AGENT_PROBE_TIMEOUT),
    )
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
    /// The forwarded path was absent or a broken symlink and now points at a
    /// live guest agent socket.
    Repaired { source: String },
    /// The forwarded path already holds a live socket. Nothing was changed.
    AlreadyLive,
    /// The forwarded path holds something that is neither a socket nor a
    /// symlink. Effigy never replaces it.
    Foreign,
    /// No live forwarded agent socket exists in the guest to link to.
    NoLiveAgent,
    /// The link was not made, or the output was not a recognised verdict.
    Failed,
}

/// Points where a concurrent writer could land between the repair's check and
/// its change. Production passes the shell no-op `:` at both points. The race
/// tests put a competing socket there to prove the repair never clobbers it.
#[derive(Debug, Clone, Copy)]
struct RepairRaceHooks<'a> {
    /// Runs after a broken symlink is seen and before it is moved aside.
    before_move: &'a str,
    /// Runs after the forwarded path is cleared and before the link is made.
    before_link: &'a str,
}

const NO_RACE_HOOKS: RepairRaceHooks<'static> = RepairRaceHooks {
    before_move: ":",
    before_link: ":",
};

/// Repair script run inside the VM. It prints exactly one verdict line.
///
/// It never overwrites what it did not create. A live socket or a regular file
/// at the forwarded path is left as it is. A broken symlink is moved aside with
/// `mv -n` and checked before removal, so a socket that appears in the window
/// is restored rather than deleted. The link itself is made with `ln -s`
/// without `-f`, which fails if anything now exists at the path. `privilege`
/// prefixes the root-only commands: `sudo ` inside the VM, empty for fixtures.
fn ssh_agent_symlink_repair_script(
    forwarded: &str,
    candidates: &str,
    privilege: &str,
    hooks: RepairRaceHooks<'_>,
) -> String {
    let RepairRaceHooks {
        before_move,
        before_link,
    } = hooks;
    format!(
        "src={forwarded}; \
         if [ -S \"$src\" ]; then echo live; exit 0; fi; \
         if [ -e \"$src\" ] && [ ! -L \"$src\" ]; then echo foreign; exit 0; fi; \
         live=; for s in {candidates}; do if [ -S \"$s\" ]; then live=$s; break; fi; done; \
         if [ -z \"$live\" ]; then echo no-live-agent; exit 0; fi; \
         {privilege}mkdir -p \"$(dirname \"$src\")\" || exit 1; \
         if [ -L \"$src\" ]; then \
           {before_move}; \
           stale=\"$src.effigy-stale.$$\"; \
           {privilege}mv -n \"$src\" \"$stale\" || exit 1; \
           if [ -L \"$stale\" ] && [ ! -S \"$stale\" ]; then \
             {privilege}rm -f \"$stale\"; \
           else \
             {privilege}mv -n \"$stale\" \"$src\"; \
             if [ -S \"$src\" ]; then echo live; else echo foreign; fi; exit 0; \
           fi; \
         fi; \
         {before_link}; \
         {privilege}ln -s \"$live\" \"$src\" && [ -S \"$src\" ] && echo \"repaired $live\" || echo failed"
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
        "live" => SshAgentSymlinkRepair::AlreadyLive,
        "foreign" => SshAgentSymlinkRepair::Foreign,
        "no-live-agent" => SshAgentSymlinkRepair::NoLiveAgent,
        _ => SshAgentSymlinkRepair::Failed,
    }
}

/// One repair invocation: the command, the paths, and its bounded time.
#[derive(Debug, Clone, Copy)]
struct RepairRun<'a> {
    program: &'a str,
    forwarded: &'a str,
    candidates: &'a str,
    privilege: &'a str,
    timeout: Duration,
    hooks: RepairRaceHooks<'a>,
}

/// Repair the forwarded SSH-agent symlink inside a running colima VM without
/// stopping or restarting it. Runs through `colima ssh`, bounded by
/// [`SSH_AGENT_REPAIR_TIMEOUT`]. An `Err` means the command could not run or
/// did not finish in time.
pub fn repair_colima_ssh_agent_symlink(
    profile: &str,
    repo_root: &Path,
) -> Result<SshAgentSymlinkRepair, String> {
    repair_colima_ssh_agent_symlink_with(
        profile,
        repo_root,
        RepairRun {
            program: "colima",
            forwarded: FORWARDED_SSH_AGENT_SOCKET,
            candidates: GUEST_FORWARDED_AGENT_GLOB,
            privilege: "sudo ",
            timeout: SSH_AGENT_REPAIR_TIMEOUT,
            hooks: NO_RACE_HOOKS,
        },
    )
}

fn repair_colima_ssh_agent_symlink_with(
    profile: &str,
    repo_root: &Path,
    run: RepairRun<'_>,
) -> Result<SshAgentSymlinkRepair, String> {
    let script =
        ssh_agent_symlink_repair_script(run.forwarded, run.candidates, run.privilege, run.hooks);
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
        run.program,
        &args,
        "colima SSH-agent symlink repair",
        Some(Instant::now() + run.timeout),
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

/// What `container up` does with the forwarded socket once the repair has run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshAgentSocketOutcome {
    /// Bring-up continues. `relinked` names the live socket when Effigy
    /// repaired the link.
    Proceed { relinked: Option<String> },
    /// Bring-up continues with this warning. The workspace may still fail to
    /// mount, but its containers are not left half-created.
    Warn(String),
    /// Bring-up stops. The dangling link would fail the workspace mount and
    /// strand its containers in `Created`.
    Refuse(String),
}

/// Decide what a broken forwarded socket means for `container up`, given the
/// probe verdict and the repair attempt. Only a dangling link that cannot be
/// repaired refuses. An absent socket keeps the warning, since forwarding was
/// never set up for it.
pub fn ssh_agent_socket_outcome(
    profile: &str,
    health: SshAgentSocketHealth,
    repair: &Result<SshAgentSymlinkRepair, String>,
    advice_mode: ColimaAdviceMode,
) -> SshAgentSocketOutcome {
    if !health.is_broken() {
        return SshAgentSocketOutcome::Proceed { relinked: None };
    }
    match repair {
        Ok(SshAgentSymlinkRepair::Repaired { source }) => {
            return SshAgentSocketOutcome::Proceed {
                relinked: Some(source.clone()),
            };
        }
        Ok(SshAgentSymlinkRepair::AlreadyLive) => {
            return SshAgentSocketOutcome::Proceed { relinked: None };
        }
        _ => {}
    }
    let detail = match health {
        SshAgentSocketHealth::Stale => "is stale (the host SSH-agent socket rotated)",
        SshAgentSocketHealth::Absent => "is not set up",
        _ => "is unavailable",
    };
    let fix = match advice_mode {
        ColimaAdviceMode::Interactive => {
            format!("Fix: run `colima restart {profile}`, then re-run this command.")
        }
        ColimaAdviceMode::Automated => {
            colima_profile_recovery_advice(profile, ColimaAdviceMode::Automated)
        }
    };
    let message = format!(
        "workspace SSH-agent forwarding {detail} on colima profile `{profile}`. The \
         workspace container can fail to start with `mkdir /run/host-services/ssh-auth.sock: \
         file exists`. {fix}"
    );
    match health {
        SshAgentSocketHealth::Stale => SshAgentSocketOutcome::Refuse(format!(
            "{message} Effigy did not start the workspace: no live guest agent socket could replace the dangling link."
        )),
        _ => SshAgentSocketOutcome::Warn(message),
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

    fn run_repair_with(
        root: &Path,
        colima: &Path,
        forwarded: &Path,
        hooks: RepairRaceHooks<'_>,
    ) -> Result<SshAgentSymlinkRepair, String> {
        let program = colima.to_string_lossy().into_owned();
        let forwarded = forwarded.to_string_lossy().into_owned();
        let candidates = root.join("ssh-*/agent.*").to_string_lossy().into_owned();
        repair_colima_ssh_agent_symlink_with(
            "effigy",
            root,
            RepairRun {
                program: &program,
                forwarded: &forwarded,
                candidates: &candidates,
                privilege: "",
                timeout: Duration::from_secs(20),
                hooks,
            },
        )
    }

    fn run_repair(root: &Path, colima: &Path, forwarded: &Path) -> SshAgentSymlinkRepair {
        run_repair_with(root, colima, forwarded, NO_RACE_HOOKS).expect("repair command runs")
    }

    fn no_stale_leftovers(forwarded: &Path) {
        let dir = forwarded.parent().expect("forwarded parent");
        for entry in std::fs::read_dir(dir).expect("forwarded dir") {
            let name = entry.expect("dir entry").file_name();
            assert!(
                !name.to_string_lossy().contains("effigy-stale"),
                "repair left a moved-aside entry behind: {name:?}"
            );
        }
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

        // Broken symlink left by a rotated agent: relinked in place.
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
        no_stale_leftovers(&forwarded);

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
            SshAgentSymlinkRepair::AlreadyLive
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
            SshAgentSymlinkRepair::Foreign
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

    /// A competing writer lands a live socket on the forwarded path after the
    /// repair decided the path was free. The link must fail rather than
    /// replace it.
    #[test]
    fn repair_loses_cleanly_to_a_socket_that_lands_before_the_link() {
        use std::os::unix::fs::FileTypeExt;
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(root.path().join("ssh-abc")).expect("agent dir");
        let _agent = std::os::unix::net::UnixListener::bind(root.path().join("ssh-abc/agent.1"))
            .expect("live guest agent");
        let racer = root.path().join("racer.sock");
        let _racer = std::os::unix::net::UnixListener::bind(&racer).expect("competing socket");
        let colima = fake_colima(root.path());
        let land_racer = format!("mv '{}' '{}'", racer.display(), forwarded.display());

        // Absent path: the competing socket arrives between the check and `ln`.
        let outcome = run_repair_with(
            root.path(),
            &colima,
            &forwarded,
            RepairRaceHooks {
                before_move: ":",
                before_link: &land_racer,
            },
        )
        .expect("repair command runs");
        assert_eq!(outcome, SshAgentSymlinkRepair::Failed);
        assert!(std::fs::symlink_metadata(&forwarded)
            .expect("competing socket")
            .file_type()
            .is_socket());
    }

    /// A broken symlink is moved aside and removed; a competing socket that
    /// lands after that must still survive the link step.
    #[test]
    fn repair_loses_cleanly_to_a_socket_that_lands_after_a_broken_link_is_cleared() {
        use std::os::unix::fs::FileTypeExt;
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        std::os::unix::fs::symlink(root.path().join("gone.sock"), &forwarded).expect("broken link");
        std::fs::create_dir_all(root.path().join("ssh-abc")).expect("agent dir");
        let _agent = std::os::unix::net::UnixListener::bind(root.path().join("ssh-abc/agent.1"))
            .expect("live guest agent");
        let racer = root.path().join("racer.sock");
        let _racer = std::os::unix::net::UnixListener::bind(&racer).expect("competing socket");
        let colima = fake_colima(root.path());
        let land_racer = format!("mv '{}' '{}'", racer.display(), forwarded.display());

        let outcome = run_repair_with(
            root.path(),
            &colima,
            &forwarded,
            RepairRaceHooks {
                before_move: ":",
                before_link: &land_racer,
            },
        )
        .expect("repair command runs");
        assert_eq!(outcome, SshAgentSymlinkRepair::Failed);
        assert!(std::fs::symlink_metadata(&forwarded)
            .expect("competing socket")
            .file_type()
            .is_socket());
        no_stale_leftovers(&forwarded);
    }

    /// A competing socket replaces the broken link between the check and the
    /// move aside. The repair must restore that socket, not delete it.
    #[test]
    fn repair_restores_a_socket_that_replaces_a_broken_link_mid_repair() {
        use std::os::unix::fs::FileTypeExt;
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        std::os::unix::fs::symlink(root.path().join("gone.sock"), &forwarded).expect("broken link");
        std::fs::create_dir_all(root.path().join("ssh-abc")).expect("agent dir");
        let _agent = std::os::unix::net::UnixListener::bind(root.path().join("ssh-abc/agent.1"))
            .expect("live guest agent");
        let racer = root.path().join("racer.sock");
        let _racer = std::os::unix::net::UnixListener::bind(&racer).expect("competing socket");
        let colima = fake_colima(root.path());
        let swap_racer = format!("mv -f '{}' '{}'", racer.display(), forwarded.display());

        let outcome = run_repair_with(
            root.path(),
            &colima,
            &forwarded,
            RepairRaceHooks {
                before_move: &swap_racer,
                before_link: ":",
            },
        )
        .expect("repair command runs");
        assert_eq!(outcome, SshAgentSymlinkRepair::AlreadyLive);
        assert!(std::fs::symlink_metadata(&forwarded)
            .expect("restored socket")
            .file_type()
            .is_socket());
        no_stale_leftovers(&forwarded);
    }

    #[test]
    fn repair_is_bounded_by_its_timeout() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        // A `colima` that never returns: the repair must give up at its deadline.
        let colima = root.path().join("colima-slow");
        std::fs::write(&colima, "#!/bin/sh\nexec sleep 5\n").expect("slow colima");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&colima, std::fs::Permissions::from_mode(0o755))
                .expect("slow colima is executable");
        }
        let program = colima.to_string_lossy().into_owned();
        let forwarded_text = forwarded.to_string_lossy().into_owned();
        let candidates = root
            .path()
            .join("ssh-*/agent.*")
            .to_string_lossy()
            .into_owned();
        let started = Instant::now();
        let result = repair_colima_ssh_agent_symlink_with(
            "effigy",
            root.path(),
            RepairRun {
                program: &program,
                forwarded: &forwarded_text,
                candidates: &candidates,
                privilege: "",
                timeout: Duration::from_millis(300),
                hooks: NO_RACE_HOOKS,
            },
        );
        assert!(result.is_err(), "a hung repair must fail: {result:?}");
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the repair returned at its deadline, not after the sleep: {:?}",
            started.elapsed()
        );
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
            classify_ssh_agent_repair_output("live\n"),
            SshAgentSymlinkRepair::AlreadyLive
        );
        assert_eq!(
            classify_ssh_agent_repair_output("foreign"),
            SshAgentSymlinkRepair::Foreign
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

    #[test]
    fn outcome_proceeds_when_the_repair_or_the_socket_is_healthy() {
        assert_eq!(
            ssh_agent_socket_outcome(
                "effigy",
                SshAgentSocketHealth::Healthy,
                &Ok(SshAgentSymlinkRepair::Failed),
                ColimaAdviceMode::Interactive,
            ),
            SshAgentSocketOutcome::Proceed { relinked: None }
        );
        assert_eq!(
            ssh_agent_socket_outcome(
                "effigy",
                SshAgentSocketHealth::Stale,
                &Ok(SshAgentSymlinkRepair::Repaired {
                    source: "/tmp/ssh-x/agent.1".to_owned()
                }),
                ColimaAdviceMode::Automated,
            ),
            SshAgentSocketOutcome::Proceed {
                relinked: Some("/tmp/ssh-x/agent.1".to_owned())
            }
        );
        assert_eq!(
            ssh_agent_socket_outcome(
                "effigy",
                SshAgentSocketHealth::Stale,
                &Ok(SshAgentSymlinkRepair::AlreadyLive),
                ColimaAdviceMode::Automated,
            ),
            SshAgentSocketOutcome::Proceed { relinked: None }
        );
    }

    #[test]
    fn outcome_refuses_a_dangling_link_with_no_live_agent_and_advice_by_caller() {
        let interactive = ssh_agent_socket_outcome(
            "effigy",
            SshAgentSocketHealth::Stale,
            &Ok(SshAgentSymlinkRepair::NoLiveAgent),
            ColimaAdviceMode::Interactive,
        );
        let SshAgentSocketOutcome::Refuse(message) = interactive else {
            panic!("a dangling link with no live agent refuses");
        };
        assert!(message.contains("Effigy did not start the workspace"));
        assert!(message.contains("colima restart effigy"));

        let automated = ssh_agent_socket_outcome(
            "effigy",
            SshAgentSocketHealth::Stale,
            &Ok(SshAgentSymlinkRepair::NoLiveAgent),
            ColimaAdviceMode::Automated,
        );
        let SshAgentSocketOutcome::Refuse(message) = automated else {
            panic!("a dangling link with no live agent refuses");
        };
        assert!(message.contains("report the degraded profile to the host operator"));
        assert!(!message.contains("colima restart"));
        assert!(!message.contains("colima stop"));

        // A repair that could not run also leaves the dangling link in place.
        assert!(matches!(
            ssh_agent_socket_outcome(
                "effigy",
                SshAgentSocketHealth::Stale,
                &Err("colima ssh exited unsuccessfully".to_owned()),
                ColimaAdviceMode::Automated,
            ),
            SshAgentSocketOutcome::Refuse(_)
        ));
    }

    #[test]
    fn outcome_only_warns_for_an_absent_socket_without_a_live_agent() {
        assert!(matches!(
            ssh_agent_socket_outcome(
                "effigy",
                SshAgentSocketHealth::Absent,
                &Ok(SshAgentSymlinkRepair::NoLiveAgent),
                ColimaAdviceMode::Automated,
            ),
            SshAgentSocketOutcome::Warn(message) if message.contains("is not set up")
        ));
    }
}
