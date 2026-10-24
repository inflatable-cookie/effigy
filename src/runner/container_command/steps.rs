//! The live steps a container command shows.
//!
//! Every step here names what is happening and ends with one line that says
//! what now holds, and how long it took. On a terminal the steps draw as
//! asynchronous spinners; under `--json`, a pipe or CI they draw nothing
//! (effigy#312's step API decides that), so a machine never reads a step.
//!
//! Per-service steps are driven by the runtime's own progress lines
//! ([`ComposeStepEvent`]), so the label changes as the runtime works instead
//! of after it finishes.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use effigy_containers::exec::ComposeStepEvent;
use effigy_ui::steps::Step;

/// A step that remembers when it started, so its ending names its duration.
#[derive(Debug)]
pub(super) struct TimedStep {
    step: Step,
    started: Instant,
}

impl TimedStep {
    pub(super) fn new(label: impl Into<String>) -> Self {
        Self {
            step: effigy_ui::steps::step(&label.into()),
            started: Instant::now(),
        }
    }

    pub(super) fn set_label(&self, label: impl Into<String>) {
        self.step.set_label(&label.into());
    }

    /// The label a person last saw for this step.
    #[cfg(test)]
    pub(super) fn label(&self) -> String {
        self.step.label()
    }

    pub(super) fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Finished successfully: `summary (1.2s)`.
    pub(super) fn succeed(&self, summary: impl AsRef<str>) {
        self.step
            .succeed(&with_duration(summary.as_ref(), self.elapsed()));
    }

    /// Failed: `summary (1.2s)`.
    pub(super) fn fail(&self, summary: impl AsRef<str>) {
        self.step
            .fail(&with_duration(summary.as_ref(), self.elapsed()));
    }

    /// Finished without a line, because the step resolved into other output.
    pub(super) fn finish_clear(&self) {
        self.step.finish_clear();
    }
}

/// A one-off step for a phase that has not finished yet.
pub(super) fn phase(label: impl Into<String>) -> TimedStep {
    TimedStep::new(label)
}

/// Run one phase under a step, ending it with `summary` or the phase's own
/// failure.
pub(super) fn during<T>(
    label: impl Into<String>,
    summary: impl AsRef<str>,
    work: impl FnOnce() -> Result<T, super::RunnerError>,
) -> Result<T, super::RunnerError> {
    let step = phase(label);
    match work() {
        Ok(value) => {
            step.succeed(summary);
            Ok(value)
        }
        Err(error) => {
            step.fail(format!("{} failed: {}", summary.as_ref(), error));
            Err(error)
        }
    }
}

/// `starting web (1.4s)` — every finished step says how long it ran.
fn with_duration(summary: &str, elapsed: Duration) -> String {
    format!("{summary} ({})", effigy_ui::human::human_duration(elapsed))
}

/// The per-service steps of one Compose invocation.
///
/// One step per image, build and container, created when the runtime first
/// names it and finished when the runtime reports the matching past tense. A
/// step the runtime never finishes is finished by the caller with what the
/// invocation as a whole reported, so no spinner is left on the terminal.
#[derive(Debug, Default)]
pub(super) struct ComposeSteps {
    project: String,
    steps: BTreeMap<String, TimedStep>,
}

impl ComposeSteps {
    pub(super) fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            steps: BTreeMap::new(),
        }
    }

    /// One line of runtime output, as a step move.
    pub(super) fn observe(&mut self, event: &ComposeStepEvent) {
        let key = self.key(event);
        let entry = self
            .steps
            .entry(key)
            .or_insert_with(|| TimedStep::new(event.label(&self.project)));
        // The label always says what is happening now, even when the runtime
        // skips a start (`Created` then `Started`). A label matches a
        // finished step, so a repeated line does not rewrite it.
        let summary = event.done_summary(&self.project);
        match summary {
            Some(summary) => entry.succeed(summary),
            None => entry.set_label(event.label(&self.project)),
        }
    }

    fn key(&self, event: &ComposeStepEvent) -> String {
        let service = event.service(&self.project);
        match event {
            ComposeStepEvent::Pulling { .. } | ComposeStepEvent::Pulled { .. } => {
                format!("image:{service}")
            }
            ComposeStepEvent::Building { .. } | ComposeStepEvent::Built { .. } => {
                format!("build:{service}")
            }
            _ => format!("container:{service}"),
        }
    }

    /// Finish every step the runtime did not report, because the invocation
    /// ended: `summary` explains what the whole invocation did.
    pub(super) fn finish_unfinished(&self, ok: bool, summary: &str) {
        for step in self.steps.values() {
            if ok {
                step.succeed(summary);
            } else {
                step.fail(summary);
            }
        }
    }

    /// How many services this invocation moved.
    pub(super) fn touched(&self) -> usize {
        self.steps.len()
    }
}

impl Drop for ComposeSteps {
    fn drop(&mut self) {
        // A step that is never finished leaves a spinner on the terminal.
        for step in self.steps.values() {
            step.finish_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_is_named_the_way_a_person_reads_it() {
        assert_eq!(
            with_duration("started web", Duration::from_millis(800)),
            "started web (800ms)"
        );
        assert_eq!(
            with_duration("started web", Duration::from_millis(1_400)),
            "started web (1s)"
        );
    }

    /// The step lines a terminal shows for one detached activation: one per
    /// image, build and container, each ending with what now holds. Piped and
    /// `--json` output draws nothing, so the lines are captured instead.
    #[test]
    fn an_activation_draws_one_finished_line_per_service_move() {
        let (_, lines) = effigy_ui::steps::capture_lines(|| {
            let mut steps = ComposeSteps::new("demo-web-dev");
            let events = [
                ComposeStepEvent::Pulling {
                    subject: "postgres:16".to_owned(),
                },
                ComposeStepEvent::Pulled {
                    subject: "postgres:16".to_owned(),
                },
                ComposeStepEvent::Building {
                    subject: "demo-web-dev-web-1".to_owned(),
                },
                ComposeStepEvent::Built {
                    subject: "demo-web-dev-web-1".to_owned(),
                },
                ComposeStepEvent::Creating {
                    subject: "demo-web-dev-web-1".to_owned(),
                },
                ComposeStepEvent::Created {
                    subject: "demo-web-dev-web-1".to_owned(),
                },
                ComposeStepEvent::Starting {
                    subject: "demo-web-dev-web-1".to_owned(),
                },
                ComposeStepEvent::Started {
                    subject: "demo-web-dev-web-1".to_owned(),
                },
                // A container the runtime reports but never started is still
                // finished, so no spinner is left behind.
                ComposeStepEvent::Creating {
                    subject: "demo-web-dev-pma-1".to_owned(),
                },
            ];
            for event in &events {
                steps.observe(event);
            }
            steps.finish_unfinished(true, "the runtime reported the stack as reconciled");
        });
        // Each line names the service and its own duration.
        let rendered = lines.join("\n");
        for expected in [
            "✓ pulled postgres:16",
            "✓ built web",
            "✓ started web",
            "✓ the runtime reported the stack as reconciled",
        ] {
            assert!(
                rendered.contains(expected),
                "{expected} missing from {rendered}"
            );
        }
        assert_eq!(
            lines.len(),
            4,
            "one line per image, build and container: {rendered}"
        );
    }

    #[test]
    fn image_build_and_container_steps_stay_distinct() {
        let mut steps = ComposeSteps::new("demo");
        steps.observe(&ComposeStepEvent::Pulling {
            subject: "postgres:16".to_owned(),
        });
        steps.observe(&ComposeStepEvent::Building {
            subject: "demo-web-1".to_owned(),
        });
        steps.observe(&ComposeStepEvent::Creating {
            subject: "demo-web-1".to_owned(),
        });
        assert_eq!(steps.touched(), 3);
    }
}
