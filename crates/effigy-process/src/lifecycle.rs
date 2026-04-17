use std::process::Child;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{ProcessEvent, ProcessManagerError, ProcessSpec};

mod monitor;
mod shutdown;
mod spawn;

pub(super) fn spawn_process_instance(
    spec: &ProcessSpec,
    events_tx: &Sender<ProcessEvent>,
    honor_start_delay: bool,
) -> Result<Arc<Mutex<Child>>, ProcessManagerError> {
    spawn::spawn_process_instance(spec, events_tx, honor_start_delay)
}

pub(super) fn terminate_child_graceful(child: &Arc<Mutex<Child>>, timeout: Duration) {
    shutdown::terminate_child_graceful(child, timeout);
}
