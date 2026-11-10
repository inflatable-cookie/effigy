use effigy_ui::steps::{self, ProgressSource, Step};

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
        let source = ProgressSource::waiting_for(probe);
        source.label("Inspecting volumes");
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

    pub(super) fn now_waiting_for(&self, probe: impl Into<String>) {
        self.source.now_waiting_for(probe);
    }

    pub(super) fn update(
        &self,
        sub_step: impl Into<String>,
        count: Option<(u64, u64)>,
        event: Option<String>,
    ) {
        let sub_step = sub_step.into();
        let label = if sub_step.starts_with("inspecting cache volumes for ") {
            Some("Inspecting cache volumes")
        } else if sub_step.starts_with("inspecting volumes for ") {
            Some("Inspecting volumes")
        } else {
            None
        };
        self.source.update(sub_step, count, event);
        if let Some(label) = label {
            self.source.label(label);
        }
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
    fn repository_paths_stay_in_detailed_volume_frames() {
        let progress = CacheVolumeProgress::start(PROBE_VOLUME_INSPECT);
        progress.update(
            "inspecting cache volumes for `/workspace/private`",
            Some((1, 2)),
            None,
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(3)),
            "Inspecting cache volumes (1/2) · 3s"
        );
        assert!(progress
            .source
            .detailed_frame_at(Duration::from_secs(3))
            .contains("/workspace/private"));
        progress.step.finish_clear();
    }

    #[test]
    fn cleanup_frames_name_the_volume_count_elapsed_and_last_event() {
        let ((), lines) = steps::capture_lines(|| {
            let progress = CacheVolumeProgress::start(PROBE_VOLUME_INSPECT);
            assert!(progress
                .source
                .detailed_frame_at(Duration::from_secs(3))
                .contains(PROBE_VOLUME_INSPECT));
            assert_eq!(
                progress.frame_at(Duration::from_secs(0)),
                "Inspecting volumes"
            );
            progress.update("inspecting cache volumes", None, None);
            assert_eq!(
                progress.frame_at(Duration::from_secs(3)),
                "inspecting cache volumes · 3s"
            );
            progress.now_waiting_for(PROBE_VOLUME_REMOVE);
            progress.update(
                "removing cache volume `web-target`",
                Some((1, 3)),
                Some("listed 3 cache volumes".to_owned()),
            );
            assert_eq!(
                progress.frame_at(Duration::from_secs(5)),
                "removing cache volume `web-target` (1/3) · listed 3 cache volumes · 5s"
            );
            progress.update(
                "removing volume `orphaned-data`",
                Some((2, 3)),
                Some("removed `web-target`".to_owned()),
            );
            assert_eq!(
                progress.frame_at(Duration::from_secs(8)),
                "removing volume `orphaned-data` (2/3) · removed `web-target` · 8s"
            );
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
