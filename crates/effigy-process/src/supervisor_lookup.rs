use std::process::Child;
use std::sync::{Arc, Mutex};

use super::{lifecycle, ProcessManagerError, ProcessSupervisor, PROCESS_GRACEFUL_STOP_TIMEOUT};

impl ProcessSupervisor {
    pub(super) fn child_handle(&self, process: &str) -> Option<Arc<Mutex<Child>>> {
        let processes = crate::locks::lock_tolerant(&self.processes);
        processes.get(process).cloned()
    }

    pub(super) fn required_child_handle(
        &self,
        process: &str,
    ) -> Result<Arc<Mutex<Child>>, ProcessManagerError> {
        self.child_handle(process)
            .ok_or_else(|| ProcessManagerError::ProcessNotFound {
                process: process.to_owned(),
            })
    }

    pub(super) fn all_child_handles(&self) -> Vec<Arc<Mutex<Child>>> {
        let processes = crate::locks::lock_tolerant(&self.processes);
        processes.values().cloned().collect()
    }

    pub(super) fn terminate_child_graceful(
        &self,
        process: &str,
    ) -> Result<(), ProcessManagerError> {
        let child = self.required_child_handle(process)?;
        lifecycle::terminate_child_graceful(&child, PROCESS_GRACEFUL_STOP_TIMEOUT);
        Ok(())
    }
}
