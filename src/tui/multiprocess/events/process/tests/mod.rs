use crate::tui::multiprocess::diagnostics::RuntimeDiagnostics;
use crate::tui::multiprocess::state::SessionState;
use effigy_process::{ProcessEvent, ProcessEventKind};

pub(super) fn state_with_process(name: &str) -> SessionState {
    SessionState::new(vec![name.to_owned()], 2000, 240, 8000)
}

pub(super) fn diagnostics() -> RuntimeDiagnostics {
    RuntimeDiagnostics::from_env()
}

pub(super) fn process_event(
    process: &str,
    kind: ProcessEventKind,
    payload: &str,
    chunk: Option<Vec<u8>>,
) -> ProcessEvent {
    ProcessEvent {
        process: process.to_owned(),
        kind,
        payload: payload.to_owned(),
        chunk,
    }
}

#[path = "chunk_tests.rs"]
mod chunk_tests;
#[path = "exit_tests.rs"]
mod exit_tests;
#[path = "line_tests.rs"]
mod line_tests;
