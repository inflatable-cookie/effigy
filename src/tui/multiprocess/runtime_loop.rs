use crossterm::event::{self, Event, KeyEventKind};

use super::config::{INPUT_POLL_WAIT, MAX_EVENTS_PER_TICK};
use super::events::{drain_process_events, handle_key_event, KeyEventContext, LoopControl};
use super::render::{render_ui, RenderUiState};
use super::view_model::build_active_view_model;
use super::{MultiProcessTuiError, MultiProcessTuiOptions, SessionRuntime};

pub(super) fn run_event_loop(
    runtime: &mut SessionRuntime,
    options: MultiProcessTuiOptions,
) -> Result<(), MultiProcessTuiError> {
    loop {
        drain_process_events(
            &runtime.supervisor,
            &mut runtime.state,
            &mut runtime.diagnostics,
            MAX_EVENTS_PER_TICK,
            runtime.vt_emulator_enabled,
        );
        runtime.state.spinner_tick = runtime.state.spinner_tick.wrapping_add(1);

        let size = runtime.terminal.size()?;
        let output_height = size.height.saturating_sub(9) as usize;
        let output_width = size.width.saturating_sub(4) as usize;
        let active_view = build_active_view_model(
            &mut runtime.state,
            output_height,
            output_width,
            runtime.vt_emulator_enabled,
        );

        runtime.terminal.draw(|frame| {
            render_ui(
                frame,
                RenderUiState {
                    process_names: &runtime.state.process_names,
                    active_index: runtime.state.active_index,
                    active_logs: &active_view.active_logs,
                    scroll_offset: active_view.scroll_offset,
                    max_offset: active_view.max_offset,
                    render_scroll_offset: active_view.render_scroll_offset,
                    scrollbar_total: active_view.scrollbar_total,
                    follow: active_view.is_follow,
                    active_process: &active_view.active_process,
                    input_line: &runtime.state.input_line,
                    input_mode: runtime.state.input_mode,
                    shell_capture_mode: runtime.state.shell_capture_mode,
                    exit_states: &runtime.state.exit_states,
                    show_help: runtime.state.show_help,
                    show_options: runtime.state.show_options,
                    options_index: runtime.state.options_index,
                    active_output_seen: active_view.active_output_seen,
                    spinner_tick: runtime.state.spinner_tick,
                    active_elapsed: active_view.active_elapsed,
                    active_restart_count: active_view.active_restart_count,
                    shell_cursor: active_view.shell_cursor,
                },
            )
        })?;
        runtime.diagnostics.record_frame();

        if !event::poll(INPUT_POLL_WAIT)? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match handle_key_event(
                &key,
                KeyEventContext {
                    supervisor: &runtime.supervisor,
                    state: &mut runtime.state,
                    diagnostics: &mut runtime.diagnostics,
                    options,
                    max_offset: active_view.max_offset,
                },
            )? {
                LoopControl::Continue => {}
                LoopControl::Quit => break,
            }
        }
    }
    Ok(())
}
