use std::path::PathBuf;

use crate::process_manager::ProcessSupervisor;

pub(super) fn empty_supervisor() -> ProcessSupervisor {
    ProcessSupervisor::spawn(PathBuf::from("."), Vec::new()).expect("spawn empty supervisor")
}

#[path = "shell_shortcuts_tests.rs"]
mod shell_shortcuts_tests;
#[path = "pre_dispatch_tests.rs"]
mod pre_dispatch_tests;
#[path = "command_dispatch_tests.rs"]
mod command_dispatch_tests;
