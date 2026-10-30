//! The live steps a container command shows.
//!
//! The per-service driver lives in `effigy_containers::exec` so `container up`
//! and `container down` share one implementation. This module adds the
//! runner-specific phase helper: a step around one piece of work that ends
//! with its own failure sentence.

pub(super) use effigy_containers::exec::{ComposeSteps, TimedStep};
pub(super) use effigy_ui::steps::ProgressSource;

/// Run one phase under a live step driven by `progress`, ending it with
/// `summary` or the phase's own failure. The phase moves `progress` as its
/// sub-steps start, so the frame names the one now running (effigy#460).
pub(super) fn during<T>(
    progress: ProgressSource,
    summary: impl AsRef<str>,
    work: impl FnOnce(&ProgressSource) -> Result<T, super::RunnerError>,
) -> Result<T, super::RunnerError> {
    let step = TimedStep::live(progress.clone());
    match work(&progress) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_phase_ends_with_what_it_was_doing_and_why() {
        let (_, lines) = effigy_ui::steps::capture_lines(|| {
            during(
                ProgressSource::waiting_for("the route registration"),
                "routes registered",
                |_| {
                    Err::<(), _>(super::super::RunnerError::task_invocation(
                        "gateway domain `api.app.test` is claimed by another checkout",
                    ))
                },
            )
        });
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("✗ routes registered failed: gateway domain `api.app.test`"),
            "{lines:?}"
        );
    }
}
