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
    /// The forwarded path was a broken symlink, or absent, and now resolves to
    /// a live guest agent socket.
    Repaired { source: String },
    /// The forwarded path already resolves to a live socket. Nothing was changed.
    AlreadyLive,
    /// The forwarded path exists and is not a symlink to a socket, or is a
    /// symlink to something other than a broken link. Effigy never replaces it.
    Foreign,
    /// No live forwarded agent socket exists in the guest to link to.
    NoLiveAgent,
    /// The link was not made, or the output was not a recognised verdict.
    Failed,
}

/// A point where a concurrent writer could land between the repair's check and
/// its change. Production passes the shell no-op `:`. The race tests put a
/// competing socket there to prove the repair never displaces it.
#[derive(Debug, Clone, Copy)]
struct RepairRaceHooks<'a> {
    /// Runs after every check, immediately before the one conditional `ln -s`.
    before_link: &'a str,
}

const NO_RACE_HOOKS: RepairRaceHooks<'static> = RepairRaceHooks { before_link: ":" };

/// Repair script run inside the VM. It prints exactly one verdict line.
///
/// It never moves or removes an existing entry. The only mutation is one
/// `ln -s` without `-f`, which creates a new name and fails if anything already
/// occupies it. The forwarded path itself is never touched:
///
/// - Absent path: the link is created at the forwarded path.
/// - Broken symlink to an absolute target: the link is created at that missing
///   target, so the forwarded path resolves through a chain to the live socket
///   and its bytes stay as they were.
///
/// A live socket, a regular file, or a symlink to an existing non-socket is left
/// as it is. `privilege` prefixes the root-only commands: `sudo ` inside the VM,
/// empty for fixture runs.
fn ssh_agent_symlink_repair_script(
    forwarded: &str,
    candidates: &str,
    privilege: &str,
    hooks: RepairRaceHooks<'_>,
) -> String {
    let RepairRaceHooks { before_link } = hooks;
    format!(
        "src={forwarded}; \
         if [ -S \"$src\" ]; then echo live; exit 0; fi; \
         if [ -e \"$src\" ]; then echo foreign; exit 0; fi; \
         live=; for s in {candidates}; do if [ -S \"$s\" ]; then live=$s; break; fi; done; \
         if [ -z \"$live\" ]; then echo no-live-agent; exit 0; fi; \
         if [ -L \"$src\" ]; then \
           target=$(readlink \"$src\"); \
           case \"$target\" in /*) ;; *) target=\"$(dirname \"$src\")/$target\" ;; esac; \
           {privilege}mkdir -p \"$(dirname \"$target\")\" || exit 1; \
           {before_link}; \
           {privilege}ln -s \"$live\" \"$target\" && [ -S \"$src\" ] && echo \"repaired $live\" || echo failed; \
         else \
           {privilege}mkdir -p \"$(dirname \"$src\")\" || exit 1; \
           {before_link}; \
           {privilege}ln -s \"$live\" \"$src\" && [ -S \"$src\" ] && echo \"repaired $live\" || echo failed; \
         fi"
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

/// The host agent socket's path as the workspace service mounts it.
const HOST_SSH_AGENT_TARGET: &str = "/run/host-services/ssh-auth.sock";

/// Whether the policy's primary workspace service mounts the host SSH agent.
///
/// The workspace mount rewrite writes that mount into the primary service's
/// volumes when forwarding is on. The effective compose documents are read
/// through the policy's own reader, which resolves the rendered snapshot
/// wherever it lives (`checkout/` or `external/`). Forwarding defaults to on,
/// so a policy whose compose cannot be read counts as mounted.
pub fn policy_mounts_host_ssh_agent(policy: &EffectiveContainerPolicy) -> bool {
    let mut defined = false;
    let mut mounted = false;
    for path in &policy.compose_files {
        let Ok(content) = policy.read_compose(path) else {
            continue;
        };
        if let Some(mount) =
            compose_service_mounts_host_ssh_agent(&content, &policy.primary_service)
        {
            defined = true;
            mounted |= mount;
        }
    }
    if defined {
        mounted
    } else {
        true
    }
}

/// Whether one rendered compose document mounts the host SSH agent on
/// `service`. `None` when the document does not define the service.
fn compose_service_mounts_host_ssh_agent(content: &str, service: &str) -> Option<bool> {
    let document: serde_yaml::Value = serde_yaml::from_str(content).ok()?;
    let definition = document.get("services")?.get(service)?;
    let Some(volumes) = definition
        .get("volumes")
        .and_then(|value| value.as_sequence())
    else {
        return Some(false);
    };
    Some(volumes.iter().any(|volume| match volume {
        serde_yaml::Value::String(text) => {
            text.split(':').any(|part| part == HOST_SSH_AGENT_TARGET)
        }
        serde_yaml::Value::Mapping(_) => {
            volume.get("target").and_then(|value| value.as_str()) == Some(HOST_SSH_AGENT_TARGET)
        }
        _ => false,
    }))
}

/// What `container up` does with the forwarded socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshAgentSocketOutcome {
    /// Bring-up continues. `relinked` names the live socket when Effigy
    /// repaired the link.
    Proceed { relinked: Option<String> },
    /// Bring-up stops. The workspace mounts the host agent, which cannot be
    /// used, so its containers would be stranded in `Created`.
    Refuse(String),
}

/// Decide `container up`'s outcome for the host SSH-agent socket.
///
/// Workloads that do not mount the agent never probe or repair the shared VM,
/// so their bring-up is unaffected. For a mounting workload, a broken socket is
/// repaired when a live guest agent exists, and otherwise refuses. The probe
/// and the repair run lazily, so they only happen when they are needed.
pub fn ssh_agent_bring_up_decision(
    profile: &str,
    mounts_agent: bool,
    probe: impl FnOnce() -> SshAgentSocketHealth,
    repair: impl FnOnce() -> Result<SshAgentSymlinkRepair, String>,
    advice_mode: ColimaAdviceMode,
) -> SshAgentSocketOutcome {
    if !mounts_agent {
        return SshAgentSocketOutcome::Proceed { relinked: None };
    }
    let health = probe();
    if !health.is_broken() {
        return SshAgentSocketOutcome::Proceed { relinked: None };
    }
    match repair() {
        Ok(SshAgentSymlinkRepair::Repaired { source }) => {
            return SshAgentSocketOutcome::Proceed {
                relinked: Some(source),
            };
        }
        Ok(SshAgentSymlinkRepair::AlreadyLive) => {
            return SshAgentSocketOutcome::Proceed { relinked: None };
        }
        _ => {}
    }
    let detail = match health {
        SshAgentSocketHealth::Stale => "is stale (the host SSH-agent socket rotated)",
        _ => "is not set up",
    };
    let fix = match advice_mode {
        ColimaAdviceMode::Interactive => {
            format!("Fix: run `colima restart {profile}`, then re-run this command.")
        }
        ColimaAdviceMode::Automated => {
            colima_profile_recovery_advice(profile, ColimaAdviceMode::Automated)
        }
    };
    SshAgentSocketOutcome::Refuse(format!(
        "workspace SSH-agent forwarding {detail} on colima profile `{profile}`. The workspace \
         mounts the host agent at {HOST_SSH_AGENT_TARGET}, and its container cannot start without \
         a live socket there. Effigy did not start the workspace: no live guest agent socket \
         could be linked. {fix}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

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

    /// A live guest agent under `root/ssh-abc/`, kept alive by the returned listener.
    fn live_guest_agent(root: &Path) -> (std::path::PathBuf, std::os::unix::net::UnixListener) {
        std::fs::create_dir_all(root.join("ssh-abc")).expect("agent dir");
        let live = root.join("ssh-abc/agent.1");
        let listener = std::os::unix::net::UnixListener::bind(&live).expect("live guest agent");
        (live, listener)
    }

    /// Whether `path` resolves, through any symlinks, to a socket.
    fn is_socket(path: &Path) -> bool {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(path)
            .map(|meta| meta.file_type().is_socket())
            .unwrap_or(false)
    }

    #[test]
    fn repair_links_an_absent_path_to_a_live_guest_agent_in_place() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        let (live, _agent) = live_guest_agent(root.path());
        let colima = fake_colima(root.path());

        // The missing parent is created and the link points at the live agent.
        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Repaired {
                source: live.to_string_lossy().into_owned()
            }
        );
        assert!(is_socket(&forwarded));

        // Only the guest `ssh` ran; the fake binary would have failed any other verb.
        let calls = std::fs::read_to_string(root.path().join("calls")).expect("call log");
        assert_eq!(calls.lines().count(), 1);
        assert!(calls
            .lines()
            .all(|line| line.starts_with("ssh --profile effigy -- sh -c")));
    }

    #[test]
    fn repair_redirects_a_broken_link_without_touching_the_forwarded_path() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        let missing_target = root.path().join("ssh-old/agent.9");
        std::os::unix::fs::symlink(&missing_target, &forwarded).expect("broken link");
        let (live, _agent) = live_guest_agent(root.path());
        let colima = fake_colima(root.path());

        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Repaired {
                source: live.to_string_lossy().into_owned()
            }
        );
        // The forwarded path is the same symlink it was; the missing target is
        // now a link to the live agent, so the chain resolves to a socket.
        assert_eq!(
            std::fs::read_link(&forwarded).expect("forwarded link"),
            missing_target
        );
        assert!(is_socket(&forwarded));
        assert!(is_socket(&missing_target));
    }

    #[test]
    fn repair_never_replaces_a_live_socket_or_a_non_socket_entry() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        let (_live, _agent) = live_guest_agent(root.path());
        let colima = fake_colima(root.path());

        // A live socket at the forwarded path is left exactly as it is.
        let live_forwarded =
            std::os::unix::net::UnixListener::bind(&forwarded).expect("live forwarded socket");
        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::AlreadyLive
        );
        assert!(is_socket(&forwarded));
        drop(live_forwarded);
        std::fs::remove_file(&forwarded).expect("drop forwarded socket");

        // A regular file is never replaced.
        std::fs::write(&forwarded, "not a socket").expect("regular file");
        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Foreign
        );
        assert!(std::fs::symlink_metadata(&forwarded)
            .expect("forwarded path")
            .file_type()
            .is_file());
        std::fs::remove_file(&forwarded).expect("drop regular file");

        // A symlink to an existing non-socket is not a broken link, so it stays.
        let existing = root.path().join("existing.txt");
        std::fs::write(&existing, "not a socket").expect("existing target");
        std::os::unix::fs::symlink(&existing, &forwarded).expect("symlink to a file");
        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Foreign
        );
        assert_eq!(
            std::fs::read_link(&forwarded).expect("forwarded link"),
            existing
        );
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

    /// A broken link with a relative target resolves against the link's own
    /// directory. Its missing target is created as a link to the live agent, so
    /// the forwarded path recovers without its bytes changing.
    #[test]
    fn repair_redirects_a_relative_broken_link_to_a_live_guest_agent() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        std::os::unix::fs::symlink("gone.sock", &forwarded).expect("relative broken link");
        let (live, _agent) = live_guest_agent(root.path());
        let colima = fake_colima(root.path());

        assert_eq!(
            run_repair(root.path(), &colima, &forwarded),
            SshAgentSymlinkRepair::Repaired {
                source: live.to_string_lossy().into_owned()
            }
        );
        assert_eq!(
            std::fs::read_link(&forwarded).expect("forwarded link"),
            Path::new("gone.sock"),
            "the forwarded link keeps its own bytes"
        );
        let resolved = forwarded.parent().expect("parent").join("gone.sock");
        assert_eq!(
            std::fs::read_link(&resolved).expect("redirect target"),
            live,
            "the relative target now links to the live agent"
        );
        assert!(is_socket(&forwarded));
    }

    /// A competing writer fills the resolved relative target before the link.
    /// The conditional link fails and the writer's socket survives.
    #[test]
    fn repair_loses_cleanly_when_a_socket_fills_a_relative_redirect_target_first() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        std::os::unix::fs::symlink("gone.sock", &forwarded).expect("relative broken link");
        let (_live, _agent) = live_guest_agent(root.path());
        let resolved = forwarded.parent().expect("parent").join("gone.sock");
        let racer = root.path().join("racer.sock");
        let _racer = std::os::unix::net::UnixListener::bind(&racer).expect("competing socket");
        let colima = fake_colima(root.path());
        let land_racer = format!("mv '{}' '{}'", racer.display(), resolved.display());

        let outcome = run_repair_with(
            root.path(),
            &colima,
            &forwarded,
            RepairRaceHooks {
                before_link: &land_racer,
            },
        )
        .expect("repair command runs");
        assert_eq!(outcome, SshAgentSymlinkRepair::Failed);
        assert!(is_socket(&resolved), "the competing socket is intact");
    }

    /// A competing writer fills the absent forwarded path between the repair's
    /// check and its link. The link must fail and the writer's socket survive.
    #[test]
    fn repair_loses_cleanly_when_a_socket_fills_an_absent_path_first() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        let (_live, _agent) = live_guest_agent(root.path());
        let racer = root.path().join("racer.sock");
        let _racer = std::os::unix::net::UnixListener::bind(&racer).expect("competing socket");
        let colima = fake_colima(root.path());
        let land_racer = format!("mv '{}' '{}'", racer.display(), forwarded.display());

        let outcome = run_repair_with(
            root.path(),
            &colima,
            &forwarded,
            RepairRaceHooks {
                before_link: &land_racer,
            },
        )
        .expect("repair command runs");
        assert_eq!(outcome, SshAgentSymlinkRepair::Failed);
        assert!(is_socket(&forwarded), "the competing socket is intact");
    }

    /// A competing writer fills the missing link target between the check and
    /// the link. The conditional link fails, and the writer's socket survives.
    #[test]
    fn repair_loses_cleanly_when_a_socket_fills_the_redirect_target_first() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        let missing_target = root.path().join("ssh-old/agent.9");
        std::os::unix::fs::symlink(&missing_target, &forwarded).expect("broken link");
        let (_live, _agent) = live_guest_agent(root.path());
        std::fs::create_dir_all(missing_target.parent().expect("target dir")).expect("target dir");
        let racer = root.path().join("racer.sock");
        let _racer = std::os::unix::net::UnixListener::bind(&racer).expect("competing socket");
        let colima = fake_colima(root.path());
        let land_racer = format!("mv '{}' '{}'", racer.display(), missing_target.display());

        let outcome = run_repair_with(
            root.path(),
            &colima,
            &forwarded,
            RepairRaceHooks {
                before_link: &land_racer,
            },
        )
        .expect("repair command runs");
        assert_eq!(outcome, SshAgentSymlinkRepair::Failed);
        assert!(is_socket(&missing_target), "the competing socket is intact");
        assert_eq!(
            std::fs::read_link(&forwarded).expect("forwarded link"),
            missing_target,
            "the forwarded link is unchanged"
        );
    }

    /// A competing writer replaces the forwarded broken link with its own live
    /// socket between the check and the link. The repair must not displace that
    /// socket: the forwarded path keeps it, and the repair never reports the
    /// writer's socket as its own change.
    #[test]
    fn repair_never_displaces_a_socket_that_replaces_the_broken_link_mid_repair() {
        let root = tempfile::tempdir().expect("repair fixture");
        let forwarded = root.path().join("run/host-services/ssh-auth.sock");
        std::fs::create_dir_all(forwarded.parent().expect("parent")).expect("forwarded dir");
        let missing_target = root.path().join("ssh-old/agent.9");
        std::os::unix::fs::symlink(&missing_target, &forwarded).expect("broken link");
        let (_live, _agent) = live_guest_agent(root.path());
        let racer = root.path().join("racer.sock");
        let _racer = std::os::unix::net::UnixListener::bind(&racer).expect("competing socket");
        let colima = fake_colima(root.path());
        let swap_racer = format!("mv -f '{}' '{}'", racer.display(), forwarded.display());

        let outcome = run_repair_with(
            root.path(),
            &colima,
            &forwarded,
            RepairRaceHooks {
                before_link: &swap_racer,
            },
        )
        .expect("repair command runs");
        assert!(
            !matches!(outcome, SshAgentSymlinkRepair::Failed),
            "the path resolves to the competing socket: {outcome:?}"
        );
        assert!(is_socket(&forwarded), "the competing socket is intact");
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
    fn compose_mount_detection_follows_the_primary_service_only() {
        let mounted = "services:\n  workspace:\n    volumes:\n      - /run/host-services/ssh-auth.sock:/run/host-services/ssh-auth.sock\n";
        assert_eq!(
            compose_service_mounts_host_ssh_agent(mounted, "workspace"),
            Some(true)
        );
        let long_form = "services:\n  workspace:\n    volumes:\n      - type: bind\n        source: /x\n        target: /run/host-services/ssh-auth.sock\n";
        assert_eq!(
            compose_service_mounts_host_ssh_agent(long_form, "workspace"),
            Some(true)
        );
        // An opted-out workspace, and a sidecar that mounts the socket, both do not mount it.
        let opted_out = "services:\n  workspace:\n    volumes:\n      - /x:/y\n";
        assert_eq!(
            compose_service_mounts_host_ssh_agent(opted_out, "workspace"),
            Some(false)
        );
        let sidecar_only = "services:\n  workspace:\n    image: x\n  tools:\n    volumes:\n      - /run/host-services/ssh-auth.sock:/run/host-services/ssh-auth.sock\n";
        assert_eq!(
            compose_service_mounts_host_ssh_agent(sidecar_only, "workspace"),
            Some(false)
        );
        assert_eq!(
            compose_service_mounts_host_ssh_agent(mounted, "absent"),
            None
        );
    }

    #[test]
    fn opted_out_workload_never_probes_or_repairs_the_shared_vm() {
        let outcome = ssh_agent_bring_up_decision(
            "effigy",
            false,
            || panic!("an opted-out workload must not probe the VM socket"),
            || panic!("an opted-out workload must not repair the VM socket"),
            ColimaAdviceMode::Automated,
        );
        assert_eq!(outcome, SshAgentSocketOutcome::Proceed { relinked: None });
    }

    #[test]
    fn healthy_mounting_workload_proceeds_without_a_repair() {
        let outcome = ssh_agent_bring_up_decision(
            "effigy",
            true,
            || SshAgentSocketHealth::Healthy,
            || panic!("a healthy socket must not be repaired"),
            ColimaAdviceMode::Automated,
        );
        assert_eq!(outcome, SshAgentSocketOutcome::Proceed { relinked: None });
    }

    #[test]
    fn mounting_workload_with_a_repairable_socket_proceeds_with_the_relink() {
        let outcome = ssh_agent_bring_up_decision(
            "effigy",
            true,
            || SshAgentSocketHealth::Stale,
            || {
                Ok(SshAgentSymlinkRepair::Repaired {
                    source: "/tmp/ssh-x/agent.1".to_owned(),
                })
            },
            ColimaAdviceMode::Automated,
        );
        assert_eq!(
            outcome,
            SshAgentSocketOutcome::Proceed {
                relinked: Some("/tmp/ssh-x/agent.1".to_owned())
            }
        );
    }

    #[test]
    fn mounting_workload_refuses_when_the_socket_is_absent_and_no_agent_exists() {
        let repairs = Cell::new(0);
        let outcome = ssh_agent_bring_up_decision(
            "effigy",
            true,
            || SshAgentSocketHealth::Absent,
            || {
                repairs.set(repairs.get() + 1);
                Ok(SshAgentSymlinkRepair::NoLiveAgent)
            },
            ColimaAdviceMode::Interactive,
        );
        assert_eq!(repairs.get(), 1);
        let SshAgentSocketOutcome::Refuse(message) = outcome else {
            panic!("an absent socket the workspace mounts refuses");
        };
        assert!(
            message.contains("Effigy did not start the workspace"),
            "{message}"
        );
        assert!(message.contains("colima restart effigy"), "{message}");
    }

    #[test]
    fn mounting_workload_refusal_for_non_interactive_callers_never_suggests_stop_or_restart() {
        for health in [SshAgentSocketHealth::Stale, SshAgentSocketHealth::Absent] {
            let outcome = ssh_agent_bring_up_decision(
                "effigy",
                true,
                || health,
                || Ok(SshAgentSymlinkRepair::NoLiveAgent),
                ColimaAdviceMode::Automated,
            );
            let SshAgentSocketOutcome::Refuse(message) = outcome else {
                panic!("a mounting workload refuses without a live agent: {health:?}");
            };
            assert!(
                message.contains("report the degraded profile to the host operator"),
                "{message}"
            );
            assert!(!message.contains("colima restart"), "{message}");
            assert!(!message.contains("colima stop"), "{message}");
        }
    }

    #[test]
    fn mounting_workload_refuses_when_the_repair_cannot_run() {
        let outcome = ssh_agent_bring_up_decision(
            "effigy",
            true,
            || SshAgentSocketHealth::Stale,
            || Err("colima ssh exited unsuccessfully".to_owned()),
            ColimaAdviceMode::Automated,
        );
        assert!(matches!(outcome, SshAgentSocketOutcome::Refuse(_)));
    }
}
