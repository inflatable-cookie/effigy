//! The host's own load, observed when a bounded runtime call ends.
//!
//! Under a saturated host a bounded Colima/nerdctl call can time out before the
//! backend answers. That is evidence about the machine, not about the service:
//! an owned inspect that timed out proves nothing about the container's state,
//! and a readiness probe that never got to run is not a failed service. Effigy
//! reports the observed host load with those timeouts, and lets a saturated
//! host retry a bounded call once more, so a busy machine is named as busy
//! instead of a healthy service being blamed (effigy#291, papercut 8064c5b9).
//!
//! The observation is read-only and local: `/proc/loadavg` on Linux, and
//! `sysctl -n vm.loadavg` on macOS and the BSDs. Nothing here starts a
//! container, a VM or a runtime command.

use effigy_kernel::{
    ctx::{CancelToken, EffectClass},
    custody::EnvValue,
    observe::Observed,
    probe::{ProbeCommand, ProbeRuntime},
    supervise::Executable,
};
use std::path::Path;
use std::time::Duration;

/// A point-in-time load observation for this host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostLoad {
    /// The one-minute load average, in runnable tasks.
    pub one_minute: f64,
    /// Logical CPUs the kernel reports.
    pub cpus: usize,
}

impl HostLoad {
    /// Observe the host now. `None` means the platform did not report a
    /// usable measurement; callers must never read that as "not loaded".
    pub fn observe() -> Option<Self> {
        let one_minute = read_one_minute_load()?;
        let cpus = std::thread::available_parallelism().ok()?.get();
        Self::measured(one_minute, cpus)
    }

    /// Build an observation from explicit inputs. Deterministic proofs use
    /// this; implausible measurements are refused rather than reported.
    pub fn measured(one_minute: f64, cpus: usize) -> Option<Self> {
        if cpus == 0 || !one_minute.is_finite() || one_minute < 0.0 {
            return None;
        }
        Some(Self { one_minute, cpus })
    }

    /// One runnable task per CPU, averaged over a minute: the host could not
    /// keep up with the work it was given. This is the threshold at which a
    /// bounded runtime timeout is evidence about the host.
    pub fn saturated(&self) -> bool {
        self.one_minute >= self.cpus as f64
    }

    /// `load average 21.40 over 18 CPU(s)`, for an operator-facing error.
    pub fn describe(&self) -> String {
        format!(
            "load average {:.2} over {} CPU(s)",
            self.one_minute, self.cpus
        )
    }

    /// `the host was overloaded (load average 40.00 over 18 CPU(s))`, or
    /// `None` when the host was keeping up.
    pub fn overload_phrase(&self) -> Option<String> {
        if !self.saturated() {
            return None;
        }
        Some(format!("the host was overloaded ({})", self.describe()))
    }

    /// The sentence a bounded-timeout error adds when this observation proves
    /// the host was saturated, and `None` when it does not. The text says what
    /// the timeout does *not* prove, so a loaded host never reads as a failed
    /// service.
    pub fn overload_note(&self) -> Option<String> {
        let phrase = self.overload_phrase()?;
        Some(format!(
            "[effigy] {phrase}, so this bounded call did not finish in time. \
             That is a host-load observation, not a service failure: the service's state is unproven \
             until a bounded call returns."
        ))
    }
}

/// Parse the one-minute load from either `/proc/loadavg` (`4.02 6.01 8.35 …`)
/// or `sysctl -n vm.loadavg` (`{ 4.02 6.01 8.35 }`).
pub fn parse_one_minute_load(raw: &str) -> Option<f64> {
    let trimmed = raw.trim().trim_start_matches('{').trim_end_matches('}');
    let first = trimmed.split_whitespace().next()?;
    first.parse::<f64>().ok()
}

fn read_one_minute_load() -> Option<f64> {
    if let Ok(raw) = std::fs::read_to_string("/proc/loadavg") {
        return parse_one_minute_load(&raw);
    }
    let budget = Duration::from_secs(2);
    let op = crate::environment::operation(EffectClass::Observe, CancelToken::default(), budget);
    let executable = Executable::current().ok()?;
    let runtime = ProbeRuntime::install(executable).ok()?;
    let cx = op.observe().ok()?.with_probes(runtime);
    let command = ProbeCommand {
        argv: vec![
            crate::compose::resolve_host_cli_program("sysctl"),
            "-n".into(),
            "vm.loadavg".into(),
        ],
        cwd: Path::new(".").canonicalize().ok()?,
        env: std::env::vars_os()
            .map(|(key, value)| (key, EnvValue::Plain(value)))
            .collect(),
        deadline: budget,
        max_output: 4096,
    };
    let Observed::Verified(output, _) = cx.probe_process(command) else {
        return None;
    };
    if output.exit.code != 0 || output.truncated || !output.warnings.is_empty() {
        return None;
    }
    parse_one_minute_load(&String::from_utf8_lossy(&output.output))
}

#[cfg(test)]
mod tests {
    use super::{parse_one_minute_load, HostLoad};

    #[test]
    fn parses_linux_and_bsd_load_formats() {
        assert_eq!(
            parse_one_minute_load("4.02 6.01 8.35 2/1234 5678\n"),
            Some(4.02)
        );
        assert_eq!(
            parse_one_minute_load("{ 21.40 19.02 16.71 }\n"),
            Some(21.40)
        );
        assert_eq!(parse_one_minute_load("  \n"), None);
        assert_eq!(parse_one_minute_load("not-a-load"), None);
    }

    #[test]
    fn saturation_is_one_runnable_task_per_cpu() {
        let idle = HostLoad::measured(4.0, 18).expect("plausible observation");
        assert!(!idle.saturated());
        assert!(idle.overload_note().is_none());

        let saturated = HostLoad::measured(20.5, 18).expect("plausible observation");
        assert!(saturated.saturated());
        assert_eq!(
            saturated.overload_phrase().as_deref(),
            Some("the host was overloaded (load average 20.50 over 18 CPU(s))")
        );
        let note = saturated
            .overload_note()
            .expect("saturated host has a note");
        assert!(note.contains("load average 20.50 over 18 CPU(s)"));
        assert!(note.contains("the host was overloaded"));
        assert!(note.contains("not a service failure"));
        assert!(note.contains("unproven"));
    }

    #[test]
    fn implausible_measurements_are_refused() {
        assert!(HostLoad::measured(1.0, 0).is_none());
        assert!(HostLoad::measured(f64::NAN, 8).is_none());
        assert!(HostLoad::measured(-1.0, 8).is_none());
    }
}
