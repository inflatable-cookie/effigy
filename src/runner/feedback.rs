//! Live steps for the runner's command planes.
//!
//! A step names what is happening while it happens, and ends with one line:
//! what now holds, or what is wrong. On a terminal that is an asynchronous
//! spinner on stderr; `--json`, piped and CI output stay silent. See
//! `docs/knowledge/architecture/product-guardrails.md` ("Human-Facing
//! Feedback").
//!
//! ```no_run
//! let status = crate::runner::feedback::live_progress(
//!     "reading the gateway state",
//!     "the gateway identity record",
//!     |progress| {
//!         progress.update("checking whether the gateway is running", None, None);
//!         resolve_status()
//!     },
//! )?;
//! ```
//!
//! The closure moves the action and the awaited object as the work moves on.
//! When it returns, the step ends with the action it last set.

use effigy_ui::steps::{live_step, ProgressSource};

use super::error::RunnerError;

/// Run one step driven by an operation's live state. `action` names what is
/// happening now and `awaiting` the exact object or probe the wait is on. The
/// closure moves them as events arrive; the step ends with the action it last
/// set, or with the refusal's sentence.
pub(in crate::runner) fn live_progress<T>(
    action: &str,
    awaiting: &str,
    work: impl FnOnce(&ProgressSource) -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    let progress = ProgressSource::waiting_for(awaiting);
    progress.update(action, None, None);
    let step = live_step(progress.clone());
    match work(&progress) {
        Ok(value) => {
            step.succeed(&progress.summary());
            Ok(value)
        }
        Err(error) => {
            step.fail(error.human_error().what_failed());
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_progress_step_ends_with_the_action_it_last_set() {
        let (value, lines) = effigy_ui::steps::capture_lines(|| {
            live_progress(
                "reading the gateway state",
                "the gateway identity record",
                |progress| {
                    progress.update("the gateway is running on pid 42", None, None);
                    Ok(42)
                },
            )
        });
        assert_eq!(value.expect("a read"), 42);
        assert_eq!(lines, vec!["✓ the gateway is running on pid 42"]);
    }

    #[test]
    fn a_live_progress_refusal_ends_with_the_sentence_a_person_reads() {
        let (refused, lines) = effigy_ui::steps::capture_lines(|| {
            live_progress::<()>(
                "starting the gateway daemon",
                "the gateway daemon process to start",
                |_| {
                    Err(RunnerError::task_invocation(
                        "gateway port 443 is already in use by pid 77; stop that process, then run `effigy gateway up`",
                    ))
                },
            )
        });
        assert!(refused.is_err());
        assert_eq!(
            lines,
            vec![
                "✗ gateway port 443 is already in use by pid 77; stop that process, then run \
                 `effigy gateway up`"
            ]
        );
    }
}
