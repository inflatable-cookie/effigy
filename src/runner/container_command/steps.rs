//! The live steps a container command shows.
//!
//! The per-service driver lives in `effigy_containers::exec` so `container up`
//! and `container down` share one implementation. This module adds the
//! runner-specific phase helper: a step around one piece of work that ends
//! with its own failure sentence.

pub(super) use effigy_containers::exec::{ComposeSteps, TimedStep};
pub(super) use effigy_containers::policy::restart::RestartPolicyStage;
pub(super) use effigy_ui::steps::ProgressSource;

/// Name one stage of the restart-policy apply now in flight: the Compose
/// declaration read, the runtime inventory read, or the update itself. A
/// stalled read names that read instead of claiming an update was sent, and
/// never names the Compose call that already finished (effigy#460, Q-017).
pub(super) fn restart_policy_stage(
    progress: &ProgressSource,
    subject: &str,
    stage: RestartPolicyStage,
    count: Option<(u64, u64)>,
) {
    // `now_waiting_for` drops the previous stage's event and count, so the
    // frame names only the stage now running; the stage's own sentence is the
    // frame's subject and the wait behind it is the policy being applied.
    progress.now_waiting_for(format!("`restart: unless-stopped` on {subject}"));
    progress.update(stage.description(subject), count, None);
}

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
