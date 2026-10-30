//! Doctor's scan progress, drawn through the one live-step API so JSON and
//! piped output stay silent without a second terminal check here.
//!
//! Each visible check gets one step driven by a `ProgressSource`: the exact
//! probe and elapsed wait are always present. The scan crate emits no
//! per-file events, so a long check names that probe rather than a second
//! canned label. A known check count or shared-inventory event is attached
//! when the caller can observe it. Each step ends with one ✓ or ✗ line that
//! names the check.

use effigy_ui::steps::{self, ProgressSource, Step};

use crate::{DoctorFinding, DoctorSeverity};

/// A check's step labels. `active` names the work while it runs; `subject`
/// names the check on its one ending line; `probe` is the exact wait the
/// live source names when no further event is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScanLabel {
    pub(super) active: &'static str,
    pub(super) subject: &'static str,
    pub(super) probe: &'static str,
}

pub(super) const GOD_FILES: ScanLabel = ScanLabel {
    active: "scanning for god files",
    subject: "god files",
    probe: "god files scan",
};

pub(super) const DUPLICATE_BLOCKS: ScanLabel = ScanLabel {
    active: "scanning for duplicate blocks",
    subject: "duplicate blocks",
    probe: "duplicate blocks scan",
};

pub(super) const HEALTH_TASK: ScanLabel = ScanLabel {
    active: "running health task",
    subject: "health task",
    probe: "health --json",
};

/// How a check ended, for its one terminal line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScanOutcome {
    /// No errors; `warnings` counts the warnings the check raised.
    Clear { warnings: usize },
    /// The check raised this many errors. Its findings sit in the report.
    Failed { errors: usize },
    /// The check did not finish, for the stated reason.
    Stopped(&'static str),
}

impl ScanOutcome {
    /// Count the findings a check raised while it ran.
    pub(super) fn from_findings<'a>(findings: impl IntoIterator<Item = &'a DoctorFinding>) -> Self {
        let mut errors = 0usize;
        let mut warnings = 0usize;
        for finding in findings {
            match finding.severity {
                DoctorSeverity::Error => errors += 1,
                DoctorSeverity::Warning => warnings += 1,
                DoctorSeverity::Info => {}
            }
        }
        if errors > 0 {
            Self::Failed { errors }
        } else {
            Self::Clear { warnings }
        }
    }

    fn is_failure(&self) -> bool {
        !matches!(self, Self::Clear { .. })
    }

    fn line(&self, subject: &str) -> String {
        match self {
            Self::Clear { warnings: 0 } => format!("{subject}: no problems found"),
            Self::Clear { warnings } => {
                format!("{subject}: no errors, {}", plural(*warnings, "warning"))
            }
            Self::Failed { errors } => format!(
                "{subject}: {}, see the findings below",
                plural(*errors, "error")
            ),
            Self::Stopped(reason) => format!("{subject}: {reason}"),
        }
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

struct ActiveScan {
    label: ScanLabel,
    #[cfg(test)]
    source: ProgressSource,
    step: Step,
}

pub(super) struct DoctorProgressReporter {
    active: Vec<ActiveScan>,
}

impl DoctorProgressReporter {
    pub(super) fn new(output_json: bool) -> Option<Self> {
        if output_json {
            return None;
        }
        Some(Self { active: Vec::new() })
    }

    pub(super) fn start_scan(&mut self, label: ScanLabel) {
        self.start_scan_with(label, None, None);
    }

    /// Start a check with a known count and the last observable event. The
    /// scan crate has no per-file hook, so `event` is the shared inventory or
    /// exact probe the caller can name.
    pub(super) fn start_scan_with(
        &mut self,
        label: ScanLabel,
        count: Option<(u64, u64)>,
        event: Option<String>,
    ) {
        let source = ProgressSource::waiting_for(label.probe);
        source.update(label.active, count, event);
        #[cfg(test)]
        let stored = source.clone();
        let step = steps::live_step(source);
        self.active.push(ActiveScan {
            label,
            #[cfg(test)]
            source: stored,
            step,
        });
    }

    /// The operator-visible label at `elapsed` for a started check. Tests use
    /// this as a fake clock.
    #[cfg(test)]
    pub(super) fn live_frame(
        &self,
        label: ScanLabel,
        elapsed: std::time::Duration,
    ) -> Option<String> {
        self.active
            .iter()
            .find(|scan| scan.label.subject == label.subject)
            .map(|scan| scan.source.frame_at(elapsed))
    }

    /// End a started check's step with its one line. A check that never
    /// started has no step to end.
    pub(super) fn finish_scan(&mut self, label: ScanLabel, outcome: &ScanOutcome) {
        let Some(index) = self
            .active
            .iter()
            .position(|scan| scan.label.subject == label.subject)
        else {
            return;
        };
        let ActiveScan { step, .. } = self.active.remove(index);
        let line = outcome.line(label.subject);
        if outcome.is_failure() {
            step.fail(&line);
        } else {
            step.succeed(&line);
        }
    }

    /// Clear any step still running when the scan ends without an outcome.
    pub(super) fn finish(&mut self) {
        for scan in self.active.drain(..) {
            scan.step.finish_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn finding(check_id: &str, severity: DoctorSeverity) -> DoctorFinding {
        DoctorFinding {
            check_id: check_id.to_owned(),
            severity,
            evidence: String::new(),
            remediation: String::new(),
            fixable: false,
        }
    }

    #[test]
    fn a_clear_check_ends_with_one_success_line() {
        let ((), lines) = steps::capture_lines(|| {
            let mut reporter = DoctorProgressReporter::new(false).expect("reporter");
            reporter.start_scan(GOD_FILES);
            reporter.finish_scan(GOD_FILES, &ScanOutcome::Clear { warnings: 0 });
            reporter.finish();
        });
        assert_eq!(lines, vec!["✓ god files: no problems found".to_owned()]);
    }

    #[test]
    fn a_failed_check_ends_with_one_failure_line_that_points_at_findings() {
        let ((), lines) = steps::capture_lines(|| {
            let mut reporter = DoctorProgressReporter::new(false).expect("reporter");
            reporter.start_scan(DUPLICATE_BLOCKS);
            reporter.finish_scan(DUPLICATE_BLOCKS, &ScanOutcome::Failed { errors: 2 });
        });
        assert_eq!(
            lines,
            vec!["✗ duplicate blocks: 2 errors, see the findings below".to_owned()]
        );
    }

    #[test]
    fn a_stopped_check_ends_with_its_reason() {
        let ((), lines) = steps::capture_lines(|| {
            let mut reporter = DoctorProgressReporter::new(false).expect("reporter");
            reporter.start_scan(HEALTH_TASK);
            reporter.finish_scan(
                HEALTH_TASK,
                &ScanOutcome::Stopped("stopped at the time budget"),
            );
        });
        assert_eq!(
            lines,
            vec!["✗ health task: stopped at the time budget".to_owned()]
        );
    }

    #[test]
    fn concurrent_checks_each_end_with_their_own_line() {
        let ((), lines) = steps::capture_lines(|| {
            let mut reporter = DoctorProgressReporter::new(false).expect("reporter");
            reporter.start_scan(GOD_FILES);
            reporter.start_scan(DUPLICATE_BLOCKS);
            reporter.finish_scan(DUPLICATE_BLOCKS, &ScanOutcome::Clear { warnings: 1 });
            reporter.finish_scan(GOD_FILES, &ScanOutcome::Failed { errors: 1 });
            reporter.finish();
        });
        assert_eq!(
            lines,
            vec![
                "✓ duplicate blocks: no errors, 1 warning".to_owned(),
                "✗ god files: 1 error, see the findings below".to_owned(),
            ]
        );
    }

    #[test]
    fn a_check_still_running_when_the_scan_ends_writes_no_line() {
        let ((), lines) = steps::capture_lines(|| {
            let mut reporter = DoctorProgressReporter::new(false).expect("reporter");
            reporter.start_scan(GOD_FILES);
            reporter.finish();
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn json_output_has_no_reporter() {
        assert!(DoctorProgressReporter::new(true).is_none());
    }

    #[test]
    fn live_frames_name_the_probe_count_elapsed_and_check_transitions() {
        let ((), lines) = steps::capture_lines(|| {
            let mut reporter = DoctorProgressReporter::new(false).expect("reporter");
            reporter.start_scan_with(
                GOD_FILES,
                Some((1, 3)),
                Some("shared scan inventory".into()),
            );
            assert_eq!(
                reporter
                    .live_frame(GOD_FILES, Duration::from_secs(0))
                    .expect("god files"),
                "scanning for god files (1/3); waiting for god files scan; last: shared scan inventory; 0s elapsed"
            );
            assert_eq!(
                reporter
                    .live_frame(GOD_FILES, Duration::from_secs(4))
                    .expect("god files"),
                "scanning for god files (1/3); waiting for god files scan; last: shared scan inventory; 4s elapsed"
            );

            reporter.finish_scan(GOD_FILES, &ScanOutcome::Clear { warnings: 0 });
            reporter.start_scan_with(
                DUPLICATE_BLOCKS,
                Some((2, 3)),
                Some("shared scan inventory".into()),
            );
            assert_eq!(
                reporter
                    .live_frame(DUPLICATE_BLOCKS, Duration::from_secs(9))
                    .expect("duplicate blocks"),
                "scanning for duplicate blocks (2/3); waiting for duplicate blocks scan; last: shared scan inventory; 9s elapsed"
            );
            reporter.finish_scan(DUPLICATE_BLOCKS, &ScanOutcome::Failed { errors: 2 });

            reporter.start_scan(HEALTH_TASK);
            assert_eq!(
                reporter
                    .live_frame(HEALTH_TASK, Duration::from_secs(3))
                    .expect("health"),
                "running health task; waiting for health --json; 3s elapsed"
            );
            reporter.finish_scan(
                HEALTH_TASK,
                &ScanOutcome::Stopped("stopped at the time budget"),
            );
            assert!(reporter
                .live_frame(GOD_FILES, Duration::from_secs(1))
                .is_none());
            assert!(reporter
                .live_frame(HEALTH_TASK, Duration::from_secs(1))
                .is_none());
        });
        assert_eq!(
            lines,
            vec![
                "✓ god files: no problems found".to_owned(),
                "✗ duplicate blocks: 2 errors, see the findings below".to_owned(),
                "✗ health task: stopped at the time budget".to_owned(),
            ]
        );
    }

    #[test]
    fn a_long_scan_without_events_names_its_exact_probe() {
        let ((), lines) = steps::capture_lines(|| {
            let mut reporter = DoctorProgressReporter::new(false).expect("reporter");
            reporter.start_scan(GOD_FILES);
            assert_eq!(
                reporter
                    .live_frame(GOD_FILES, Duration::from_secs(12))
                    .expect("god files"),
                "scanning for god files; waiting for god files scan; 12s elapsed"
            );
            reporter.finish();
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn outcome_counts_only_the_findings_it_is_given() {
        let findings = [
            finding("god_files", DoctorSeverity::Warning),
            finding("god_files", DoctorSeverity::Info),
        ];
        assert_eq!(
            ScanOutcome::from_findings(findings.iter()),
            ScanOutcome::Clear { warnings: 1 }
        );
        let findings = [
            finding("god_files", DoctorSeverity::Error),
            finding("god_files", DoctorSeverity::Error),
            finding("god_files", DoctorSeverity::Warning),
        ];
        assert_eq!(
            ScanOutcome::from_findings(findings.iter()),
            ScanOutcome::Failed { errors: 2 }
        );
        assert_eq!(
            ScanOutcome::from_findings(std::iter::empty()),
            ScanOutcome::Clear { warnings: 0 }
        );
    }
}
