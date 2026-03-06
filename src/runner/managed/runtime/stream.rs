use std::time::Duration;

use crate::process_manager::{ProcessEvent, ProcessEventKind, ProcessSupervisor};
use crate::ui::{NoticeLevel, Renderer};

use super::super::super::RunnerError;

const STREAM_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const STREAM_DRAIN_POLLS_AFTER_EXIT: usize = 3;

pub(super) fn collect_stream_non_zero_exits(
    supervisor: &ProcessSupervisor,
    expected: usize,
    renderer: &mut impl Renderer,
) -> Result<Vec<(String, String)>, RunnerError> {
    let mut state = StreamState::default();
    while state.exit_count < expected || state.drained_after_exit < STREAM_DRAIN_POLLS_AFTER_EXIT {
        if let Some(event) = supervisor.next_event_timeout(STREAM_EVENT_POLL_INTERVAL) {
            state.record_event(event, renderer)?;
        } else {
            state.record_idle_tick(expected);
        }
    }

    Ok(state.finish())
}

#[derive(Default)]
struct StreamState {
    exit_count: usize,
    drained_after_exit: usize,
    non_zero_exits: Vec<(String, String)>,
}

impl StreamState {
    fn record_event(
        &mut self,
        event: ProcessEvent,
        renderer: &mut impl Renderer,
    ) -> Result<(), RunnerError> {
        if self.exit_count > 0 {
            self.drained_after_exit = 0;
        }
        match event.kind {
            ProcessEventKind::Stdout => {
                renderer.text(&format!("[{}] {}", event.process, event.payload))?;
            }
            ProcessEventKind::Stderr => {
                renderer.text(&format!("[{} stderr] {}", event.process, event.payload))?;
            }
            ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
            ProcessEventKind::Exit => self.record_exit(event, renderer)?,
        }
        Ok(())
    }

    fn record_exit(
        &mut self,
        event: ProcessEvent,
        renderer: &mut impl Renderer,
    ) -> Result<(), RunnerError> {
        self.exit_count += 1;
        if event.payload != "exit=0" {
            self.non_zero_exits
                .push((event.process.clone(), event.payload.clone()));
        }
        renderer.notice(
            NoticeLevel::Info,
            &format!("process `{}` {}", event.process, event.payload),
        )?;
        Ok(())
    }

    fn record_idle_tick(&mut self, expected: usize) {
        if self.exit_count >= expected {
            self.drained_after_exit += 1;
        }
    }

    fn finish(self) -> Vec<(String, String)> {
        self.non_zero_exits
    }
}

#[cfg(test)]
#[path = "stream/tests.rs"]
mod tests;
