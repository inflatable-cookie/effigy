//! The per-service steps one Compose invocation shows.
//!
//! Every step here names what is happening and ends with one line that says
//! what now holds, and how long it took. On a terminal the steps draw as
//! asynchronous spinners; under `--json`, a pipe or CI they draw nothing
//! (effigy#312's step API decides that), so a machine never reads a step.
//!
//! One driver serves both directions: `container up` feeds it the runtime's
//! pull/build/create/start lines, `container down` its stopping/removing lines,
//! so the label changes as the runtime works instead of after it finishes.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::compose_progress::ComposeStepEvent;
use effigy_ui::steps::{ProgressSource, Step};

/// A step that remembers when it started and whether it has ended.
#[derive(Debug)]
pub struct TimedStep {
    step: Step,
    /// The live state a live step renders from; `None` for a legacy label.
    source: Option<ProgressSource>,
    started: Instant,
    finished: Cell<bool>,
}

impl TimedStep {
    /// A step whose frames come from `source`: the current sub-step, counts,
    /// latest event and elapsed time (effigy#460, Q-017).
    pub fn live(source: ProgressSource) -> Self {
        Self {
            step: effigy_ui::steps::live_step(source.clone()),
            source: Some(source),
            started: Instant::now(),
            finished: Cell::new(false),
        }
    }

    #[cfg_attr(
        effigy_feedback_lint,
        deprecated(note = "feedback legacy spinner: migrate to a live progress source")
    )]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            step: effigy_ui::steps::step(&label.into()),
            source: None,
            started: Instant::now(),
            finished: Cell::new(false),
        }
    }

    /// The label a person last saw for this step.
    pub fn label(&self) -> String {
        match &self.source {
            Some(source) => source.frame_at(self.elapsed()),
            None => self.step.label(),
        }
    }

    pub fn set_label(&self, label: impl Into<String>) {
        match &self.source {
            Some(source) => source.update(label.into(), None, None),
            None => self.step.set_label(&label.into()),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Whether this step already has its one ending line.
    pub fn is_finished(&self) -> bool {
        self.finished.get()
    }

    /// Finished successfully: `summary (1.2s)`.
    pub fn succeed(&self, summary: impl AsRef<str>) {
        if self.finished.replace(true) {
            return;
        }
        self.step
            .succeed(&with_duration(summary.as_ref(), self.elapsed()));
    }

    /// Failed: `summary (1.2s)`.
    pub fn fail(&self, summary: impl AsRef<str>) {
        if self.finished.replace(true) {
            return;
        }
        self.step
            .fail(&with_duration(summary.as_ref(), self.elapsed()));
    }

    /// Finished without a line, because the step resolved into other output.
    pub fn finish_clear(&self) {
        if self.finished.replace(true) {
            return;
        }
        self.step.finish_clear();
    }
}

/// A step that moves with the runtime's own progress lines. Between two lines
/// the frame names the wait it is in, with elapsed time, so a long pull or
/// start never shows one fixed label for its whole duration.
fn runtime_step(label: String) -> TimedStep {
    let source = ProgressSource::waiting_for("the runtime's next report");
    source.update(label, None, None);
    TimedStep::live(source)
}

/// A one-off step for a phase that has not finished yet.
#[cfg_attr(
    effigy_feedback_lint,
    deprecated(note = "feedback legacy spinner: migrate to a live progress source")
)]
pub fn phase(label: impl Into<String>) -> TimedStep {
    TimedStep::new(label)
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
#[derive(Debug)]
pub struct ComposeSteps {
    project: String,
    steps: BTreeMap<String, (TimedStep, String)>,
}

impl ComposeSteps {
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            steps: BTreeMap::new(),
        }
    }

    /// One line of runtime output, as a step move.
    pub fn observe(&mut self, event: &ComposeStepEvent) {
        let key = self.key(event);
        let subject = event.subject().to_owned();
        let project = self.project.clone();
        let entry = self
            .steps
            .entry(key)
            .or_insert_with(|| (runtime_step(event.label(&project)), subject.clone()));
        let step = &entry.0;
        if step.is_finished() {
            return;
        }
        match event.done_summary(&self.project) {
            Some(summary) => step.succeed(summary),
            None => step.set_label(event.label(&self.project)),
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
    pub fn finish_unfinished(&self, ok: bool, summary: &str) {
        for (step, _) in self.steps.values() {
            if step.is_finished() {
                continue;
            }
            if ok {
                step.succeed(summary);
            } else {
                step.fail(summary);
            }
        }
    }

    /// Finish every container step the runtime did not report, from what the
    /// post-invocation inventory still holds.
    ///
    /// A teardown that the runtime never narrated is still a teardown: a
    /// container the inventory no longer lists is gone, and one it still lists
    /// is named as present rather than assumed stopped. `remaining` is `None`
    /// when the inventory could not be read, and then the step only claims
    /// what the runtime was asked to do.
    pub fn finish_unfinished_teardown(&self, remaining: Option<&[String]>) {
        for (key, (step, subject)) in &self.steps {
            if step.is_finished() || !key.starts_with("container:") {
                continue;
            }
            let service = key.strip_prefix("container:").unwrap_or(key);
            let duration = effigy_ui::human::human_duration(step.elapsed());
            match remaining {
                Some(remaining) if remaining.iter().any(|held| held == subject) => step.fail(format!(
                    "{service} is still present after the teardown ({duration}); inspect it with `effigy container status` and `effigy container logs --service {service}`"
                )),
                Some(_) => step.succeed(format!("stopped and removed {service} ({duration})")),
                None => step.succeed(format!(
                    "asked the runtime to stop and remove {service} ({duration}); `effigy container status` confirms it"
                )),
            }
        }
    }

    /// How many subjects this invocation moved.
    pub fn touched(&self) -> usize {
        self.steps.len()
    }
}

impl Drop for ComposeSteps {
    fn drop(&mut self) {
        // A step that is never finished leaves a spinner on the terminal.
        for (step, _) in self.steps.values() {
            step.finish_clear();
        }
    }
}

#[cfg(test)]
#[path = "compose_steps/tests.rs"]
mod tests;
