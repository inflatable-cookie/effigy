use crate::process_manager::{ProcessEvent, ProcessEventKind};
use crate::ui::{
    KeyValue, MessageBlock, NoticeLevel, Renderer, SpinnerHandle, StepState, SummaryCounts,
    TableSpec, UiResult,
};

use super::StreamState;

#[derive(Default)]
struct RecordingRenderer {
    lines: Vec<String>,
    notices: Vec<String>,
}

struct TestSpinner;

impl SpinnerHandle for TestSpinner {
    fn set_message(&self, _message: &str) {}
    fn finish_success(&self, _message: &str) {}
    fn finish_error(&self, _message: &str) {}
}

impl Renderer for RecordingRenderer {
    fn text(&mut self, body: &str) -> UiResult<()> {
        self.lines.push(body.to_owned());
        Ok(())
    }

    fn section(&mut self, _title: &str) -> UiResult<()> {
        Ok(())
    }

    fn notice(&mut self, _level: NoticeLevel, body: &str) -> UiResult<()> {
        self.notices.push(body.to_owned());
        Ok(())
    }

    fn bullet_list(&mut self, _title: &str, _items: &[String]) -> UiResult<()> {
        Ok(())
    }

    fn success_block(&mut self, _block: &MessageBlock) -> UiResult<()> {
        Ok(())
    }

    fn error_block(&mut self, _block: &MessageBlock) -> UiResult<()> {
        Ok(())
    }

    fn warning_block(&mut self, _block: &MessageBlock) -> UiResult<()> {
        Ok(())
    }

    fn key_values(&mut self, _items: &[KeyValue]) -> UiResult<()> {
        Ok(())
    }

    fn step(&mut self, _label: &str, _state: StepState) -> UiResult<()> {
        Ok(())
    }

    fn summary(&mut self, _counts: SummaryCounts) -> UiResult<()> {
        Ok(())
    }

    fn table(&mut self, _spec: &TableSpec) -> UiResult<()> {
        Ok(())
    }

    fn spinner(&mut self, _label: &str) -> UiResult<Box<dyn SpinnerHandle>> {
        Ok(Box::new(TestSpinner))
    }
}

fn event(process: &str, kind: ProcessEventKind, payload: &str) -> ProcessEvent {
    ProcessEvent {
        process: process.to_owned(),
        kind,
        payload: payload.to_owned(),
        chunk: None,
    }
}

#[test]
fn stream_state_records_stdout_and_stderr_lines() {
    let mut state = StreamState::default();
    let mut renderer = RecordingRenderer::default();

    state
        .record_event(
            event("api", ProcessEventKind::Stdout, "ready"),
            &mut renderer,
        )
        .expect("stdout should render");
    state
        .record_event(
            event("jobs", ProcessEventKind::Stderr, "failed to bind"),
            &mut renderer,
        )
        .expect("stderr should render");

    assert_eq!(
        renderer.lines,
        vec![
            "[api] ready".to_owned(),
            "[jobs stderr] failed to bind".to_owned(),
        ]
    );
}

#[test]
fn stream_state_records_non_zero_exits_and_exit_notices() {
    let mut state = StreamState::default();
    let mut renderer = RecordingRenderer::default();

    state
        .record_event(
            event("api", ProcessEventKind::Exit, "exit=0"),
            &mut renderer,
        )
        .expect("successful exit should render notice");
    state
        .record_event(
            event("jobs", ProcessEventKind::Exit, "exit=7"),
            &mut renderer,
        )
        .expect("failed exit should render notice");

    assert_eq!(state.exit_count, 2);
    assert_eq!(
        state.finish(),
        vec![("jobs".to_owned(), "exit=7".to_owned())]
    );
    assert_eq!(
        renderer.notices,
        vec![
            "process `api` exit=0".to_owned(),
            "process `jobs` exit=7".to_owned(),
        ]
    );
}

#[test]
fn stream_state_only_drains_after_all_expected_exits() {
    let mut state = StreamState::default();

    state.record_idle_tick(1);
    assert_eq!(state.drained_after_exit, 0);

    state.exit_count = 1;
    state.record_idle_tick(1);
    assert_eq!(state.drained_after_exit, 1);
}
