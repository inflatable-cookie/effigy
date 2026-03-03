use std::io::Write;

use super::{lifecycle, signal, ProcessManagerError, ProcessSupervisor};

impl ProcessSupervisor {
    pub fn send_input(&self, process: &str, input: &str) -> Result<(), ProcessManagerError> {
        let child = self.child_handle(process);
        let Some(child) = child else {
            return Ok(());
        };
        let mut child = child.lock().expect("child lock");
        let Some(stdin) = child.stdin.as_mut() else {
            return Err(ProcessManagerError::MissingStdio {
                process: process.to_owned(),
            });
        };
        stdin
            .write_all(input.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|error| ProcessManagerError::InputWrite {
                process: process.to_owned(),
                error,
            })
    }

    pub fn terminate_all(&self) {
        let children = self.all_child_handles();
        for child in children {
            signal::send_kill(&mut child.lock().expect("child lock"));
        }
    }

    pub fn terminate_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        self.terminate_child_graceful(process)
    }

    pub fn restart_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        let spec = self
            .specs
            .get(process)
            .cloned()
            .ok_or_else(|| ProcessManagerError::ProcessNotFound {
                process: process.to_owned(),
            })?;
        if self.child_handle(process).is_some() {
            self.terminate_child_graceful(process)?;
        }
        let mut restart_spec = spec;
        restart_spec.start_after_ms = 0;
        let replacement = lifecycle::spawn_process_instance(&restart_spec, &self.events_tx, false)?;
        let mut processes = self.processes.lock().expect("process map lock");
        processes.insert(process.to_owned(), replacement);
        Ok(())
    }
}
