use std::path::PathBuf;

use crate::multiprocess::state::SessionState;
use effigy_process::ProcessSupervisor;

pub(super) fn empty_supervisor() -> ProcessSupervisor {
    ProcessSupervisor::spawn(PathBuf::from("."), Vec::new()).expect("spawn empty supervisor")
}

pub(super) fn state_with_processes(processes: &[&str]) -> SessionState {
    SessionState::new(
        ".".into(),
        processes.iter().map(|name| (*name).to_owned()).collect(),
        2000,
        240,
        8000,
    )
}

#[path = "dispatch_tests.rs"]
mod dispatch_tests;
#[path = "navigation_tests.rs"]
mod navigation_tests;
