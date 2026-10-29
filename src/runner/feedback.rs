//! Live steps for the runner's command planes.
//!
//! A step names what is happening while it happens, and ends with one line:
//! what now holds, or what is wrong. On a terminal that is an asynchronous
//! spinner on stderr; `--json`, piped and CI output stay silent. See
//! `docs/knowledge/architecture/product-guardrails.md` ("Human-Facing
//! Feedback").
//!
//! ```no_run
//! let status = crate::runner::feedback::live("reading gateway state", |step| {
//!     step.set_label("checking whether the gateway is running");
//!     resolve_status()
//! })?;
//! ```
//!
//! The closure may move the label as the work moves on. When it returns, the
//! step ends with the label it last set, unless the closure already finished
//! it with a summary naming what now holds.

use effigy_ui::steps::{step, Step};

use super::error::RunnerError;

/// Run one labelled step. A refusal ends the step with the sentence a person
/// reads for that failure, not an error variant or a `Debug` tree.
#[cfg_attr(
    effigy_feedback_lint,
    deprecated(note = "feedback legacy spinner: migrate to a live progress source")
)]
pub(in crate::runner) fn live<T>(
    label: &str,
    work: impl FnOnce(&Step) -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    let step = step(label);
    match work(&step) {
        Ok(value) => {
            step.succeed("");
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
    fn a_step_that_succeeds_ends_with_the_label_it_last_set() {
        let (value, lines) = effigy_ui::steps::capture_lines(|| {
            live("reading gateway state", |step| {
                step.set_label("the gateway is running on pid 42");
                Ok(42)
            })
        });
        assert_eq!(value.expect("a read"), 42);
        assert_eq!(lines, vec!["✓ the gateway is running on pid 42"]);
    }

    #[test]
    fn a_refusal_ends_the_step_with_the_sentence_a_person_reads() {
        let (refused, lines) = effigy_ui::steps::capture_lines(|| {
            live::<()>("starting the gateway", |_| {
                Err(RunnerError::task_invocation(
                    "gateway port 443 is already in use by pid 77; stop that process, then run `effigy gateway up`",
                ))
            })
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
