use effigy_ui::steps::{self, ProgressFrame, ProgressSource, Step};

use crate::EffigyRuntimeError;

pub(super) const PROBE_VOLUME_INSPECT: &str = "runtime volume inspect";
pub(super) const PROBE_VOLUME_REMOVE: &str = "runtime volume remove";

/// Live progress for cache and volume list/prune. Labels name the current
/// volume or inspect batch, known count, elapsed wait and last event or the
/// exact runtime probe.
pub(super) struct CacheVolumeProgress {
    source: ProgressSource,
    step: Step,
}

impl CacheVolumeProgress {
    pub(super) fn start(probe: impl Into<String>) -> Self {
        let source = ProgressSource::new(
            ProgressFrame::default()
                .label("Inspecting volumes")
                .waiting_for(probe),
        );
        let step = steps::live_step(source.clone());
        Self { source, step }
    }

    pub(super) fn run<T>(
        probe: impl Into<String>,
        work: impl FnOnce(&Self) -> Result<T, EffigyRuntimeError>,
    ) -> Result<T, EffigyRuntimeError> {
        let progress = Self::start(probe);
        match work(&progress) {
            Ok(value) => {
                progress.step.finish_clear();
                Ok(value)
            }
            Err(error) => {
                progress.fail_error(&error);
                Err(error)
            }
        }
    }

    pub(super) fn update(&self, build: impl FnOnce(&ProgressFrame) -> ProgressFrame) {
        self.source.update(build);
    }

    fn fail_error(&self, error: &EffigyRuntimeError) {
        let cause = error
            .to_string()
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        self.step.fail(&cause);
    }

    #[cfg(test)]
    pub(super) fn frame_at(&self, elapsed: std::time::Duration) -> String {
        self.source.frame_at(elapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cleanup_frames_name_the_volume_count_elapsed_and_last_event() {
        let ((), lines) = steps::capture_lines(|| {
            let progress = CacheVolumeProgress::start(PROBE_VOLUME_INSPECT);
            assert!(progress
                .source
                .detailed_frame_at(Duration::ZERO)
                .contains(PROBE_VOLUME_INSPECT));
            assert_eq!(progress.frame_at(Duration::ZERO), "Inspecting volumes");
            progress.update(|_| {
                ProgressFrame::new("inspecting cache volumes for `/workspace/private`")
                    .label("Inspecting cache volumes")
                    .count(Some((1, 2)))
                    .event(Some("listed 2 cache volumes".to_owned()))
            });
            assert_eq!(
                progress.frame_at(Duration::from_secs(3)),
                "Inspecting cache volumes (1/2) · listed 2 cache volumes · 3s"
            );
            let detail = progress.source.detailed_frame_at(Duration::from_secs(3));
            assert!(detail.contains("/workspace/private"));
            assert!(detail.contains("listed 2 cache volumes"));
            progress.step.finish_clear();
        });
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn a_blocked_inspect_names_the_exact_probe() {
        let ((), lines) = steps::capture_lines(|| {
            let progress = CacheVolumeProgress::start(PROBE_VOLUME_INSPECT);
            assert_eq!(
                progress.frame_at(Duration::from_secs(12)),
                "Inspecting volumes · 12s"
            );
            progress.fail_error(&EffigyRuntimeError::task_invocation(
                "runtime volume inspect timed out",
            ));
        });
        assert_eq!(lines, vec!["✗ runtime volume inspect timed out".to_owned()]);
    }
}
