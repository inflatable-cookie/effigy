use std::path::PathBuf;

use effigy_process::ProcessSupervisor;

pub(super) fn empty_supervisor() -> ProcessSupervisor {
    ProcessSupervisor::spawn(PathBuf::from("."), Vec::new()).expect("spawn empty supervisor")
}

#[path = "command_dispatch_tests.rs"]
mod command_dispatch_tests;
#[path = "pre_dispatch_tests.rs"]
mod pre_dispatch_tests;
#[path = "shell_shortcuts_tests.rs"]
mod shell_shortcuts_tests;
