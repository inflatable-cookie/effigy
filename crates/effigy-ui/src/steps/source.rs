//! The operation's shared progress state.
//!
//! A producer publishes one [`ProgressFrame`] per phase under one lock; a step
//! renders the concise or detailed form of whatever is current. Both
//! renderings are pure functions of a frame and an elapsed time, so a step's
//! frames can be asserted without a terminal.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Shared, operation-specific progress state. The awaited object or probe is
/// always present, even before the operation has emitted its first event.
#[derive(Debug, Clone)]
pub struct ProgressSource {
    state: Arc<Mutex<ProgressFrame>>,
    started: Instant,
}

/// A phase's complete presentation, built inside a source update.
/// An omitted wait keeps the operation's current awaited object; all other
/// evidence is replaced, so a new phase cannot inherit an old blocker.
#[derive(Debug, Default)]
pub struct ProgressFrame {
    awaiting: Option<String>,
    sub_step: Option<String>,
    count: Option<(u64, u64)>,
    event: Option<String>,
    concise_label: Option<String>,
    concise_event: Option<String>,
}

impl ProgressFrame {
    pub fn new(sub_step: impl Into<String>) -> Self {
        Self {
            sub_step: Some(sub_step.into()),
            ..Self::default()
        }
    }

    pub fn count(mut self, count: Option<(u64, u64)>) -> Self {
        self.count = count;
        self
    }

    pub fn event(mut self, event: Option<String>) -> Self {
        self.concise_event = event.clone();
        self.event = event;
        self
    }

    pub fn waiting_for(mut self, awaiting: impl Into<String>) -> Self {
        self.awaiting = Some(awaiting.into());
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.concise_label = Some(label.into());
        self
    }

    pub fn event_label(mut self, label: impl Into<String>) -> Self {
        self.concise_event = Some(label.into());
        self
    }

    /// Carry a still-current observation explicitly while the same wait advances.
    pub fn observations_from(mut self, previous: &Self) -> Self {
        self.event = previous.event.clone();
        self.concise_event = previous.concise_event.clone();
        self
    }

    pub fn detail_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self.concise_event = None;
        self
    }
}

impl ProgressSource {
    /// Name the exact object or probe being awaited.
    pub fn waiting_for(awaiting: impl Into<String>) -> Self {
        Self::new(ProgressFrame::default().waiting_for(awaiting))
    }

    pub fn new(frame: ProgressFrame) -> Self {
        Self {
            state: Arc::new(Mutex::new(frame)),
            started: Instant::now(),
        }
    }

    /// Build and replace the phase, including concise and detailed evidence,
    /// under one lock. The builder must not call back into this source.
    pub fn update(&self, build: impl FnOnce(&ProgressFrame) -> ProgressFrame) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut frame = build(&state);
        if frame.awaiting.is_none() {
            frame.awaiting = state.awaiting.take();
        }
        *state = frame;
    }

    /// Publish an observation without changing the phase or its count.
    pub fn event(&self, event: impl Into<String>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let event = event.into();
        state.concise_event = Some(event.clone());
        state.event = Some(event);
    }

    /// Publish a blocker and its short terminal wording in one observation.
    pub fn blocker(&self, event: impl Into<String>, label: impl Into<String>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.event = Some(event.into());
        state.concise_event = Some(label.into());
    }

    /// Keep diagnostic evidence for verbose output without crowding the spinner.
    pub fn detail_event(&self, event: impl Into<String>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.event = Some(event.into());
        state.concise_event = None;
    }

    /// A concise terminal frame. Time earns a place after two seconds; a
    /// sub-step replaces the wait rather than repeating it.
    pub fn frame_at(&self, elapsed: Duration) -> String {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let awaiting = state.awaiting.as_deref().unwrap_or_default();
        let mut label = state
            .concise_label
            .clone()
            .or_else(|| state.sub_step.clone())
            .unwrap_or_else(|| format!("Waiting for {}", awaiting));
        if let Some((done, total)) = state.count {
            label.push_str(&format!(" ({done}/{total})"));
        }
        if let Some(event) = &state.concise_event {
            label.push_str(&format!(" · {event}"));
        }
        if elapsed >= Duration::from_secs(2) {
            label.push_str(&format!(" · {}s", elapsed.as_secs()));
        }
        label
    }

    /// What the step ends with when it succeeds: the sub-step last set, or
    /// the awaited object if the operation never named a sub-step.
    pub fn summary(&self) -> String {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .sub_step
            .clone()
            .unwrap_or_else(|| state.awaiting.clone().unwrap_or_default())
    }

    /// The operator-visible label at `elapsed`. Tests use this as a fake clock.
    ///
    /// A frame always names what is still awaited once its sub-step is known,
    /// and the last event is added on top: an event says what just happened,
    /// never what the operation is waiting for, so publishing one cannot hide
    /// the awaited object for the rest of the phase (Q-017).
    pub fn detailed_frame_at(&self, elapsed: Duration) -> String {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let awaiting = state.awaiting.as_deref().unwrap_or_default();
        let mut label = state
            .sub_step
            .clone()
            .unwrap_or_else(|| format!("waiting for {}", awaiting));
        if let Some((done, total)) = state.count {
            label.push_str(&format!(" ({done}/{total})"));
        }
        if state.sub_step.is_some() && !awaiting.is_empty() {
            label.push_str(&format!("; waiting for {}", awaiting));
        }
        if let Some(event) = &state.event {
            label.push_str(&format!("; last: {event}"));
        }
        label.push_str(&format!("; {}s elapsed", elapsed.as_secs()));
        label
    }

    pub(in crate::steps) fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub(in crate::steps) fn frame(&self) -> String {
        self.frame_at(self.started.elapsed())
    }
}
