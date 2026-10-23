//! Stale nerdctl health-check pid files in the Colima VM.
//!
//! nerdctl 2.3.1 starts every health check as a containerd exec process named
//! `health-check-<id>` and never deletes it, so containerd's runtime-v2 task
//! directory keeps one `health-check-<id>.pid` file per check, forever. Across
//! a profile's stack that is tens of thousands of files per container per day,
//! and `/run` is a tmpfs with a finite inode count: once it fills, every
//! `exec` and `create` in the profile fails with `no space left on device`.
//! nerdctl 2.4.1 deletes the exec process in a `defer`, which removes the leak
//! at its source (effigy#291, papercut 03f4e93a).
//!
//! Effigy cannot upgrade the runtime inside a running Colima profile from a
//! task, so it reports the pressure in `effigy doctor` and prunes exactly the
//! leaked files — matching the exact `health-check-*.pid` name, directly inside
//! a containerd runtime-v2 task directory, older than
//! [`HEALTHCHECK_PIDFILE_STALE_MINUTES`] — on `container up` and on request.

use std::path::Path;
use std::time::Instant;

use super::process::run_command_capture_allow_failure_with_deadline;

use super::implementation::ContainerExecError;

/// The containerd runtime-v2 task root inside the Colima VM. Each container's
/// exec pid files sit directly below `<root>/<namespace>/<container>/`.
pub const HEALTHCHECK_TASK_ROOT: &str = "/run/containerd/io.containerd.runtime.v2.task";

/// The only file name pattern the prune may ever match.
pub const HEALTHCHECK_PIDFILE_PATTERN: &str = "health-check-*.pid";

/// A pid file younger than this may belong to a health check still in flight,
/// so the prune never touches it.
pub const HEALTHCHECK_PIDFILE_STALE_MINUTES: u64 = 10;

/// Inode use at which `/run` is called pressured even before it is full.
pub const RUN_INODE_PRESSURE_PERCENT: u64 = 80;

/// Stale pid files that already justify a doctor warning.
pub const STALE_PIDFILE_WARNING_COUNT: u64 = 10_000;

/// What the VM reported about `/run` inodes and leaked health-check pid files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunInodePressure {
    /// `df -i /run` inode total.
    pub inode_total: u64,
    /// `df -i /run` inode use.
    pub inode_used: u64,
    /// Every `health-check-*.pid` file under the task root.
    pub pid_files: u64,
    /// Those older than [`HEALTHCHECK_PIDFILE_STALE_MINUTES`].
    pub stale_pid_files: u64,
}

impl RunInodePressure {
    /// Integer inode-use percentage; `0` when the VM reported no inodes.
    pub fn inode_use_percent(&self) -> u64 {
        if self.inode_total == 0 {
            return 0;
        }
        self.inode_used.saturating_mul(100) / self.inode_total
    }

    /// `/run` is close enough to full, or the leak is large enough, that the
    /// profile will start refusing creates and execs.
    pub fn is_pressured(&self) -> bool {
        self.inode_use_percent() >= RUN_INODE_PRESSURE_PERCENT
            || self.stale_pid_files >= STALE_PIDFILE_WARNING_COUNT
    }

    /// `1912/819200 inodes (0%), 907 health-check pid files (0 older than 10m)`
    pub fn summary(&self) -> String {
        format!(
            "{}/{} inodes used ({}%), {} health-check pid file(s) ({} older than {}m)",
            self.inode_used,
            self.inode_total,
            self.inode_use_percent(),
            self.pid_files,
            self.stale_pid_files,
            HEALTHCHECK_PIDFILE_STALE_MINUTES
        )
    }

    /// The doctor warning for a pressured profile, naming both remediations.
    pub fn warning(&self, profile: &str) -> Option<String> {
        if !self.is_pressured() {
            return None;
        }
        Some(format!(
            "colima profile `{profile}`: /run inode use is {} ({}/{}), with {} stale nerdctl \
             health-check pid file(s) under {HEALTHCHECK_TASK_ROOT}. nerdctl 2.3.1 leaks one per \
             health check and nerdctl 2.4.1 fixes it; once /run fills, every exec and container \
             create in the profile fails with `no space left on device`. Effigy prunes stale files \
             on `container up`; prune now with `effigy container profile prune-health-pid-files \
             --profile {profile}`. Do not delete anything else under {HEALTHCHECK_TASK_ROOT}.",
            self.inode_use_percent(),
            self.inode_used,
            self.inode_total,
            self.stale_pid_files
        ))
    }
}

/// The result of one bounded prune.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthCheckPidFilePrune {
    /// Matching files before the prune.
    pub before: u64,
    /// Matching files after the prune.
    pub after: u64,
}

impl HealthCheckPidFilePrune {
    /// Files the prune removed. `before - after` is the durable truth: a
    /// concurrent health check may add a fresh file, which the prune correctly
    /// leaves alone.
    pub fn deleted(&self) -> u64 {
        self.before.saturating_sub(self.after)
    }

    /// `removed 268184 stale nerdctl health-check pid file(s); 907 remain`
    pub fn summary(&self) -> String {
        format!(
            "removed {} stale nerdctl health-check pid file(s); {} remain",
            self.deleted(),
            self.after
        )
    }
}

/// Read `/run` inode use and the leaked pid-file counts from `profile`.
///
/// Read-only. The caller's absolute deadline bounds the one bounded `colima
/// ssh` call and nothing is retried here.
pub fn inspect_run_inode_pressure(
    profile: &str,
    repo_root: &Path,
    deadline: Instant,
) -> Result<RunInodePressure, ContainerExecError> {
    let script = run_inode_pressure_probe_script();
    let output = run_vm_script(
        profile,
        repo_root,
        &script,
        "inspect Colima /run inodes",
        deadline,
    )?;
    parse_run_inode_pressure(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        ContainerExecError::Failure {
            command: "inspect Colima /run inodes".to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: format!(
                "[effigy] the VM probe did not report inode use and pid-file counts. Probe output:\n{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        }
    })
}

/// Delete the leaked pid files older than [`HEALTHCHECK_PIDFILE_STALE_MINUTES`]
/// under the exact containerd task root, and report the counts around it.
///
/// Only files matching [`HEALTHCHECK_PIDFILE_PATTERN`], directly inside a
/// containerd runtime-v2 task directory, are ever deleted. The caller's
/// absolute deadline bounds the one bounded `colima ssh` call.
pub fn prune_stale_health_check_pidfiles(
    profile: &str,
    repo_root: &Path,
    deadline: Instant,
) -> Result<HealthCheckPidFilePrune, ContainerExecError> {
    let script = prune_stale_health_check_pidfiles_script();
    let output = run_vm_script(
        profile,
        repo_root,
        &script,
        "prune stale nerdctl health-check pid files",
        deadline,
    )?;
    parse_health_check_pidfile_prune(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        ContainerExecError::Failure {
            command: "prune stale nerdctl health-check pid files".to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: format!(
                "[effigy] the VM prune did not report before/after counts. Prune output:\n{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        }
    })
}

fn run_vm_script(
    profile: &str,
    repo_root: &Path,
    script: &str,
    label: &str,
    deadline: Instant,
) -> Result<std::process::Output, ContainerExecError> {
    let args = [
        "ssh",
        "--profile",
        profile,
        "--",
        "sudo",
        "sh",
        "-c",
        script,
    ];
    run_command_capture_allow_failure_with_deadline(
        repo_root,
        "colima",
        &args,
        label,
        Some(deadline),
    )
}

/// The read-only probe. `-mindepth 3 -maxdepth 3` is exactly
/// `<root>/<namespace>/<container>/<file>`; `find` never follows a symlink.
fn run_inode_pressure_probe_script() -> String {
    format!(
        "root={HEALTHCHECK_TASK_ROOT}; pattern='{HEALTHCHECK_PIDFILE_PATTERN}'; \
         df -i /run | awk 'NR==2 {{print \"inodes\", $2, $3}}'; \
         printf 'pidfiles '; \
         find \"$root\" -mindepth 3 -maxdepth 3 -type f -name \"$pattern\" 2>/dev/null | wc -l; \
         printf 'stale '; \
         find \"$root\" -mindepth 3 -maxdepth 3 -type f -name \"$pattern\" -mmin +{HEALTHCHECK_PIDFILE_STALE_MINUTES} 2>/dev/null | wc -l"
    )
}

/// The prune. It prints the matching-file count before and after so the caller
/// reports real counts instead of trusting `find`'s exit status.
fn prune_stale_health_check_pidfiles_script() -> String {
    format!(
        "root={HEALTHCHECK_TASK_ROOT}; pattern='{HEALTHCHECK_PIDFILE_PATTERN}'; \
         printf 'before '; \
         find \"$root\" -mindepth 3 -maxdepth 3 -type f -name \"$pattern\" 2>/dev/null | wc -l; \
         find \"$root\" -mindepth 3 -maxdepth 3 -type f -name \"$pattern\" -mmin +{HEALTHCHECK_PIDFILE_STALE_MINUTES} -delete 2>/dev/null; \
         printf 'after '; \
         find \"$root\" -mindepth 3 -maxdepth 3 -type f -name \"$pattern\" 2>/dev/null | wc -l"
    )
}

/// Parse the probe's `inodes <total> <used>`, `pidfiles <n>` and `stale <n>`
/// lines. Every field is required: a partial report is not evidence.
pub fn parse_run_inode_pressure(stdout: &str) -> Option<RunInodePressure> {
    let mut inode_total = None;
    let mut inode_used = None;
    let mut pid_files = None;
    let mut stale_pid_files = None;
    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("inodes") => {
                inode_total = fields.next().and_then(|value| value.parse().ok());
                inode_used = fields.next().and_then(|value| value.parse().ok());
            }
            Some("pidfiles") => pid_files = fields.next().and_then(|value| value.parse().ok()),
            Some("stale") => stale_pid_files = fields.next().and_then(|value| value.parse().ok()),
            _ => {}
        }
    }
    Some(RunInodePressure {
        inode_total: inode_total?,
        inode_used: inode_used?,
        pid_files: pid_files?,
        stale_pid_files: stale_pid_files?,
    })
}

/// Parse the prune's `before <n>` and `after <n>` lines.
pub fn parse_health_check_pidfile_prune(stdout: &str) -> Option<HealthCheckPidFilePrune> {
    let mut before = None;
    let mut after = None;
    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("before") => before = fields.next().and_then(|value| value.parse().ok()),
            Some("after") => after = fields.next().and_then(|value| value.parse().ok()),
            _ => {}
        }
    }
    Some(HealthCheckPidFilePrune {
        before: before?,
        after: after?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{
        inspect_run_inode_pressure, parse_health_check_pidfile_prune, parse_run_inode_pressure,
        prune_stale_health_check_pidfiles, prune_stale_health_check_pidfiles_script,
        run_inode_pressure_probe_script, HealthCheckPidFilePrune, RunInodePressure,
        HEALTHCHECK_PIDFILE_PATTERN, HEALTHCHECK_PIDFILE_STALE_MINUTES, HEALTHCHECK_TASK_ROOT,
    };

    #[test]
    fn probe_script_reads_only_the_exact_task_root_and_pattern() {
        let script = run_inode_pressure_probe_script();
        assert!(script.contains(HEALTHCHECK_TASK_ROOT));
        assert!(script.contains(HEALTHCHECK_PIDFILE_PATTERN));
        assert!(script.contains(&format!("-mmin +{HEALTHCHECK_PIDFILE_STALE_MINUTES}")));
        assert!(script.contains("-mindepth 3 -maxdepth 3 -type f"));
        assert!(script.contains("df -i /run"));
        assert!(!script.contains("-delete"));
        assert!(!script.contains(" rm "));
    }

    #[test]
    fn prune_script_deletes_only_the_exact_pattern_older_than_ten_minutes() {
        let script = prune_stale_health_check_pidfiles_script();
        assert!(script.contains(HEALTHCHECK_TASK_ROOT));
        assert!(script.contains(&format!(
            "-name \"$pattern\" -mmin +{HEALTHCHECK_PIDFILE_STALE_MINUTES} -delete"
        )));
        assert!(script.contains("-mindepth 3 -maxdepth 3 -type f"));
        // The only deletion is the pattern-scoped find, never a bare rm,
        // wildcard expansion or the task root itself.
        assert_eq!(script.matches("-delete").count(), 1);
        assert!(!script.contains(" rm "));
        assert!(!script.contains("rm -"));
        assert!(!script.contains(&format!("{HEALTHCHECK_TASK_ROOT}/*")));
    }

    #[test]
    fn parses_the_probe_report() {
        let pressure = parse_run_inode_pressure("inodes 819200 1912\npidfiles 907\nstale 0\n")
            .expect("complete report");
        assert_eq!(
            pressure,
            RunInodePressure {
                inode_total: 819_200,
                inode_used: 1_912,
                pid_files: 907,
                stale_pid_files: 0,
            }
        );
        assert_eq!(pressure.inode_use_percent(), 0);
        assert!(!pressure.is_pressured());
        assert!(pressure.summary().contains(
            "1912/819200 inodes used (0%), 907 health-check pid file(s) (0 older than 10m)"
        ));
        assert!(pressure.warning("effigy").is_none());
    }

    /// The private adapter script: it records every argv vector it is handed
    /// and answers the probe or prune with the counts from the fixture.
    const FAKE_COLIMA: &str = r#"#!/bin/sh
printf '%s\n' "$@" >> "$FIXTURE/commands"
printf '\n' >> "$FIXTURE/commands"
case "$*" in
  *before*|*after*|*pidfiles*|*stale*)
    if [ -n "$FAKE_PRUNE" ]; then
      printf 'before 269091\nafter 907\n'
    else
      printf 'inodes 819200 270087\npidfiles 269082\nstale 268182\n'
    fi
    ;;
esac
"#;

    #[test]
    fn probe_and_prune_run_through_the_colima_ssh_adapter() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        let fixture = tempfile::tempdir().expect("fresh private fixture");
        let bin = fixture.path().join("bin");
        fs::create_dir(&bin).expect("private bin");
        let program = bin.join("colima");
        fs::write(&program, FAKE_COLIMA).expect("private adapter");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700))
            .expect("executable private adapter");

        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--ignored",
                "--exact",
                "exec::healthcheck_pidfiles::tests::probe_and_prune_child",
                "--nocapture",
            ])
            .current_dir(fixture.path())
            .env("PATH", &bin)
            .env("FIXTURE", fixture.path())
            .output()
            .expect("run and reap the private proof process");
        assert!(
            output.status.success(),
            "private proof failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let commands = fs::read_to_string(fixture.path().join("commands"))
            .expect("private adapter command receipt");
        let lines: Vec<&str> = commands.lines().filter(|line| !line.is_empty()).collect();
        assert!(
            lines.iter().all(|line| *line == "ssh"
                || *line == "--profile"
                || *line == "effigy"
                || *line == "--"
                || *line == "sudo"
                || *line == "sh"
                || *line == "-c"
                || line.contains("root=")),
            "only the bounded `colima ssh --profile effigy -- sudo sh -c <script>` adapter may run: {commands}"
        );
        assert!(commands.contains("ssh"), "got: {commands}");
        assert!(commands.contains("sudo"), "got: {commands}");
        assert!(commands.contains(HEALTHCHECK_TASK_ROOT), "got: {commands}");
        assert!(
            commands.contains(&format!(
                "-name \"$pattern\" -mmin +{HEALTHCHECK_PIDFILE_STALE_MINUTES} -delete"
            )),
            "the only deletion is the pattern-scoped find: {commands}"
        );
        assert_eq!(
            commands.matches("-delete").count(),
            1,
            "one deletion, no retries of it: {commands}"
        );
        assert!(!commands.contains(" rm "), "no rm: {commands}");
    }

    #[test]
    #[ignore]
    fn probe_and_prune_child() {
        let fixture = PathBuf::from(std::env::var("FIXTURE").expect("fixture"));
        let deadline = Instant::now() + Duration::from_secs(30);
        let pressure = inspect_run_inode_pressure("effigy", &fixture, deadline)
            .expect("probe through the production adapter");
        assert_eq!(pressure.inode_used, 270_087);
        assert_eq!(pressure.stale_pid_files, 268_182);
        assert!(pressure.is_pressured());

        std::env::set_var("FAKE_PRUNE", "1");
        let prune = prune_stale_health_check_pidfiles("effigy", &fixture, deadline)
            .expect("prune through the production adapter");
        assert_eq!(prune.before, 269_091);
        assert_eq!(prune.after, 907);
        assert_eq!(prune.deleted(), 268_184);
    }

    #[test]
    fn a_partial_probe_report_is_not_evidence() {
        assert!(parse_run_inode_pressure("inodes 819200 269738\npidfiles 268740\n").is_none());
        assert!(parse_run_inode_pressure("garbage\n").is_none());
        assert!(parse_health_check_pidfile_prune("before 10\n").is_none());
    }

    #[test]
    fn pressured_profiles_name_both_remediations() {
        let full = RunInodePressure {
            inode_total: 819_200,
            inode_used: 270_087,
            pid_files: 269_082,
            stale_pid_files: 268_182,
        };
        assert_eq!(full.inode_use_percent(), 32);
        assert!(full.is_pressured());
        let warning = full.warning("effigy").expect("pressured profile warns");
        assert!(warning.contains("colima profile `effigy`"));
        assert!(warning.contains("nerdctl 2.3.1 leaks one per health check"));
        assert!(warning.contains("nerdctl 2.4.1 fixes it"));
        assert!(
            warning.contains("effigy container profile prune-health-pid-files --profile effigy")
        );
        assert!(warning.contains(HEALTHCHECK_TASK_ROOT));
        assert!(warning.contains("Do not delete anything else"));

        let nearly_full = RunInodePressure {
            inode_total: 100,
            inode_used: 80,
            pid_files: 1,
            stale_pid_files: 0,
        };
        assert!(nearly_full.is_pressured());

        let leak_only = RunInodePressure {
            inode_total: 819_200,
            inode_used: 1_000,
            pid_files: 20_000,
            stale_pid_files: 19_000,
        };
        assert!(leak_only.is_pressured());
    }

    #[test]
    fn prune_counts_are_before_minus_after() {
        let prune = parse_health_check_pidfile_prune("before 269091\nafter 907\n")
            .expect("complete prune report");
        assert_eq!(
            prune,
            HealthCheckPidFilePrune {
                before: 269_091,
                after: 907,
            }
        );
        assert_eq!(prune.deleted(), 268_184);
        assert_eq!(
            prune.summary(),
            "removed 268184 stale nerdctl health-check pid file(s); 907 remain"
        );

        // A concurrent health check may add a file the prune must keep: the
        // count never goes negative.
        let concurrent = HealthCheckPidFilePrune {
            before: 5,
            after: 6,
        };
        assert_eq!(concurrent.deleted(), 0);
    }
}
