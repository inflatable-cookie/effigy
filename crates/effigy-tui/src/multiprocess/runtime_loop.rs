use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};

use super::config::{INPUT_POLL_WAIT, MAX_EVENTS_PER_TICK};
use super::events::{drain_process_events, handle_key_event, KeyEventContext, LoopControl};
use super::render::{render_ui, RenderUiState};
use super::view_model::build_active_view_model;
use super::{MultiProcessTuiError, MultiProcessTuiOptions, SessionRuntime};

#[cfg(test)]
pub(super) fn run_event_loop(
    runtime: &mut SessionRuntime<'_>,
    options: MultiProcessTuiOptions,
) -> Result<(), MultiProcessTuiError> {
    run_event_loop_with_control(runtime, options, None).map(|_| ())
}

pub(super) fn run_event_loop_with_control(
    runtime: &mut SessionRuntime<'_>,
    options: MultiProcessTuiOptions,
    control: Option<&super::MultiProcessTuiControl<'_>>,
) -> Result<bool, MultiProcessTuiError> {
    loop {
        if let Some(control) = control {
            if (control.poll)(runtime.supervisor.as_ref()).map_err(MultiProcessTuiError::Control)? {
                return Ok(true);
            }
        }
        drain_process_events(
            runtime.supervisor.as_ref(),
            &mut runtime.state,
            &mut runtime.diagnostics,
            MAX_EVENTS_PER_TICK,
            runtime.vt_emulator_enabled,
        );
        // The session's own entry state is authoritative: it names what each
        // entry waits for, so the tab never borrows another entry's label. The
        // snapshot is replaced, never merged, so a wait that settled — a
        // listener that became ready, the run's own probe after it answered —
        // leaves its tab instead of lingering (effigy#458).
        runtime
            .state
            .apply_entry_waits(runtime.supervisor.entry_waits());
        if runtime.state.shutdown_requested
            && !runtime.state.startup_failed
            && runtime.supervisor.startup_finished()
            && startup_states_settled(runtime)
        {
            break;
        }
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

        let drawn = runtime.terminal.draw(|frame| {
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
                    active_startup_state: runtime
                        .state
                        .startup_states
                        .get(&active_view.active_process)
                        .copied()
                        .unwrap_or(super::state::ProcessStartupState::Running),
                    active_vt: active_view.active_vt,
                    input_line: &runtime.state.input_line,
                    input_mode: runtime.state.input_mode,
                    shell_capture_mode: runtime.state.shell_capture_mode,
                    exit_states: &runtime.state.exit_states,
                    startup_states: &runtime.state.startup_states,
                    entry_waits: &runtime.state.entry_waits,
                    active_entry_wait: runtime.state.entry_waits.get(&active_view.active_process),
                    show_help: runtime.state.show_help,
                    show_options: runtime.state.show_options,
                    options_index: runtime.state.options_index,
                    footer_message: runtime.state.footer_message.as_deref(),
                    active_output_seen: active_view.active_output_seen,
                    spinner_tick: runtime.state.spinner_tick,
                    active_elapsed: active_view.active_elapsed,
                    active_restart_count: active_view.active_restart_count,
                    shell_cursor: active_view.shell_cursor,
                },
            )
        });
        if let Err(error) = drawn {
            // A write to a closed terminal can fail before its input reports
            // the hangup; a hung-up input says the terminal is gone.
            if matches!(terminal_input(Duration::ZERO), TerminalInput::HungUp) {
                runtime.terminal_lost = true;
                return Ok(true);
            }
            return Err(error.into());
        }
        runtime.diagnostics.record_frame();
        if runtime.state.shutdown_requested
            && (runtime.state.startup_failed
                || (runtime.supervisor.startup_finished() && startup_states_settled(runtime)))
        {
            break;
        }

        // Every event crossterm already read, then whatever the terminal has.
        // A closed terminal ends the session like a quit: the caller's
        // shutdown stops and settles what the session started.
        let mut wait = INPUT_POLL_WAIT;
        loop {
            if matches!(terminal_input(wait), TerminalInput::HungUp) {
                runtime.terminal_lost = true;
                return Ok(true);
            }
            wait = Duration::ZERO;
            if !event::poll(Duration::ZERO)? {
                break;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match handle_key_event(
                &key,
                KeyEventContext {
                    supervisor: runtime.supervisor.as_ref(),
                    state: &mut runtime.state,
                    diagnostics: &mut runtime.diagnostics,
                    options,
                    max_offset: active_view.max_offset,
                },
            )? {
                LoopControl::Continue => {}
                LoopControl::Quit => return Ok(true),
            }
        }
    }
    Ok(false)
}

enum TerminalInput {
    Ready,
    Idle,
    HungUp,
}

/// Waits up to `wait` for terminal input without letting crossterm wait on
/// it: crossterm's tty reader retries a zero-byte read forever, so after the
/// terminal closes (POLLHUP) any crossterm poll spins. `Idle` with a zero
/// wait still lets crossterm hand out events it already parsed.
fn terminal_input(wait: Duration) -> TerminalInput {
    // SAFETY: isatty on this process's standard input.
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
        // crossterm reads /dev/tty instead; it alone can wait on it.
        return match event::poll(wait) {
            Ok(true) => TerminalInput::Ready,
            _ => TerminalInput::Idle,
        };
    }
    terminal_input_on(libc::STDIN_FILENO, wait)
}

fn terminal_input_on(fd: libc::c_int, wait: Duration) -> TerminalInput {
    let mut input = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = i32::try_from(wait.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: one initialized pollfd for this process's standard input.
    let ready = unsafe { libc::poll(&mut input, 1, millis) };
    if ready <= 0 {
        return TerminalInput::Idle;
    }
    if input.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
        TerminalInput::HungUp
    } else {
        TerminalInput::Ready
    }
}

fn startup_states_settled(runtime: &SessionRuntime<'_>) -> bool {
    runtime.state.pending_startup == 0
}

#[cfg(test)]
mod terminal_input_tests {
    use super::*;

    /// A closed terminal reads as hangup, never as idle input, so the loop
    /// ends instead of handing crossterm a reader that spins on EOF.
    #[test]
    #[cfg_attr(
        effigy_shared_process,
        ignore = "hangup needs every master reference closed, which a shared test process cannot promise; nextest runs this proof in its own process"
    )]
    fn terminal_input_reports_idle_ready_and_hangup_on_a_real_pty() {
        // Close-on-exec from birth: other tests in a shared process spawn
        // children, and one that inherited the master would keep it open.
        // SAFETY: opens a new PTY master this test owns and closes.
        let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
        assert!(master >= 0, "private PTY");
        // SAFETY: grant and unlock the slave of the master this test owns.
        assert_eq!(unsafe { libc::grantpt(master) }, 0);
        assert_eq!(unsafe { libc::unlockpt(master) }, 0);
        let mut name = [0 as libc::c_char; 128];
        #[cfg(target_os = "linux")]
        // SAFETY: the reentrant name lookup writes into this test's buffer.
        let named = unsafe { libc::ptsname_r(master, name.as_mut_ptr(), name.len()) } == 0;
        #[cfg(target_os = "macos")]
        // SAFETY: TIOCPTYGNAME (_IOC(IOC_OUT, 't', 83, 128)) writes the slave
        // name into this 128-byte buffer; ptsname's static one is shared.
        let named = unsafe { libc::ioctl(master, 0x4080_7453, name.as_mut_ptr()) } == 0;
        assert!(named, "PTY slave name");
        // SAFETY: opens the NUL-terminated slave path the kernel just named.
        let slave = unsafe {
            libc::open(
                name.as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
            )
        };
        assert!(slave >= 0, "PTY slave");
        assert!(matches!(
            terminal_input_on(slave, Duration::ZERO),
            TerminalInput::Idle
        ));
        // A whole line: the fresh slave is canonical, unlike the TUI's raw one.
        // SAFETY: a two-byte write to the live master this test owns.
        assert_eq!(unsafe { libc::write(master, b"x\n".as_ptr().cast(), 2) }, 2);
        assert!(matches!(
            terminal_input_on(slave, Duration::from_millis(500)),
            TerminalInput::Ready
        ));
        // SAFETY: closes the master this test owns: the terminal goes away.
        unsafe { libc::close(master) };
        assert!(matches!(
            terminal_input_on(slave, Duration::from_millis(500)),
            TerminalInput::HungUp
        ));
        // SAFETY: closes the slave this test owns.
        unsafe { libc::close(slave) };
    }
}
