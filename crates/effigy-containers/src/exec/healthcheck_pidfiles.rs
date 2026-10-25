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
//! task, so it prunes exactly the leaked files — matching the exact
//! `health-check-*.pid` name, directly inside a containerd runtime-v2 task
//! directory, older than [`HEALTHCHECK_PIDFILE_STALE_MINUTES`] — on
//! `container up` and on request.

use std::path::Path;
use std::time::Instant;

use super::implementation::ContainerExecError;
use super::process::run_command_capture_allow_failure_with_deadline;

/// The containerd runtime-v2 task root inside the Colima VM. Each container's
/// exec pid files sit directly below `<root>/<namespace>/<container>/`.
pub const HEALTHCHECK_TASK_ROOT: &str = "/run/containerd/io.containerd.runtime.v2.task";

/// The only file name pattern the prune may ever match.
pub const HEALTHCHECK_PIDFILE_PATTERN: &str = "health-check-*.pid";

/// A pid file younger than this may belong to a health check still in flight,
/// so the prune never touches it.
pub const HEALTHCHECK_PIDFILE_STALE_MINUTES: u64 = 10;

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
    let args = [
        "ssh",
        "--profile",
        profile,
        "--",
        "sudo",
        "sh",
        "-c",
        script.as_str(),
    ];
    let output = run_command_capture_allow_failure_with_deadline(
        repo_root,
        "colima",
        &args,
        "prune stale nerdctl health-check pid files",
        Some(deadline),
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
        parse_health_check_pidfile_prune, prune_stale_health_check_pidfiles,
        prune_stale_health_check_pidfiles_script, HealthCheckPidFilePrune,
        HEALTHCHECK_PIDFILE_STALE_MINUTES, HEALTHCHECK_TASK_ROOT,
    };

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

    /// The private adapter script: it records every argv vector it is handed
    /// and answers the prune with the counts from the fixture.
    const FAKE_COLIMA: &str = r#"#!/bin/sh
printf '%s\n' "$@" >> "$FIXTURE/commands"
printf '\n' >> "$FIXTURE/commands"
printf 'before 269091\nafter 907\n'
"#;

    #[test]
    fn prune_runs_through_the_colima_ssh_adapter() {
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
                "exec::healthcheck_pidfiles::tests::prune_child",
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
    fn prune_child() {
        let fixture = PathBuf::from(std::env::var("FIXTURE").expect("fixture"));
        let deadline = Instant::now() + Duration::from_secs(30);
        let prune = prune_stale_health_check_pidfiles("effigy", &fixture, deadline)
            .expect("prune through the production adapter");
        assert_eq!(prune.before, 269_091);
        assert_eq!(prune.after, 907);
        assert_eq!(prune.deleted(), 268_184);
    }

    #[test]
    fn a_partial_prune_report_is_not_evidence() {
        assert!(parse_health_check_pidfile_prune("before 10\n").is_none());
        assert!(parse_health_check_pidfile_prune("garbage\n").is_none());
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
