//! How a source acquisition says what it is doing, as it does it.
//!
//! Bundle and catalog acquisition can clone or snapshot for seconds at a time.
//! The engine reports one start event and one finish event per source through a
//! [`SourceProgress`] hook; the command plane decides how to draw it
//! (`effigy_ui::steps` draws an asynchronous spinner on a terminal and nothing
//! in `--json`). The engine itself never writes to a terminal.

use std::fmt;
use std::sync::Arc;

/// One source's acquisition progress, in the words a person reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceProgressEvent {
    /// A source is about to be cloned, fetched or snapshotted.
    Started {
        /// The source's handle: `bundle` or `catalog:<name>`.
        source: String,
        /// What is being fetched, short enough for a spinner label: the
        /// bundle's own name, the catalog handle or the source's base name.
        subject: String,
        /// The revision pinned for this acquisition: a ref, a commit, a
        /// content digest or `path`.
        revision: String,
    },
    /// The source finished: acquired, or kept at its current pin.
    Finished {
        /// The source's handle: `bundle` or `catalog:<name>`.
        source: String,
        /// The subject named by the matching [`Self::Started`] event.
        subject: String,
        /// Why this source was not updated, when it was not. `None` means the
        /// source was acquired.
        kept: Option<String>,
        /// What went wrong, when it did. The source's pin, if any, is kept.
        failure: Option<String>,
    },
}

/// Where acquisition progress goes. The default is silent, which is what
/// `--json`, piped output and library callers get.
#[derive(Clone, Default)]
pub struct SourceProgress(Option<Arc<dyn Fn(SourceProgressEvent) + Send + Sync>>);

impl SourceProgress {
    /// Report nothing.
    pub fn silent() -> Self {
        Self(None)
    }

    /// Report every event to `report`.
    pub fn to(report: impl Fn(SourceProgressEvent) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(report)))
    }

    pub(crate) fn emit(&self, event: SourceProgressEvent) {
        if let Some(report) = &self.0 {
            report(event);
        }
    }
}

impl fmt::Debug for SourceProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceProgress")
            .field("reporting", &self.0.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn silent_progress_reports_nothing() {
        // Silence is the default, so a library caller draws nothing.
        let progress = SourceProgress::silent();
        progress.emit(SourceProgressEvent::Started {
            source: "bundle".to_owned(),
            subject: "decodelabs".to_owned(),
            revision: "main".to_owned(),
        });
        assert!(matches!(SourceProgress::default(), SourceProgress(None)));
    }

    #[test]
    fn a_hook_sees_each_event_once() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let progress = SourceProgress::to(move |event| {
            sink.lock().expect("progress log").push(event);
        });
        progress.emit(SourceProgressEvent::Started {
            source: "bundle".to_owned(),
            subject: "decodelabs".to_owned(),
            revision: "main".to_owned(),
        });
        progress.emit(SourceProgressEvent::Finished {
            source: "bundle".to_owned(),
            subject: "decodelabs".to_owned(),
            kept: None,
            failure: None,
        });
        assert_eq!(seen.lock().expect("progress log").len(), 2);
    }

    #[test]
    fn debug_names_that_progress_is_reported_not_the_closure() {
        let progress = SourceProgress::to(|_| {});
        assert_eq!(
            format!("{progress:?}"),
            "SourceProgress { reporting: true }"
        );
    }
}
