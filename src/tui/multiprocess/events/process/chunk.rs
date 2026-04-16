use crate::process_manager::{ProcessEvent, ProcessEventKind};

use super::super::super::diagnostics::RuntimeDiagnostics;
use super::super::super::state::SessionState;
use effigy_tui::terminal_text::config::{VT_PARSER_COLS, VT_PARSER_ROWS, VT_PARSER_SCROLLBACK};

pub(super) fn handle_chunk_event_impl(
    event_item: &ProcessEvent,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    vt_emulator_enabled: bool,
) {
    let had_output = state.mark_process_received_output(&event_item.process);
    if !vt_emulator_enabled {
        return;
    }
    if !had_output {
        state.vt_parsers.insert(
            event_item.process.clone(),
            vt100::Parser::new(VT_PARSER_ROWS, VT_PARSER_COLS, VT_PARSER_SCROLLBACK),
        );
        state.set_vt_saw_chunk_for(&event_item.process, false);
        diagnostics.record_vt_reset(&event_item.process);
    }
    let Some(chunk) = event_item.chunk.as_ref() else {
        return;
    };
    let Some(parser) = state.vt_parser_mut_for(&event_item.process) else {
        return;
    };
    parser.process(chunk);
    state.set_vt_saw_chunk_for(&event_item.process, true);
    match event_item.kind {
        ProcessEventKind::StdoutChunk => {
            diagnostics.record_stdout_chunk(&event_item.process, chunk.len())
        }
        ProcessEventKind::StderrChunk => {
            diagnostics.record_stderr_chunk(&event_item.process, chunk.len())
        }
        _ => {}
    }
}
