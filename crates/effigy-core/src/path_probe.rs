use std::ffi::OsString;
use std::path::Path;

pub fn command_available_in_path(command: &str) -> bool {
    command_available_in_path_with(command, std::env::var_os("PATH"))
}

fn command_available_in_path_with(command: &str, raw_paths: Option<OsString>) -> bool {
    let Some(paths) = raw_paths else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| command_available_in_dir(command, &dir))
}

fn command_available_in_dir(command: &str, dir: &Path) -> bool {
    let candidate = dir.join(command);
    if candidate.is_file() {
        return true;
    }
    #[cfg(windows)]
    {
        let exe = dir.join(format!("{command}.exe"));
        if exe.is_file() {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "path_probe/tests.rs"]
mod tests;
