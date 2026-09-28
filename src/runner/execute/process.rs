use std::process::Command as ProcessCommand;

#[cfg(unix)]
use std::sync::{Arc, Mutex};

use super::context::ExecutionTaskContext;
use crate::runner::error::RunnerError;
use effigy_core::shell::with_local_node_bin_path;
use effigy_env::secret::SecretString;

#[cfg(unix)]
pub(super) struct ChildProcessGroupSignalForwarder {
    state: Arc<Mutex<ChildProcessSignalState>>,
    handle: signal_hook::iterator::Handle,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(not(unix))]
pub(super) struct ChildProcessGroupSignalForwarder;

#[cfg(unix)]
#[derive(Default)]
struct ChildProcessSignalState {
    process_group: Option<i32>,
    pending: Vec<i32>,
}

impl ChildProcessGroupSignalForwarder {
    pub(super) fn install() -> std::io::Result<Option<Self>> {
        if super::super::admission::scoped_lease_id().is_none() {
            return Ok(None);
        }

        #[cfg(unix)]
        {
            use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
            use signal_hook::iterator::Signals;

            let mut signals = Signals::new([SIGHUP, SIGINT, SIGTERM])?;
            let handle = signals.handle();
            let state = Arc::new(Mutex::new(ChildProcessSignalState::default()));
            let thread_state = state.clone();
            let thread = std::thread::Builder::new()
                .name("effigy-child-signal-forwarder".to_owned())
                .spawn(move || {
                    for signal in signals.forever() {
                        let process_group = {
                            let mut state = thread_state
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            match state.process_group {
                                Some(process_group) => Some(process_group),
                                None => {
                                    state.pending.push(signal);
                                    None
                                }
                            }
                        };
                        if let Some(process_group) = process_group {
                            forward_signal_to_process_group(process_group, signal);
                        }
                    }
                })?;
            Ok(Some(Self {
                state,
                handle,
                thread: Some(thread),
            }))
        }

        #[cfg(not(unix))]
        {
            Ok(None)
        }
    }

    pub(super) fn attach(&mut self, child_pid: u32) {
        #[cfg(unix)]
        {
            let pending = {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.process_group = Some(child_pid as i32);
                std::mem::take(&mut state.pending)
            };
            for signal in pending {
                forward_signal_to_process_group(child_pid as i32, signal);
            }
        }

        #[cfg(not(unix))]
        let _ = child_pid;
    }
}

#[cfg(unix)]
impl Drop for ChildProcessGroupSignalForwarder {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(unix)]
fn forward_signal_to_process_group(process_group: i32, signal: i32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

    let signal = match signal {
        SIGHUP => Signal::SIGHUP,
        SIGINT => Signal::SIGINT,
        SIGTERM => Signal::SIGTERM,
        _ => return,
    };
    let _ = kill(Pid::from_raw(-process_group), signal);
}

pub(super) fn build_shell_process(
    context: &ExecutionTaskContext<'_>,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> ProcessCommand {
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-c")
        .arg(context.command())
        .current_dir(context.repo_for_task());
    with_local_node_bin_path(&mut process, context.repo_for_task());
    if let Some(secrets) = secret_env {
        for (key, secret) in secrets {
            process.env(key, secret.expose());
        }
    }
    if let Some(lease_id) = super::super::admission::scoped_lease_id() {
        process.env("EFFIGY_ADMISSION_LEASE_ID", lease_id);
        if let Some(cpu_units) = super::super::admission::scoped_cpu_units() {
            process.env("CARGO_BUILD_JOBS", cpu_units.to_string());
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            process.process_group(0);
        }
    }
    process
}

pub(super) fn command_launch_error(
    context: &ExecutionTaskContext<'_>,
    error: std::io::Error,
) -> RunnerError {
    RunnerError::TaskCommandLaunch {
        command: context.command().to_owned(),
        error,
    }
}
